// Zed extension for Elasticsearch (.es) files.
//
// This crate compiles to WebAssembly and runs inside Zed. Its job here is small
// but important: tell Zed how to launch our language server. Zed then spawns
// that native binary and speaks LSP to it over stdio, and the server publishes
// diagnostics (see ../server).
//
// A Zed extension does NOT itself become the language server. It implements
// `language_server_command`, returning the executable to run.

use std::fs;

use zed_extension_api::{
    self as zed, Architecture, DownloadedFileType, GithubReleaseAsset, LanguageServerId,
    LanguageServerInstallationStatus, Os, Result,
};

// ---------------------------------------------------------------------------
// Locating the server binary.
//
// A Zed extension runs from its own sandboxed install directory, NOT this
// source repo, so a relative path to our build output would not resolve. We
// also do not want a machine-specific absolute path baked into committed
// source. So we resolve the binary at runtime, in priority order:
//
//   1. The `ELASTICSEARCH_LS_BINARY` environment variable, if set. This is the
//      dev escape hatch: point it at
//      `server/target/debug/elasticsearch-language-server` (or a release build)
//      for local development.
//   2. `elasticsearch-language-server` found on the worktree's `$PATH` (e.g.
//      after `cargo install --path server`, or a symlink into `~/.cargo/bin`).
//   3. A prebuilt binary downloaded from this repo's GitHub Releases for the
//      user's platform, extracted into the extension's working dir and cached
//      so it is not re-downloaded on every launch. This is the path real users
//      hit after installing from the Zed registry.
//
// If none resolve, we return an error explaining how to fix it instead of
// failing with an opaque "binary not found".
// ---------------------------------------------------------------------------

/// Environment variable a developer can set to point at a locally-built server.
const SERVER_BINARY_ENV: &str = "ELASTICSEARCH_LS_BINARY";

/// The binary name to look for on `$PATH` when the env var is not set.
const SERVER_BINARY_NAME: &str = "elasticsearch-language-server";

/// `<owner>/<repo>` that hosts the server release assets (co-located with this
/// extension — see the plan's Phase 0 decision).
const SERVER_REPO: &str = "foulkelore/zed-elasticsearch";

/// The release tag we fetch the server binary from. Pinned (not `latest`) so the
/// downloaded server is reproducible and matches the version this extension was
/// tested against. Bumping the server means bumping this in lockstep.
const SERVER_RELEASE_TAG: &str = "server-v0.1.0";

struct ElasticsearchExtension;

