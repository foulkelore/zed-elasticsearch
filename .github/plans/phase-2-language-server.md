# Phase 2 — Elasticsearch language server (diagnostics)

## Summary

Add a real Language Server Protocol (LSP) server for `.es` files and wire the
Zed extension to launch it. The server analyzes each open `.es` document and
publishes diagnostics (squiggles). We start with one diagnostic — an invalid or
missing HTTP method on a request line — and grow from there.

This is the first time the project contains executable Rust. It is also a Rust
learning vehicle, so we build in thin, test-driven vertical slices and explain
concepts as they arise.

## Goals

- A standalone Rust binary, `elasticsearch-language-server`, that speaks LSP over
  stdio: it starts, completes the `initialize` handshake, accepts document
  open/change notifications, and publishes `textDocument/publishDiagnostics`.
- A first diagnostic: a request line whose first token is not a valid HTTP
  method (GET, POST, PUT, DELETE, HEAD, PATCH, OPTIONS) is flagged with a clear
  message and the precise range of the offending token.
- The Zed extension launches the server via `language_server_command`, and the
  squiggle appears in the editor on a bad request line.
- The diagnostic logic is covered by fast unit tests, independent of the LSP
  transport (pure function: text in, list of diagnostics out).

## Architecture (how the pieces fit)

```
+-----------------------------+        spawns         +-------------------------------+
|  Zed extension (WASM)       |  ───────────────────► |  elasticsearch-language-server |
|  elasticsearch/src/lib.rs   |   language_server_     |  (native binary, tower-lsp)    |
|  impl zed::Extension        |   command() returns    |  - initialize / shutdown       |
|  - language_server_command  |   { command, args }    |  - didOpen/didChange           |
+-----------------------------+                        |  - publishDiagnostics          |
            ▲                                          +-------------------------------+
            │ extension.toml                                        │
            │ [language_servers.*]                                  │ calls
            │                                                       ▼
            │                                          +-------------------------------+
            └───────── one repo ───────────────────►  |  analysis module (pure Rust)  |
                                                       |  fn analyze(text) -> Vec<Diag> |
                                                       |  unit-tested in isolation      |
                                                       +-------------------------------+
```

Key fact (verified against Zed docs + the `test-extension` source): a Zed
extension does not *become* a language server. The WASM extension implements
`language_server_command`, which returns a `zed::Command` (an executable path +
args + env). Zed runs that program and speaks LSP to it over stdio. So "our own
language server" = a separate native binary that the extension launches.

## Scope / layout

Everything lives in the existing extension repo (decision: co-locate, do not
make a third repo — the file tools can edit a subdirectory directly, and it
keeps one repo to manage; we can split it out later if we publish).

```
elasticsearch/
  extension.toml            # add [language_servers.elasticsearch-language-server]
                            # + add the extension as a Rust crate (lib.rs)
  Cargo.toml                # NEW: the WASM extension crate (zed_extension_api)
  src/
    lib.rs                  # NEW: impl zed::Extension, language_server_command
  server/                   # NEW: the native LSP server (separate crate)
    Cargo.toml              # bin crate: tower-lsp + tokio
    src/
      main.rs               # stdio transport + tower-lsp service wiring
      analysis.rs           # pure analyze(text) -> Vec<Diagnostic-like>; UNIT TESTED
```

Note: the extension WASM crate and the server native crate are **separate**
crates with different targets (`wasm32-wasip1` vs the host triple). They are not
a single cargo workspace, to avoid target/feature bleed. Each builds on its own.

## Pinned versions (verified on crates.io)

- `zed_extension_api = "0.7.0"` (extension crate, edition 2024)
- `tower-lsp = "0.20.0"` (server; brings `tokio` + `lsp-types` transitively)
- `tokio = { version = "1", features = ["full"] }` (server runtime)

Risk note: `tower-lsp` 0.20.0 is mature but last published in 2023 and pins its
own `lsp-types`. It is still the standard Rust LSP framework and is fine for our
needs. If it ever becomes a blocker we can revisit `tower-lsp-server` or a hand-
rolled stdio loop, but not now.

## The dev "where does the binary come from?" question

The `test-extension` downloads a prebuilt server from GitHub releases. We are
staying local, so instead:

- We build the server ourselves: `cargo build` in `server/` produces
  `server/target/debug/elasticsearch-language-server`.
- During development, `language_server_command` returns the **absolute path** to
  that debug binary. (We will read it from an env var or compute it; simplest
  first version: a hardcoded absolute dev path, clearly marked, same pattern as
  the `file://` grammar path we keep local and never commit to the public URL.)
- Later, for publishing, we would switch to downloading a released binary or
  building in CI. Out of scope for Phase 2.

## Approach (incremental, test-driven slices)

