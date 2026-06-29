# Elasticsearch (.es) Language Extension for Zed

## Summary

Build a Zed editor extension that adds first-class language support for
Elasticsearch Console / Dev Tools files (the `.es` format). When a user opens an
`.es` file, Zed should recognize the language and apply syntax highlighting,
bracket matching, comment toggling, indentation, and code folding/outline —
the same "the editor understands this file" experience you get with Java or
Python.

This is also a **Rust learning project**, so the plan is sequenced to deliver a
working result with little/no Rust first (Phase 1), then optionally layer in
real Rust later (Phase 2) once there is motivation and context for it.

## The `.es` file format (what we are highlighting)

An `.es` file is a sequence of REST requests, each made of:

1. A **request line**: an HTTP method + a path, optionally with query params.
   - Methods: `GET`, `POST`, `PUT`, `DELETE`, `HEAD`, `PATCH`
   - Example: `GET my-index/_search?pretty`
2. An optional **JSON body** spanning one or more lines.
3. **Comments**: lines starting with `#` or `//`.

Example:

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

## Goals

- Opening a `.es` file shows "Elasticsearch" in Zed's language selector.
- HTTP methods, paths, and query params in the request line are highlighted.
- JSON request bodies are highlighted using Zed's existing JSON grammar
  (via Tree-sitter language injection — we do not re-implement JSON).
- `#` and `//` comments are highlighted and toggle with cmd-/.
- Brackets/braces match and auto-close inside bodies.
- Each request appears as a foldable/outline-able unit.
- The extension installs cleanly as a Zed "dev extension" and works locally.

## Scope

This project spans **two Git repositories** (a Zed requirement — grammars are
loaded from their own Git repo by commit SHA):

1. **The grammar repo** — `tree-sitter-elasticsearch` (new, separate repo)
   - `grammar.js` — the Tree-sitter grammar (small: request line + body + comments)
   - Generated parser + test corpus
2. **The extension repo** — this project (`elasticsearch/`)
   - `extension.toml` — manifest + grammar reference
   - `languages/elasticsearch/config.toml` — language metadata
   - `languages/elasticsearch/*.scm` — Tree-sitter queries (highlights, brackets,
     injections, indents, outline)
   - `LICENSE` — required by Zed for publishing

## Approach

### Phase 0 — Environment (DONE)

- [x] Replace Homebrew Rust with rustup-managed toolchain
- [x] Add `wasm32-wasip1` target (required to compile Zed extensions)
- [x] Ensure `~/.cargo/env` is sourced by zsh so new shells find Rust

### Phase 1 — Highlighting extension (little/no Rust)

The deliverable is a fully working, locally-installed extension.

1. **Scaffold the extension repo** (`extension.toml`, language `config.toml`).
   - Confirm Zed detects `.es` files as "Elasticsearch" even before highlighting
     works (language selector shows the name).
2. **Create the grammar repo** `tree-sitter-elasticsearch`.
   - Install `tree-sitter-cli` (via npm) to develop/test the grammar.
   - Write `grammar.js` incrementally, test-first using Tree-sitter's corpus
     tests (`test/corpus/*.txt`):
     1. Parse a single request line (method + path).
     2. Add query params (`?pretty`, `?a=b&c=d`).
     3. Add comments (`#` and `//`).
     4. Add a JSON body as an opaque block (delimited, not parsed internally —
        JSON parsing is delegated via injection).
     5. Support multiple requests in one file.
3. **Reference the grammar** from `extension.toml`.
   - Local dev: `file://` URL pointing at the local grammar repo.
   - Later/publish: real GitHub URL + commit SHA.
4. **Write the query files** in `languages/elasticsearch/`:
   - `highlights.scm` — methods as @keyword, paths as @string/@property,
     params, comments as @comment.
   - `injections.scm` — inject `json` into the request body node.
   - `brackets.scm` — `{}` / `[]` / `"` pairs.
   - `indents.scm` — indent inside objects/arrays.
   - `outline.scm` — one outline item per request.
5. **Install as a dev extension** and iterate on the highlighting until it looks
   right in both light and dark themes.

### Phase 2 — Language server (real Rust) — OPTIONAL, LATER

Only if we decide the extra power is worth it. Possible features:

- Autocomplete of Elasticsearch API endpoints and query DSL keys.
- "Run request" against a configured cluster, showing the response.
- Diagnostics for malformed requests.

This requires implementing the `zed::Extension` trait in Rust
(`src/lib.rs`), compiled to WebAssembly, and either bundling logic or
downloading/launching a language server. This is where the bulk of the Rust
learning happens, and we will decide on it after Phase 1 ships.

## Out of Scope

- Publishing to the Zed extension registry (we develop and run locally first;
  publishing is a later, separate step).
- A full Elasticsearch Query DSL grammar (we delegate body parsing to JSON).
- Running queries / cluster connectivity (that is Phase 2 at the earliest).
- Themes or icon themes (Zed requires those to be separate extensions anyway).

## Risks & Open Questions

- [ ] **Grammar authoring is its own learning curve.** Mitigation: keep the
      grammar minimal and lean on JSON injection; follow the `hurl` grammar as a
      reference (same "HTTP + JSON body" shape).
- [ ] **Two-repo workflow.** The grammar must live in its own Git repo for Zed
      to load it. Mitigation: use a `file://` URL during local development so we
      can iterate without pushing; switch to a GitHub URL + SHA before publish.
- [ ] **`zed_extension_api` version compatibility** (only relevant in Phase 2).
- [ ] **Existing community grammar is too minimal** (`TheChromion/tree-sitter-elasticsearch`
      only parses method + path, no bodies/comments). Decision: write our own
      minimal grammar rather than depend on/fork it.
- [ ] Should the language be named "Elasticsearch" exactly? (Affects the
      language selector label and `config.toml`.) Assumed yes.

## Checklist

### Phase 1
- [x] Scaffold `extension.toml`
- [x] Add `languages/elasticsearch/config.toml` (path_suffixes = ["es"])
- [x] Add `LICENSE` (Apache 2.0)
- [x] Add sample `.es` file, README, .gitignore
- [x] Confirm Zed detects `.es` as Elasticsearch (pre-grammar)
- [x] Create `tree-sitter-elasticsearch` grammar repo
- [x] Grammar: request line (method + path) + corpus test
- [x] Grammar: query params + corpus test
- [x] Grammar: comments + corpus test
- [x] Grammar: JSON body block + corpus test
- [x] Grammar: multiple requests + corpus test (incl. bulk multi-object body)
- [x] Reference grammar from `extension.toml` (file:// for dev)
- [x] `highlights.scm`
- [x] `injections.scm` (inject json into body)
- [x] `brackets.scm` (minimal — JSON injection handles body brackets)
- [x] `indents.scm` (minimal — JSON injection handles body indents)
- [x] `outline.scm`
- [x] Install as dev extension and verify highlighting (light + dark)
- [x] Verify `cmd-/` comment toggle inserts `#` (Elasticsearch-native)

### Phase 2 (optional, later)
- [ ] Decide whether to build the language server
- [ ] `Cargo.toml` + `src/lib.rs` implementing `zed::Extension`
- [ ] Language server command / download logic
- [ ] Feature: autocomplete and/or run-request
