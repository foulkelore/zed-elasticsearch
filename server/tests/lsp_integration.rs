//! Integration test: drive the compiled language server over stdio.
//!
//! The unit tests in `analysis.rs` cover the pure lint logic, but nothing
//! exercises `main.rs` — the LSP wiring that frames JSON-RPC over stdin/stdout,
//! completes the `initialize` handshake, and turns lints into
//! `publishDiagnostics` notifications. This test spawns the real binary (Cargo
//! builds it and hands us its path via `CARGO_BIN_EXE_*`) and speaks LSP to it,
//! asserting the handshake plus a `didOpen` -> `publishDiagnostics` round-trip
//! for both a faulty and a clean document.

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use serde_json::{json, Value};

/// Wrap a JSON-RPC message in LSP `Content-Length` framing.
fn frame(message: &Value) -> Vec<u8> {
    let body = serde_json::to_vec(message).expect("serialize message");
    let mut bytes = format!("Content-Length: {}\r\n\r\n", body.len()).into_bytes();
    bytes.extend_from_slice(&body);
    bytes
}

/// Read exactly one framed LSP message: parse headers for `Content-Length`,
/// then read that many bytes of JSON body.
fn read_message(reader: &mut impl BufRead) -> Value {
    let mut content_length = None;
    loop {
        let mut line = String::new();
        let read = reader.read_line(&mut line).expect("read header line");
        assert_ne!(read, 0, "server closed stdout before sending a full message");
        let line = line.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            break; // blank line terminates the header block
        }
        if let Some(value) = line.strip_prefix("Content-Length:") {
            content_length =
                Some(value.trim().parse::<usize>().expect("parse Content-Length"));
        }
    }
    let length = content_length.expect("message had no Content-Length header");
    let mut body = vec![0u8; length];
    reader.read_exact(&mut body).expect("read message body");
    serde_json::from_slice(&body).expect("parse JSON body")
}

/// Read messages until one satisfies `predicate`, returning it. Skips unrelated
/// traffic such as the `window/logMessage` the server emits after `initialized`.
fn read_until(reader: &mut impl BufRead, predicate: impl Fn(&Value) -> bool) -> Value {
    loop {
        let message = read_message(reader);
        if predicate(&message) {
            return message;
        }
    }
}

#[test]
fn initialize_handshake_and_diagnostics_round_trip() {
    let binary = env!("CARGO_BIN_EXE_elasticsearch-language-server");
    let mut server = Command::new(binary)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn language server");

    let mut stdin = server.stdin.take().expect("server stdin");
    let stdout = server.stdout.take().expect("server stdout");

    // Run the conversation on a worker thread so a misbehaving server cannot
    // hang the test forever — the main thread enforces a timeout and kills the
    // child if the worker does not report back in time.
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let mut reader = BufReader::new(stdout);

        // 1. initialize -> a result carrying our server info + sync capability.
        stdin
            .write_all(&frame(&json!({
                "jsonrpc": "2.0", "id": 1, "method": "initialize",
                "params": { "processId": null, "rootUri": null, "capabilities": {} }
            })))
            .unwrap();
        let init =
            read_until(&mut reader, |m| m["id"] == json!(1) && m.get("result").is_some());

        // 2. initialized + didOpen a doc with one invalid method -> one diagnostic.
        stdin
            .write_all(&frame(&json!({
                "jsonrpc": "2.0", "method": "initialized", "params": {}
            })))
            .unwrap();
        let bad_uri = "file:///bad.es";
        stdin
            .write_all(&frame(&json!({
                "jsonrpc": "2.0", "method": "textDocument/didOpen",
                "params": { "textDocument": {
                    "uri": bad_uri, "languageId": "Elasticsearch", "version": 1,
                    "text": "PLOP /_search\n"
                }}
            })))
            .unwrap();
        let bad_diags = read_until(&mut reader, |m| {
            m["method"] == json!("textDocument/publishDiagnostics")
                && m["params"]["uri"] == json!(bad_uri)
        });

        // 3. didOpen a clean doc -> empty diagnostics (server always publishes).
        let good_uri = "file:///good.es";
        stdin
            .write_all(&frame(&json!({
                "jsonrpc": "2.0", "method": "textDocument/didOpen",
                "params": { "textDocument": {
                    "uri": good_uri, "languageId": "Elasticsearch", "version": 1,
                    "text": "GET /_search\n"
                }}
            })))
            .unwrap();
        let good_diags = read_until(&mut reader, |m| {
            m["method"] == json!("textDocument/publishDiagnostics")
                && m["params"]["uri"] == json!(good_uri)
        });

        // 4. shutdown / exit so the server process terminates cleanly. Both
        // methods take no params — tower-lsp rejects an explicit `null` params
        // on `shutdown` with -32602, so the field is omitted entirely.
        stdin
            .write_all(&frame(&json!({
                "jsonrpc": "2.0", "id": 2, "method": "shutdown"
            })))
            .unwrap();
        let _ =
            read_until(&mut reader, |m| m["id"] == json!(2) && m.get("result").is_some());
        stdin
            .write_all(&frame(&json!({
                "jsonrpc": "2.0", "method": "exit"
            })))
            .unwrap();

        let _ = tx.send((init, bad_diags, good_diags));
    });

    let (init, bad_diags, good_diags) = match rx.recv_timeout(Duration::from_secs(20)) {
        Ok(values) => values,
        Err(mpsc::RecvTimeoutError::Timeout) => {
            let _ = server.kill();
            panic!("language server did not complete the LSP conversation within 20s");
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            let _ = server.kill();
            panic!("worker thread failed before completing the LSP conversation");
        }
    };

    // initialize handshake: the server identifies itself and advertises sync.
    assert_eq!(
        init["result"]["serverInfo"]["name"],
        json!("elasticsearch-language-server"),
    );
    let sync = &init["result"]["capabilities"]["textDocumentSync"];
    assert!(
        sync.is_object() || sync.is_number(),
        "expected a textDocumentSync capability, got: {sync}",
    );

    // Faulty doc: exactly one ERROR diagnostic from our analyzer, sourced + worded.
    let diags = bad_diags["params"]["diagnostics"]
        .as_array()
        .expect("diagnostics array");
    assert_eq!(diags.len(), 1, "expected one diagnostic, got: {diags:?}");
    assert_eq!(diags[0]["severity"], json!(1), "diagnostic should be ERROR severity");
    assert_eq!(diags[0]["source"], json!("elasticsearch"));
    assert!(
        diags[0]["message"]
            .as_str()
            .unwrap()
            .contains("not a valid HTTP method"),
        "unexpected message: {}",
        diags[0]["message"],
    );

    // Clean doc: no diagnostics.
    let good = good_diags["params"]["diagnostics"]
        .as_array()
        .expect("diagnostics array");
    assert!(
        good.is_empty(),
        "clean document should have no diagnostics, got: {good:?}",
    );

    let _ = server.wait();
}