### Slice 0 — Plan + server scaffold that starts and shuts down
1. Create `server/` crate (`cargo new --bin server --name elasticsearch-language-server`).
2. Add `tower-lsp` + `tokio`. Implement the minimal `LanguageServer`:
   `initialize` (advertise text sync + that we publish diagnostics), `initialized`,
   `shutdown`. `main` wires stdin/stdout to the tower-lsp service.
3. Build it. Verify with a tiny scripted LSP handshake over stdio (write an
   `initialize` request to the process stdin, read the `initialize` result) — no
   Zed yet. This proves the transport works.

### Slice 1 — First diagnostic logic (pure, TDD) — RED then GREEN
4. Add `analysis.rs` with `analyze(text: &str) -> Vec<Lint>` where `Lint` carries
   a message + line/column range. **Write the failing test first**: a document
   whose request line starts with `GET` produces no lint; one starting with
   `FOO` produces exactly one lint pointing at `FOO`. Then implement to green.
5. Cover edge cases as separate tests: lowercase `get` (decide: flag or allow —
   default flag, ES console is case-sensitive-ish; we will treat non-uppercase
   canonical methods as invalid), blank lines, comment lines (`#`/`//`) ignored,
   a line that is only a method, trailing whitespace.

### Slice 2 — Wire diagnostics through LSP + into Zed
6. In the server, on `didOpen`/`didChange`, run `analyze` on the document text
   and `publish_diagnostics` with the lints mapped to `lsp_types::Diagnostic`.
7. Create the **extension** crate: `Cargo.toml` (`zed_extension_api`),
   `src/lib.rs` implementing `zed::Extension` + `language_server_command`
   returning the dev path to the built server binary. `register_extension!`.
8. Add to `extension.toml`:
   ```toml
   [language_servers.elasticsearch-language-server]
   name = "Elasticsearch Language Server"
   languages = ["Elasticsearch"]
   ```
9. Build the server (`cargo build` in `server/`), build/install the dev
   extension, clear the grammar cache if needed. **Visual QA**: open an `.es`
   file, type `FOO /_search`, confirm a red squiggle on `FOO` with our message;
   fix it to `GET`, confirm the squiggle clears.

### Slice 3+ — More diagnostics (future, not this plan’s commitment)
- Malformed JSON body (reuse the structural understanding we already have; the
  server can run our parser or a JSON check and flag `ERROR` spans).
- Missing path after method; obviously-wrong paths.
- Later: hover, completion, formatting. Each its own slice.

## Out of scope (Phase 2)

- Publishing the extension/server (still local).
- Shipping/downloading a prebuilt server binary; CI builds.
- Talking to a live Elasticsearch cluster.
- Hover / completion / go-to / formatting (later phases).
- Making the two crates a single cargo workspace.

## Risks & open questions

- [ ] **Server binary path in dev.** Hardcoded absolute path is brittle if the
      repo moves. Acceptable for local dev (mirrors the `file://` grammar
      decision); revisit before publish.
- [ ] **`tower-lsp` age.** Mature but quiet; pins its own `lsp-types`. Fine for
      now (see version note).
- [ ] **Case sensitivity of methods.** We will flag non-canonical-uppercase
      methods (e.g. `get`) as invalid in Slice 1; confirm this matches how you
      actually write `.es` files. Easy to relax later.
- [ ] **Rebuild friction.** Changing the server requires `cargo build` + Zed
      picking up the new binary (it re-spawns the process; usually a window
      reload or re-open suffices). We will document the exact refresh step after
      Slice 2.
- [ ] **WASM extension build.** Adding `src/lib.rs` + `Cargo.toml` turns the
      grammar-only extension into a Rust extension; Zed compiles it to WASM on
      install. Confirm the install still succeeds (it downloads wasi-sdk the
      first time, as seen in the log earlier).

## Checklist

### Slice 0 — scaffold
- [x] Create `server/` bin crate (`elasticsearch-language-server`)
- [x] Add `tower-lsp` + `tokio`; minimal initialize/initialized/shutdown
- [x] `cargo build` server succeeds
- [x] Scripted stdio handshake returns an `initialize` result (no Zed)

### Slice 1 — diagnostic logic (TDD)
- [x] `analysis.rs`: failing test for `FOO` request line (RED)
- [x] Implement `analyze` to green (GREEN)
- [x] Edge-case tests: comments ignored, blank lines, method-only line, lowercase
- [x] `cargo test` in `server/` all green

### Slice 2 — wire into Zed
- [x] Server publishes diagnostics on didOpen/didChange
- [ ] Extension crate: `Cargo.toml` + `src/lib.rs` (`language_server_command`)
- [ ] `extension.toml`: `[language_servers.*]` entry
- [ ] Build server; reinstall dev extension
- [ ] Visual QA: squiggle on `FOO`, clears when fixed to `GET`
- [ ] Commit (server + extension wiring); keep dev-only paths local