impl ElasticsearchExtension {
    /// Resolve the path to the language server binary, or explain why we could
    /// not. See the module comment above for the resolution order.
    fn resolve_server_binary(
        language_server_id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<String> {
        // 1. Explicit override via environment variable (local development).
        if let Some((_, path)) = worktree
            .shell_env()
            .into_iter()
            .find(|(key, _)| key == SERVER_BINARY_ENV)
        {
            if !path.is_empty() {
                return Ok(path);
            }
        }

        // 2. Discover the binary on the worktree's `$PATH` (local development).
        if let Some(path) = worktree.which(SERVER_BINARY_NAME) {
            return Ok(path);
        }

        // 3. Download a prebuilt binary for this platform (published install).
        let (os, arch) = zed::current_platform();
        Self::download_server_binary(language_server_id, os, arch)
    }

    /// Download (or reuse a cached copy of) the prebuilt server binary for the
    /// given platform. Reports progress to Zed's UI and, on failure, records a
    /// `Failed` status so the reason surfaces to the user.
    fn download_server_binary(
        language_server_id: &LanguageServerId,
        os: Os,
        arch: Architecture,
    ) -> Result<String> {
        let result = Self::download_server_binary_inner(language_server_id, os, arch);
        if let Err(error) = &result {
            zed::set_language_server_installation_status(
                language_server_id,
                &LanguageServerInstallationStatus::Failed(error.clone()),
            );
        }
        result
    }

    fn download_server_binary_inner(
        language_server_id: &LanguageServerId,
        os: Os,
        arch: Architecture,
    ) -> Result<String> {
        let target = ServerAsset::for_platform(os, arch)?;

        // The binary is extracted into a tag-named directory inside the
        // extension's working dir. Pinning the directory to the release tag lets
        // us detect "already downloaded" without any network call, and isolates
        // versions so an upgrade lands in a fresh dir.
        let version_dir = format!("{SERVER_BINARY_NAME}-{SERVER_RELEASE_TAG}");
        let binary_path = format!("{version_dir}/{}", target.binary_file_name);

        if !is_file(&binary_path) {
            zed::set_language_server_installation_status(
                language_server_id,
                &LanguageServerInstallationStatus::CheckingForUpdate,
            );

            let release = zed::github_release_by_tag_name(SERVER_REPO, SERVER_RELEASE_TAG)?;
            let asset = find_asset(&release.assets, &target.asset_name)?;

            zed::set_language_server_installation_status(
                language_server_id,
                &LanguageServerInstallationStatus::Downloading,
            );

            zed::download_file(&asset.download_url, &version_dir, target.file_type).map_err(
                |error| format!("failed to download {}: {error}", target.asset_name),
            )?;

            if !is_file(&binary_path) {
                return Err(format!(
                    "downloaded archive {} did not contain the expected binary at {binary_path}",
                    target.asset_name
                ));
            }

            zed::make_file_executable(&binary_path)
                .map_err(|error| format!("failed to mark {binary_path} executable: {error}"))?;

            remove_other_versions(&version_dir);
        }

        zed::set_language_server_installation_status(
            language_server_id,
            &LanguageServerInstallationStatus::None,
        );

        Ok(binary_path)
    }
}

/// The per-platform release asset to download and what it unpacks to. Mirrors
/// the naming contract enforced by `.github/workflows/release-server.yml`:
/// `elasticsearch-language-server-<rust-target-triple>.<ext>`, where the archive
/// holds the bare binary at its root.
struct ServerAsset {
    asset_name: String,
    binary_file_name: &'static str,
    file_type: DownloadedFileType,
}

impl ServerAsset {
    fn for_platform(os: Os, arch: Architecture) -> Result<Self> {
        let (triple, extension, binary_file_name, file_type) = match (os, arch) {
            (Os::Mac, Architecture::Aarch64) => (
                "aarch64-apple-darwin",
                "tar.gz",
                SERVER_BINARY_NAME,
                DownloadedFileType::GzipTar,
            ),
            (Os::Mac, Architecture::X8664) => (
                "x86_64-apple-darwin",
                "tar.gz",
                SERVER_BINARY_NAME,
                DownloadedFileType::GzipTar,
            ),
            (Os::Linux, Architecture::X8664) => (
                "x86_64-unknown-linux-gnu",
                "tar.gz",
                SERVER_BINARY_NAME,
                DownloadedFileType::GzipTar,
            ),
            (Os::Linux, Architecture::Aarch64) => (
                "aarch64-unknown-linux-gnu",
                "tar.gz",
                SERVER_BINARY_NAME,
                DownloadedFileType::GzipTar,
            ),
            (Os::Windows, Architecture::X8664) => (
                "x86_64-pc-windows-msvc",
                "zip",
                "elasticsearch-language-server.exe",
                DownloadedFileType::Zip,
            ),
            (os, arch) => {
                return Err(format!(
                    "no prebuilt Elasticsearch language server for {}/{}. \
                     Build it from source and set `{SERVER_BINARY_ENV}` or put \
                     `{SERVER_BINARY_NAME}` on your PATH.",
                    os_label(os),
                    arch_label(arch),
                ));
            }
        };

        Ok(ServerAsset {
            asset_name: format!("{SERVER_BINARY_NAME}-{triple}.{extension}"),
            binary_file_name,
            file_type,
        })
    }
}

fn find_asset<'a>(assets: &'a [GithubReleaseAsset], name: &str) -> Result<&'a GithubReleaseAsset> {
    assets
        .iter()
        .find(|asset| asset.name == name)
        .ok_or_else(|| {
            format!("release `{SERVER_RELEASE_TAG}` has no asset named `{name}`")
        })
}

/// Remove cached binaries from other release tags so old versions do not
/// accumulate in the extension's working dir. Best-effort: failures are ignored.
fn remove_other_versions(current_dir: &str) {
    let prefix = format!("{SERVER_BINARY_NAME}-");
    let Ok(entries) = fs::read_dir(".") else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with(&prefix) && name != current_dir {
            let _ = fs::remove_dir_all(entry.path());
        }
    }
}

fn is_file(path: &str) -> bool {
    fs::metadata(path).is_ok_and(|stat| stat.is_file())
}

fn os_label(os: Os) -> &'static str {
    match os {
        Os::Mac => "macOS",
        Os::Linux => "Linux",
        Os::Windows => "Windows",
    }
}

fn arch_label(arch: Architecture) -> &'static str {
    match arch {
        Architecture::Aarch64 => "aarch64",
        Architecture::X86 => "x86",
        Architecture::X8664 => "x86_64",
    }
}

impl zed::Extension for ElasticsearchExtension {
    fn new() -> Self {
        ElasticsearchExtension
    }

    fn language_server_command(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<zed::Command> {
        Ok(zed::Command {
            command: Self::resolve_server_binary(language_server_id, worktree)?,
            args: Vec::new(),
            env: Default::default(),
        })
    }
}

zed::register_extension!(ElasticsearchExtension);
