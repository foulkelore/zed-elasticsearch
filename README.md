# Elasticsearch for Zed

A [Zed](https://zed.dev) extension that adds language support for Elasticsearch
Console / Dev Tools files (the `.es` format) — the same request format you use in
Kibana Dev Tools.

## Features

- Syntax highlighting for Elasticsearch requests:
  - HTTP methods (`GET`, `POST`, `PUT`, `DELETE`, `HEAD`, `PATCH`)
  - Request paths and query parameters
  - `#` and `//` line comments
- JSON request bodies are parsed and highlighted by the extension's own
  grammar (object keys, string values, numbers, booleans, null, and brackets)
- Bracket matching and auto-closing inside bodies
- Comment toggling with `cmd-/` inserts `#` everywhere, including inside bodies

## Example

```
# Cluster health
GET _cluster/health

PUT my-index
{
  "settings": { "number_of_shards": 1 }
}

POST my-index/_search?pretty
{
  "query": { "match": { "title": "elasticsearch" } }
}
```

## Status

Phase 1 (syntax highlighting) is under active development. See
[`.github/plans/elasticsearch-es-language-extension.md`](.github/plans/elasticsearch-es-language-extension.md)
for the full project plan.

## Development

This extension is installed locally as a Zed *dev extension*:

1. Open the Extensions page in Zed.
2. Click **Install Dev Extension**.
3. Select this directory.

The Tree-sitter grammar lives in a separate repository,
`tree-sitter-elasticsearch`. During local development the grammar is loaded from
the local filesystem (see the `file://` URL in `extension.toml`).

### Language server (Phase 2)

The extension launches a native language server for diagnostics. Build it and
make the extension find it one of two ways:

```sh
# Build the debug binary
cargo build --manifest-path server/Cargo.toml

# Option A: point the extension at the build via an env var (in your shell rc)
export ELASTICSEARCH_LS_BINARY="$PWD/server/target/debug/elasticsearch-language-server"

# Option B: install it onto your PATH instead (no env var needed)
cargo install --path server
```

The extension resolves the binary from `ELASTICSEARCH_LS_BINARY` first, then
falls back to `elasticsearch-language-server` on your `$PATH`.

## License

Apache License 2.0. See [LICENSE](LICENSE).
