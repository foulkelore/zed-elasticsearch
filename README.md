# Elasticsearch for Zed

A [Zed](https://zed.dev) extension that adds language support for Elasticsearch
Console / Dev Tools files (the `.es` format) — the same request format you use in
Kibana Dev Tools. It gives you **syntax highlighting** and **inline diagnostics**
for `.es` request files, with no cluster connection and no manual setup.

> **Scope:** this extension understands the **Console request file format**
> (HTTP method + path + JSON body, like Kibana Dev Tools). It is an editor
> language extension — it does **not** connect to a cluster, run requests, or
> implement the full Query DSL.
>
> **Unofficial:** this is a community project and is **not affiliated with,
> endorsed by, or sponsored by Elastic N.V.** "Elasticsearch" and "Kibana" are
> trademarks of Elastic N.V., used here only to describe what the extension
> supports.

<!--
TODO(before registry submission): add a screenshot or GIF here showing
highlighting + a diagnostic squiggle, e.g.:
![Elasticsearch highlighting and diagnostics in Zed](docs/screenshot.png)
Open examples/sample.es (or examples/value-types.es) in Zed to capture one.
-->

## Installation

From Zed's extension registry:

1. Open the command palette (`cmd-shift-p` / `ctrl-shift-p`) and run
   **`zed: extensions`** (or open **Extensions** from the menu).
2. Search for **Elasticsearch**.
3. Click **Install**.

Open any `.es` file and you'll get highlighting and diagnostics immediately. The
language server is downloaded automatically for your platform the first time it
is needed — there is no separate build or install step.

### File association

The extension claims the `.es` file suffix. To treat other files as
Elasticsearch Console, add them to your Zed settings:

```jsonc
{
  "file_types": {
    "Elasticsearch": ["es", "console"]
  }
}
```

## Features

- **Syntax highlighting** for Elasticsearch requests:
  - HTTP methods (`GET`, `POST`, `PUT`, `DELETE`, `HEAD`, `PATCH`)
  - Request paths and query parameters (`?pretty&size=10`)
  - `#` and `//` line comments
- **JSON request bodies** are parsed and highlighted by the extension's own
  grammar: object keys, string values, numbers, booleans, `null`, and brackets.
- **Bracket matching and auto-closing** inside request bodies.
- **Comment toggling** with `cmd-/` inserts `#` (Elasticsearch/Kibana's native
  comment style), including inside bodies.

### Diagnostics

A bundled language server analyzes each `.es` file as you type and reports:

| Diagnostic | Example |
|------------|---------|
| Invalid HTTP method | `PLOP /_search` → `PLOP` is not a valid HTTP method |
| Request line missing a path | `GET` on its own → `GET` request is missing a path |
| Malformed JSON in a request body | `{ "a": }` → JSON syntax error at the offending position |
| Duplicate object keys | `{ "a": 1, "a": 2 }` → duplicate key `a` (silently lost otherwise) |
| Per-line NDJSON validation | each malformed line of a `_bulk` / `_msearch` body is flagged individually |

Diagnostics are pure editor-side analysis of the request file — the extension
never contacts a cluster.

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

More samples — including value-type coloring and edge cases — live in the
[`examples/`](examples) directory.

## Troubleshooting

- **No highlighting or diagnostics on a file?** Confirm the file ends in `.es`,
  or add its extension under `file_types` (see [File association](#file-association)).
- **Diagnostics not appearing?** The language server downloads on first use; a
  slow or offline network can delay it. Check the LSP logs: run
  **`zed: open log`** from the command palette, or launch Zed from a terminal
  with `zed --foreground` to see the server's stderr and any download errors.
- **Want to verify the server is being found?** The download/launch path and any
  resolution error are reported in the LSP logs above.

## Development

This extension is installed locally as a Zed *dev extension*:

1. Open the Extensions page in Zed.
2. Click **Install Dev Extension**.
3. Select this directory.

The Tree-sitter grammar lives in a separate repository,
[`tree-sitter-elasticsearch`](https://github.com/foulkelore/tree-sitter-elasticsearch).
During local development the grammar is loaded from the local filesystem (see the
`file://` URL in `extension.toml`); released builds pin a published `https://`
revision.

### Language server

For end users the extension downloads a prebuilt language server for their
platform from the [`server-v*` releases](https://github.com/foulkelore/zed-elasticsearch/releases)
and caches it — no build step is required.

For local development, build the server and let the extension pick it up. The
extension resolves the binary in this order:

1. the `ELASTICSEARCH_LS_BINARY` environment variable,
2. `elasticsearch-language-server` on your `$PATH`,
3. the downloaded release binary (the user-facing default).

```sh
# Build the debug binary
cargo build --manifest-path server/Cargo.toml

# Option A: point the extension at the build via an env var (in your shell rc)
export ELASTICSEARCH_LS_BINARY="$PWD/server/target/debug/elasticsearch-language-server"

# Option B: install it onto your PATH instead (no env var needed)
cargo install --path server
```

Run the server's unit and integration tests with:

```sh
cargo test --manifest-path server/Cargo.toml
```

## License

Apache License 2.0. See [LICENSE](LICENSE).

## Trademarks

"Elasticsearch" and "Kibana" are trademarks of Elastic N.V. This project is an
independent, community-maintained extension and is not affiliated with or
endorsed by Elastic N.V. The names are used solely to identify the file format
and tooling this extension supports.
