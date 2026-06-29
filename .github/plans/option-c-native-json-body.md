# Option C — Native JSON body parsing (drop JSON injection)

## Summary

Stop injecting Zed's `json` grammar into Elasticsearch request bodies. Instead,
have our own Tree-sitter grammar parse the JSON body structurally and expose
its nodes, then highlight those nodes ourselves. This makes the entire `.es`
file a **single language scope**, which fixes the `cmd-/` comment-toggle
behavior (it will insert `#` everywhere, including inside bodies) while keeping
rich body highlighting under our control.

## Motivation / root cause

`cmd-/` inserts `//` when the cursor is inside a `{ }` body because the body is
an **injected JSON** region, and Zed's JSON language config uses
`line_comments = ["// "]`. Zed resolves the comment prefix from the innermost
language scope at the cursor, so the injected JSON scope wins. Confirmed by
inspecting Zed's `toggle_comments` / `line_comment_prefixes` and by checking the
injection ranges for `examples/sample.es` (the body for the `POST _search`
request injects JSON across rows 41–52, i.e. lines 42–53).

By parsing the body ourselves and removing the injection, there is only one
scope (Elasticsearch), whose `line_comments = ["# "]` — so `#` is used
everywhere.

## Goals

- `cmd-/` inserts `# ` on every line of a `.es` file, including inside bodies.
- JSON request bodies remain fully highlighted (strings, numbers, booleans,
  null, object keys vs. string values, punctuation).
- Bracket matching / auto-close and indentation work inside bodies, driven by
  our own grammar (not the injected JSON).
- All existing corpus tests pass (after updating body expectations), and the
  full sample + edge cases still parse without unexpected errors.

## Scope

### Grammar repo (`tree-sitter-elasticsearch`)
- `grammar.js` — un-hide the body's JSON rules; rename `_object`→`object`,
  `_array`→`array`, `_pair`→`pair`, `_string`→`string`, `_number`→`number`,
  `_true`/`_false`→`boolean`, `_null`→`null`. Add `key:`/`value:` fields on
  `pair` so object keys can be highlighted differently from string values.
- `test/corpus/bodies.txt` — update expected trees to reflect the now-visible
  body structure. (`comments.txt` and `requests.txt` are unaffected.)
- Regenerate the parser (`tree-sitter generate`) and run `tree-sitter test`.
- Commit; note the new commit SHA.

### Extension repo (`elasticsearch/`)
- `languages/elasticsearch/injections.scm` — remove the `json` injection rule.
  (Delete the file, or leave it empty/commented. Decision: delete it, since we
  no longer inject anything.)
- `languages/elasticsearch/highlights.scm` — add highlight patterns for the new
  body nodes:
  - `(pair key: (string) @property)`
  - value `(string) @string`
  - `(number) @number`
  - `(boolean) @boolean` (or `@constant` if the theme lacks `@boolean`)
  - `(null) @constant`
  - object/array punctuation `{ } [ ] : ,` → `@punctuation.bracket` /
    `@punctuation.delimiter`
- `languages/elasticsearch/brackets.scm` — add `{}`/`[]`/`""` pairs now that the
  body nodes are ours.
- `languages/elasticsearch/indents.scm` — indent inside `object`/`array`.
- `extension.toml` — bump the grammar `rev` to the new grammar commit SHA.

## Approach (incremental, test-driven)

1. **Un-hide + rename** the body rules in `grammar.js`; add `key:`/`value:`
   fields to `pair`. Keep `string` and `number` as `token(...)` so they stay
   single leaf tokens.
2. **Regenerate** the parser; expect corpus failures in `bodies.txt` (the tree
   shape changed). This is the "red" step.
3. **Update `bodies.txt`** expected trees to match the new structure; re-run
   `tree-sitter test` until green ("green" step).
4. **Sanity-parse** `examples/sample.es` and `examples/edge-cases.es`; confirm
   only the intended/expected errors (incomplete bodies in edge cases) remain.
5. **Remove the injection** (`injections.scm`).
6. **Write body highlights** in `highlights.scm`; validate with
   `tree-sitter query languages/elasticsearch/highlights.scm examples/sample.es`
   from the grammar repo so we catch invalid patterns before loading Zed.
7. **Add brackets/indents** for the body; validate the same way.
8. **Commit grammar repo**, bump `extension.toml` `rev`.
9. **Reinstall the dev extension** (uninstall + `zed: install dev extension`)
   and visually verify: body highlighting looks right AND `cmd-/` inserts `#`
   inside a body (e.g. on line 42 of the sample).
10. **Commit the extension repo** as a save point.

## Out of scope

- Publishing to GitHub (staying local for now).
- A full JSON Query DSL (we still only parse generic JSON shape).
- Diagnostics / language server (that is Phase 2).

## Risks & open questions

- [ ] **Theme capture names.** Some themes may not define `@boolean`; if colors
      look off we fall back to `@constant`/`@constant.builtin`. Decide after
      visual QA.
- [ ] **Error recovery.** Mid-typing bodies will still produce ERROR nodes
      (expected). We should confirm a half-typed body doesn't wreck highlighting
      of the rest of the file. Tree-sitter's error recovery generally isolates
      this, but we will eyeball it.
- [ ] **`token()` on string/number.** Keeping these as single tokens avoids
      exposing their internals (we do not want to highlight individual escape
      chars yet). If we later want escape-sequence highlighting, we revisit.
- [ ] **Bulk bodies** (`_bulk`): multiple top-level objects on separate lines
      must still parse (already covered by `body: repeat1($._value)`/now
      `repeat1($.value)`); keep a corpus case for it.

## Checklist

### Grammar
- [x] Un-hide + rename body rules in `grammar.js`
- [x] Add `key:` / `value:` fields to `pair`
- [x] Regenerate parser
- [x] Update `test/corpus/bodies.txt` expected trees
- [x] `tree-sitter test` all green
- [x] Parse `sample.es` / `edge-cases.es` — only expected errors
- [x] Commit grammar repo; record new SHA (`2194e3859d01cdd50b9585b92e5f15904f105f11`)

### Extension
- [x] Remove `injections.scm`
- [x] Add body highlights to `highlights.scm`
- [x] Add `brackets.scm` body pairs
- [x] Add `indents.scm` body indent rules
- [x] Validate all `.scm` via `tree-sitter query`
- [x] Bump `extension.toml` grammar `rev`
- [ ] Reinstall dev extension; verify `#` toggling in body + highlighting
- [ ] Commit extension repo
