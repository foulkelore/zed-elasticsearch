// Elasticsearch language server — entry point.
//
// This binary speaks the Language Server Protocol (LSP) over stdin/stdout.
// Zed launches it as a child process and exchanges JSON-RPC messages with it.
//
// It completes the `initialize` handshake, then on every document open/change
// it runs the pure `analysis::analyze` function and publishes diagnostics
// (squiggles) back to the editor.

use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

// Pure diagnostic logic lives in its own module, unit-tested in isolation.
mod analysis;

// The state our server carries. `Client` is tower-lsp's handle for sending
// messages *back* to the editor (logs, diagnostics, etc.).
struct Backend {
    client: Client,
}

impl Backend {
    // Analyze a document's text and publish the resulting diagnostics to the
    // editor. Called on open and on every change. Passing `version` lets the
    // editor discard stale diagnostics if changes overtake each other.
    async fn analyze_and_publish(&self, uri: Url, text: &str, version: Option<i32>) {
        // `.map(Diagnostic::from)` works because of the `From` impls below: the
        // standard library gives us `Iterator::map` over our `Lint`s, and each
        // `&Lint` converts into an LSP `Diagnostic`.
        let diagnostics: Vec<Diagnostic> =
            analysis::analyze(text).iter().map(Diagnostic::from).collect();
        self.client
            .publish_diagnostics(uri, diagnostics, version)
            .await;
    }
}

// ---------------------------------------------------------------------------
// Converting our internal analysis types into LSP types via the `From` trait.
//
// `From<A> for B` is the idiomatic Rust way to say "a B can be built from an A".
// It also gives us `A.into()` for free (the `Into` trait is the mirror image),
// and lets `.map(Diagnostic::from)` read as a clean conversion at the call site.
//
// Orphan rule note: you may implement a foreign trait (`From`) for a foreign
// type (`lsp_types::Position`/`Range`/`Diagnostic`) ONLY when a local type is
// involved. Here our own `analysis::Position`/`Range`/`Lint` are the source
// types, which satisfies the rule. We could not, for example, write
// `impl From<lsp_types::Position> for lsp_types::Range` — both sides foreign.
//
// The impls compose: `Range`'s conversion calls `Position`'s via `.into()`, and
// `Diagnostic`'s calls `Range`'s. Small, single-purpose conversions stack up.
// ---------------------------------------------------------------------------

impl From<analysis::Position> for Position {
    fn from(pos: analysis::Position) -> Self {
        Position {
            line: pos.line,
            character: pos.character,
        }
    }
}

impl From<analysis::Range> for Range {
    fn from(range: analysis::Range) -> Self {
        Range {
            start: range.start.into(),
            end: range.end.into(),
        }
    }
}

impl From<&analysis::Lint> for Diagnostic {
    fn from(lint: &analysis::Lint) -> Self {
        Diagnostic {
            range: lint.range.into(),
            severity: Some(DiagnosticSeverity::ERROR),
            source: Some("elasticsearch".to_string()),
            message: lint.message.clone(),
            ..Diagnostic::default()
        }
    }
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

    // The editor opened an `.es` document: analyze it and publish diagnostics.
    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let doc = params.text_document;
        self.analyze_and_publish(doc.uri, &doc.text, Some(doc.version))
            .await;
    }

    // The editor changed an open document. Because we advertised FULL text
    // sync, the last content change holds the entire new document text.
    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        if let Some(change) = params.content_changes.into_iter().last() {
            self.analyze_and_publish(
                params.text_document.uri,
                &change.text,
                Some(params.text_document.version),
            )
            .await;
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    // Lock the Lint -> Diagnostic mapping: severity, source, message, and the
    // composed Range/Position conversions. Guards against an accidental field
    // change (e.g. someone dropping the source or flipping severity).
    #[test]
    fn lint_converts_to_error_diagnostic() {
        let lint = analysis::Lint {
            message: "boom".to_string(),
            range: analysis::Range {
                start: analysis::Position { line: 1, character: 2 },
                end: analysis::Position { line: 1, character: 5 },
            },
        };

        let diagnostic = Diagnostic::from(&lint);

        assert_eq!(diagnostic.severity, Some(DiagnosticSeverity::ERROR));
        assert_eq!(diagnostic.source.as_deref(), Some("elasticsearch"));
        assert_eq!(diagnostic.message, "boom");
        assert_eq!(diagnostic.range.start, Position { line: 1, character: 2 });
        assert_eq!(diagnostic.range.end, Position { line: 1, character: 5 });
    }
}
