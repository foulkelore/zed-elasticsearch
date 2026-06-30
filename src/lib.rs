// Zed extension for Elasticsearch (.es) files.
//
// This crate compiles to WebAssembly and runs inside Zed. Its job here is small
// but important: tell Zed how to launch our language server. Zed then spawns
// that native binary and speaks LSP to it over stdio, and the server publishes
// diagnostics (see ../server).
//
// A Zed extension does NOT itself become the language server. It implements
// `language_server_command`, returning the executable to run.

use zed_extension_api::{self as zed, LanguageServerId, Result};

// ---------------------------------------------------------------------------
// Locating the server binary in local development.
//
// A Zed extension runs from its own sandboxed install directory, NOT this
// source repo, so a relative path to our build output would not resolve. We
// also do not want a machine-specific absolute path baked into committed
// source. So we resolve the binary at runtime, in priority order:
//
//   1. The `ELASTICSEARCH_LS_BINARY` environment variable, if set. This is the
//      escape hatch: point it at `server/target/debug/elasticsearch-language-server`
//      (or a release build) for local development.
//   2. `elasticsearch-language-server` found on the worktree's `$PATH` (e.g.
//      after `cargo install --path server`, or a symlink into `~/.cargo/bin`).
//
// If neither resolves, we return an error explaining how to fix it instead of
// failing with an opaque "binary not found".
//
// For a published release we would download or bundle a prebuilt binary; that
// is out of scope here (mirrors the local `file://` grammar decision).
// ---------------------------------------------------------------------------

/// Environment variable a developer can set to point at a locally-built server.
const SERVER_BINARY_ENV: &str = "ELASTICSEARCH_LS_BINARY";

/// The binary name to look for on `$PATH` when the env var is not set.
const SERVER_BINARY_NAME: &str = "elasticsearch-language-server";

struct ElasticsearchExtension;

impl ElasticsearchExtension {
    /// Resolve the path to the language server binary, or explain why we could
    /// not. See the module comment above for the resolution order.
    fn server_binary_path(worktree: &zed::Worktree) -> Result<String> {
        // 1. Explicit override via environment variable.
        if let Some((_, path)) = worktree
            .shell_env()
            .into_iter()
            .find(|(key, _)| key == SERVER_BINARY_ENV)
        {
            if !path.is_empty() {
                return Ok(path);
            }
        }

        // 2. Discover the binary on the worktree's `$PATH`.
        if let Some(path) = worktree.which(SERVER_BINARY_NAME) {
            return Ok(path);
        }

        Err(format!(
            "could not find the Elasticsearch language server. \
             Set `{SERVER_BINARY_ENV}` to the built binary \
             (e.g. server/target/debug/{SERVER_BINARY_NAME}), \
             or put `{SERVER_BINARY_NAME}` on your PATH \
             (e.g. `cargo install --path server`)."
        ))
    }
}

impl zed::Extension for ElasticsearchExtension {
    fn new() -> Self {
        ElasticsearchExtension
    }

    fn language_server_command(
        &mut self,
        _language_server_id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<zed::Command> {
        Ok(zed::Command {
            command: Self::server_binary_path(worktree)?,
            args: Vec::new(),
            env: Default::default(),
        })
    }
}

zed::register_extension!(ElasticsearchExtension);
