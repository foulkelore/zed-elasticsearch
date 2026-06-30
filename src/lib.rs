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
// DEV-ONLY: absolute path to the locally-built server binary.
//
// Why absolute? When Zed runs an extension, the extension's working directory
// is its own installed copy (sandboxed), NOT this source repo. So a relative
// path like "server/target/debug/..." would not resolve to our build output.
//
// This mirrors the local `file://` grammar path in extension.toml: it is kept
// local and never published. For a release we would download or bundle a
// prebuilt server binary instead (out of scope for now).
//
// If you move the repo, update this path (and rebuild the server with
// `cargo build` inside `server/`).
// ---------------------------------------------------------------------------
const DEV_SERVER_BINARY: &str =
    "/Users/TXI4N9D/code/foulkelore/Zed/elasticsearch/server/target/debug/elasticsearch-language-server";

struct ElasticsearchExtension;

impl zed::Extension for ElasticsearchExtension {
    fn new() -> Self {
        ElasticsearchExtension
    }

    fn language_server_command(
        &mut self,
        _language_server_id: &LanguageServerId,
        _worktree: &zed::Worktree,
    ) -> Result<zed::Command> {
        Ok(zed::Command {
            command: DEV_SERVER_BINARY.to_string(),
            args: Vec::new(),
            env: Default::default(),
        })
    }
}

zed::register_extension!(ElasticsearchExtension);
