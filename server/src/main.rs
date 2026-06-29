// Elasticsearch language server — entry point.
//
// This binary speaks the Language Server Protocol (LSP) over stdin/stdout.
// Zed launches it as a child process and exchanges JSON-RPC messages with it.
//
// For Slice 0 the server does almost nothing: it completes the `initialize`
// handshake and can shut down. Diagnostics arrive in later slices.

use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

// Pure diagnostic logic lives in its own module, unit-tested in isolation.
mod analysis;

// The state our server carries. `Client` is tower-lsp's handle for sending
// messages *back* to the editor (e.g. logs, diagnostics). We hold onto it so
// later slices can push diagnostics. For now we only use it to log.
struct Backend {
    client: Client,
}

// `#[tower_lsp::async_trait]` lets us write `async fn` inside a trait impl,
// which stable Rust traits do not natively allow yet. tower-lsp provides this
// macro so our handler methods can be async.
#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    // Called once, first, when the editor connects. We respond with our
    // capabilities: what we can do. Here we say we want full-text document
    // sync (the editor sends us the whole document text on open and change),
    // which is what our future diagnostics will analyze.
    async fn initialize(&self, _params: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                ..ServerCapabilities::default()
            },
            server_info: Some(ServerInfo {
                name: "elasticsearch-language-server".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
        })
    }

    // Called after `initialize` succeeds. A good place to log that we're up.
    async fn initialized(&self, _params: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "elasticsearch-language-server initialized")
            .await;
    }

    // Called when the editor asks us to shut down. We have no resources to
    // release yet, so we just acknowledge.
    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }
}

// `#[tokio::main]` sets up the async runtime and runs our async `main`.
#[tokio::main]
async fn main() {
    // LSP uses stdin for requests from the editor and stdout for our replies.
    // (Logging must NOT go to stdout — it would corrupt the protocol stream —
    // so tower-lsp routes log_message to the client instead.)
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    // Build the service, handing each Backend a Client to talk back through.
    let (service, socket) = LspService::new(|client| Backend { client });

    // Run the server: read from stdin, write to stdout, until the stream ends.
    Server::new(stdin, stdout, socket).serve(service).await;
}
