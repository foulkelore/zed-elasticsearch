// Pure diagnostic logic for Elasticsearch (.es) files.
//
// This module knows nothing about LSP or I/O. It takes document text and
// returns a list of `Lint`s (problems found). Keeping it pure makes it fast
// and easy to unit-test in isolation; `main.rs` is responsible for turning
// these `Lint`s into LSP diagnostics and sending them to the editor.

/// A zero-based position in the document: which line, and the UTF-16 code-unit
/// offset within that line (`character`). Zero-based UTF-16 is exactly what LSP
/// expects; see `utf16_len` for why the unit matters for non-ASCII text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    pub line: u32,
    pub character: u32,
}

/// A half-open range [start, end) within the document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}

/// One problem found in the document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lint {
    pub message: String,
    pub range: Range,
}

/// Number of UTF-16 code units in `s`.
///
/// LSP `Position.character` is a UTF-16 code-unit offset — not a byte offset and
/// not a Unicode scalar (char) count. Most characters are 1 unit, but anything
/// outside the Basic Multilingual Plane (e.g. many emoji) is a surrogate pair =
/// 2 units. `str::encode_utf16` yields exactly those units, so counting them
/// gives the value the editor expects.
fn utf16_len(s: &str) -> u32 {
    s.encode_utf16().count() as u32
}

/// Convert a 0-based **byte** offset within `line` into a 0-based **UTF-16**
/// column. Used to translate serde_json's byte-based error column (verified:
/// it counts bytes) into the UTF-16 offset LSP wants.
///
/// Defensive about inputs we don't fully trust: the byte offset is clamped to
/// the line length and walked back to the nearest char boundary so slicing
/// never panics.
fn byte_to_utf16_col(line: &str, byte_col: usize) -> u32 {
    let mut boundary = byte_col.min(line.len());
    while boundary > 0 && !line.is_char_boundary(boundary) {
        boundary -= 1;
    }
    utf16_len(&line[..boundary])
}

/// The HTTP methods Elasticsearch / Kibana Dev Tools accept on a request line.
///
/// This list MUST stay in sync with the `method` rule in the grammar
/// (`tree-sitter-elasticsearch/grammar.js`). If the two disagree, a request can
/// be "valid" to this server (no squiggle) yet fail to parse in the grammar
/// (broken highlighting), or vice versa.
const VALID_METHODS: [&str; 6] = ["GET", "POST", "PUT", "DELETE", "HEAD", "PATCH"];

/// Path fragments whose request bodies are NDJSON (one JSON value per line),
/// not a single JSON document. We skip JSON validation for these to avoid
/// false positives; proper per-line validation can come later.
const NDJSON_PATH_MARKERS: [&str; 2] = ["_bulk", "_msearch"];

/// Analyze document text and return all lints found.
///
/// Currently detects:
/// - a request line whose first token is not a valid HTTP method;
/// - a request line with a valid method but no path;
/// - a request body that is not valid JSON (single-object bodies only).
pub fn analyze(text: &str) -> Vec<Lint> {
    let mut lints = Vec::new();
    let lines: Vec<&str> = text.lines().collect();

    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let line_number = i as u32;

        // Find the first non-whitespace character and where it starts.
        // `find` gives a byte offset; we convert byte offsets to UTF-16 columns
        // (what LSP wants) via `utf16_len` just before building the range below.
        let start_col = match line.find(|c: char| !c.is_whitespace()) {
            Some(col) => col,
            None => {
                i += 1;
                continue; // blank or whitespace-only line: nothing to check
            }
        };

        // The first token runs until the next whitespace (or end of line).
        let rest = &line[start_col..];
        let token_len = rest
            .find(|c: char| c.is_whitespace())
            .unwrap_or(rest.len());
        let token = &rest[..token_len];

        // Only HTTP request lines are processed. A request line begins with an
        // all-letters token (the method). This skips JSON body lines (`{`, `"`,
        // `}`), comments (`#`, `//`), and anything else that is not a request.
        if !is_request_candidate(token) {
            i += 1;
            continue;
        }

        // The path is whatever follows the method token on the line.
        let path = rest[token_len..].trim_start();
        // Byte offsets become UTF-16 columns for LSP. Both slice points are at
        // char boundaries (a non-whitespace start, and a whitespace/EOL end).
        let start_char = utf16_len(&line[..start_col]);
        let end_char = utf16_len(&line[..start_col + token_len]);
        let method_range = Range {
            start: Position { line: line_number, character: start_char },
            end: Position { line: line_number, character: end_char },
        };

        // One diagnostic per request line, in precedence order: an invalid
        // method wins over a missing path.
        if !VALID_METHODS.contains(&token) {
            lints.push(Lint {
                message: format!(
                    "`{token}` is not a valid HTTP method. Expected one of: {}.",
                    VALID_METHODS.join(", ")
                ),
                range: method_range,
            });
        } else if path.is_empty() {
            lints.push(Lint {
                message: format!("`{token}` request is missing a path (e.g. `{token} /_search`)."),
                range: method_range,
            });
        }

        // Collect the request's body: the consecutive lines after the request
        // line, up to a blank line or the next request line.
        let body_start = i + 1;
        let mut body_end = body_start; // exclusive
        while body_end < lines.len() {
            let body_line = lines[body_end];
            if body_line.trim().is_empty() || looks_like_request_line(body_line) {
                break;
            }
            body_end += 1;
        }

        // Validate the body. Two shapes:
        // - NDJSON endpoints (_bulk, _msearch): each non-blank line is its own
        //   JSON value, validated independently.
        // - everything else: the whole body is a single JSON document.
        if body_end > body_start {
            let body = &lines[body_start..body_end];
            if is_ndjson_path(path) {
                lints.extend(validate_ndjson_body(body, body_start as u32));
            } else if let Some(lint) = validate_json_body(body, body_start as u32) {
                lints.push(lint);
            }
        }

        // Advance past the body so we do not re-scan its lines as requests.
        i = body_end;
    }

    lints
}

/// True if a path contains an NDJSON endpoint marker (e.g. `_bulk`).
fn is_ndjson_path(path: &str) -> bool {
    NDJSON_PATH_MARKERS.iter().any(|m| path.contains(m))
}

/// Cheap check used while scanning a body: does this line begin a new request?
/// (Reuses the same rule as `is_request_candidate` but recomputes the token.)
fn looks_like_request_line(line: &str) -> bool {
    let start_col = match line.find(|c: char| !c.is_whitespace()) {
        Some(col) => col,
        None => return false,
    };
    let rest = &line[start_col..];
    let token_len = rest.find(|c: char| c.is_whitespace()).unwrap_or(rest.len());
    let token = &rest[..token_len];
    is_request_candidate(token)
}

/// Validate an NDJSON body (`_bulk` / `_msearch`): each non-blank line must be a
/// valid JSON value on its own. Returns one lint per malformed line.
///
/// `body_start_line` is the document line index (0-based) of `body_lines[0]`.
/// We deliberately do not enforce bulk action/source pairing here (Option A);
/// that semantic layer can come later.
fn validate_ndjson_body(body_lines: &[&str], body_start_line: u32) -> Vec<Lint> {
    let mut lints = Vec::new();
    for (offset, line) in body_lines.iter().enumerate() {
        if line.trim().is_empty() {
            continue; // tolerate blank lines within the body
        }
        let abs_line = body_start_line + offset as u32;
        // A single line is itself a one-line body, so reuse the same validator.
        if let Some(lint) = validate_json_body(&[*line], abs_line) {
            lints.push(lint);
        }
    }
    lints
}

/// Validate `body_lines` (the lines of a single request body) as one JSON value.
/// Returns a `Lint` positioned at the parse error, or `None` if the body parses.
///
/// `body_start_line` is the document line index (0-based) of `body_lines[0]`, so
/// we can translate serde_json's body-relative error position back to the
/// document's absolute coordinates.
fn validate_json_body(body_lines: &[&str], body_start_line: u32) -> Option<Lint> {
    let body_text = body_lines.join("\n");

    match serde_json::from_str::<serde_json::Value>(&body_text) {
        Ok(_) => None,
        Err(err) => {
            // serde_json reports 1-based line and BYTE column within `body_text`.
            // Convert to 0-based, translate the byte column to UTF-16 using the
            // offending line's text, then offset by where the body starts.
            let err_line_in_body = err.line().saturating_sub(1) as u32;
            let byte_col = err.column().saturating_sub(1);
            let err_line_text = body_lines
                .get(err_line_in_body as usize)
                .copied()
                .unwrap_or("");
            let err_col = byte_to_utf16_col(err_line_text, byte_col);
            let doc_line = body_start_line + err_line_in_body;
            Some(Lint {
                message: format!("Invalid JSON in request body: {err}"),
                range: Range {
                    start: Position { line: doc_line, character: err_col },
                    end: Position { line: doc_line, character: err_col + 1 },
                },
            })
        }
    }
}

/// Decide whether a line's first token marks it as an HTTP request line.
///
/// A request line begins with the method: one or more ASCII letters (e.g. GET,
/// FOO, get). We deliberately do NOT require a `/path` here — a method with no
/// path is still a (broken) request, which the missing-path diagnostic reports.
/// This still excludes JSON body lines (`{`, `"`, `}`), comments (`#`, `//`),
/// and numbers, since none of those start with an all-letters token.
fn is_request_candidate(token: &str) -> bool {
    !token.is_empty() && token.chars().all(|c| c.is_ascii_alphabetic())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_unknown_method_on_request_line() {
        let lints = analyze("FOO /_search");

        assert_eq!(lints.len(), 1, "expected exactly one lint for a bad method");
        let lint = &lints[0];
        assert!(
            lint.message.contains("FOO"),
            "message should name the offending token, got: {}",
            lint.message
        );
        // The squiggle should cover just `FOO`: line 0, characters 0..3.
        assert_eq!(
            lint.range,
            Range {
                start: Position { line: 0, character: 0 },
                end: Position { line: 0, character: 3 },
            }
        );
    }

    #[test]
    fn accepts_valid_method() {
        let lints = analyze("GET /_search");
        assert!(lints.is_empty(), "valid method should produce no lints");
    }

    #[test]
    fn flags_lowercase_method_as_invalid() {
        // Elasticsearch console methods are uppercase; treat `get` as invalid.
        let lints = analyze("get /_search");
        assert_eq!(lints.len(), 1, "lowercase method should be flagged");
        assert_eq!(lints[0].range.start.character, 0);
        assert_eq!(lints[0].range.end.character, 3);
    }

    #[test]
    fn ignores_json_body_lines() {
        // The body lines start with `{`, `"`, `}` — none are request lines and
        // none should be flagged as bad methods.
        let doc = "GET /_search\n{\n  \"query\": { \"match_all\": {} }\n}";
        let lints = analyze(doc);
        assert!(
            lints.is_empty(),
            "JSON body lines must not be treated as request lines, got: {lints:?}"
        );
    }

    #[test]
    fn ignores_comment_lines() {
        let doc = "# get the docs\n// another comment\nGET /_search";
        let lints = analyze(doc);
        assert!(
            lints.is_empty(),
            "comment lines must be ignored, got: {lints:?}"
        );
    }

    #[test]
    fn ignores_blank_and_whitespace_lines() {
        let doc = "\n   \nGET /_search\n\n";
        let lints = analyze(doc);
        assert!(lints.is_empty(), "blank lines must be ignored, got: {lints:?}");
    }

    #[test]
    fn flags_bad_method_on_correct_line_in_multiline_doc() {
        // Two requests; the second has a bad method on line index 3.
        let doc = "GET /_search\n{}\n\nFOO /_count";
        let lints = analyze(doc);
        assert_eq!(lints.len(), 1, "exactly one bad method expected");
        assert_eq!(lints[0].range.start.line, 3, "lint should be on line 3");
        assert_eq!(lints[0].range.start.character, 0);
        assert_eq!(lints[0].range.end.character, 3);
    }

    #[test]
    fn valid_methods_match_grammar() {
        // Guard against drift: this list must equal the `method` rule in
        // tree-sitter-elasticsearch/grammar.js. If you add a method to one,
        // add it to the other (and a corpus test) in the same change.
        assert_eq!(
            VALID_METHODS,
            ["GET", "POST", "PUT", "DELETE", "HEAD", "PATCH"],
            "VALID_METHODS drifted from the grammar's `method` rule"
        );
    }

    // --- JSON body validation (Slice 3) -----------------------------------

    #[test]
    fn accepts_valid_json_body() {
        let doc = "GET /_search\n{\n  \"query\": { \"match_all\": {} }\n}";
        let lints = analyze(doc);
        assert!(lints.is_empty(), "valid JSON body should produce no lints, got: {lints:?}");
    }

    #[test]
    fn flags_malformed_json_body() {
        // Missing value after the colon: invalid JSON.
        let doc = "POST /_count\n{ \"bad\": }";
        let lints = analyze(doc);
        assert_eq!(lints.len(), 1, "malformed body should produce one lint, got: {lints:?}");
        // The error is on the body line (document line 1).
        assert_eq!(lints[0].range.start.line, 1, "lint should be on the body line");
        assert!(
            lints[0].message.to_lowercase().contains("json"),
            "message should mention JSON, got: {}",
            lints[0].message
        );
    }

    #[test]
    fn accepts_request_with_no_body() {
        // A lone request line with no following body must not be flagged.
        let doc = "GET /_search";
        let lints = analyze(doc);
        assert!(lints.is_empty(), "request without a body should produce no lints");
    }

    #[test]
    fn flags_malformed_body_on_correct_line_in_multiline_doc() {
        // First request is fine; second request's body is broken on line 4.
        let doc = "GET /_search\n{}\n\nPOST /_count\n{ \"oops\" }";
        let lints = analyze(doc);
        assert_eq!(lints.len(), 1, "exactly one malformed body expected, got: {lints:?}");
        assert_eq!(lints[0].range.start.line, 4, "lint should be on line 4");
    }

    #[test]
    fn accepts_valid_bulk_body_line_by_line() {
        // _bulk bodies are NDJSON: multiple JSON values, one per line. A body
        // where every line is valid JSON must not be flagged (each line is
        // validated independently rather than as one document).
        let doc = "POST /_bulk\n{ \"index\": {} }\n{ \"field\": 1 }";
        let lints = analyze(doc);
        assert!(lints.is_empty(), "valid bulk body must not be flagged, got: {lints:?}");
    }

    #[test]
    fn flags_both_methods_on_adjacent_bodyless_requests() {
        // Two bad request lines back-to-back with no bodies and no blank line:
        // the body-advance logic must not swallow the second request line.
        let doc = "FOO /_search\nBAR /_count";
        let lints = analyze(doc);
        assert_eq!(lints.len(), 2, "both bad methods should be flagged, got: {lints:?}");
        assert_eq!(lints[0].range.start.line, 0);
        assert_eq!(lints[1].range.start.line, 1);
    }

    // --- Missing path (Slice 4) -------------------------------------------

    #[test]
    fn flags_method_with_missing_path() {
        // A valid method with no path is an incomplete request.
        let doc = "GET";
        let lints = analyze(doc);
        assert_eq!(lints.len(), 1, "missing path should produce one lint, got: {lints:?}");
        assert!(
            lints[0].message.to_lowercase().contains("path"),
            "message should mention the missing path, got: {}",
            lints[0].message
        );
        // The squiggle covers the method token GET: line 0, characters 0..3.
        assert_eq!(lints[0].range.start.line, 0);
        assert_eq!(lints[0].range.start.character, 0);
        assert_eq!(lints[0].range.end.character, 3);
    }

    #[test]
    fn missing_path_with_trailing_whitespace_still_flagged() {
        // `GET ` (trailing spaces, no path) is still a missing path.
        let doc = "GET   ";
        let lints = analyze(doc);
        assert_eq!(lints.len(), 1, "missing path should be flagged, got: {lints:?}");
        assert!(lints[0].message.to_lowercase().contains("path"));
    }

    #[test]
    fn bad_method_takes_precedence_over_missing_path() {
        // `FOO` is both an invalid method and missing a path. We report only the
        // method error (one diagnostic per line, method wins).
        let doc = "FOO";
        let lints = analyze(doc);
        assert_eq!(lints.len(), 1, "exactly one lint expected, got: {lints:?}");
        assert!(
            lints[0].message.contains("FOO") && lints[0].message.to_lowercase().contains("method"),
            "the method error should win, got: {}",
            lints[0].message
        );
    }

    #[test]
    fn accepts_method_with_path() {
        // Sanity: a complete request line is still fine.
        let doc = "GET /_search";
        let lints = analyze(doc);
        assert!(lints.is_empty(), "complete request should produce no lints");
    }

    #[test]
    fn flags_missing_path_on_correct_line_in_multiline_doc() {
        let doc = "GET /_search\n{}\n\nPOST";
        let lints = analyze(doc);
        assert_eq!(lints.len(), 1, "one missing-path lint expected, got: {lints:?}");
        assert_eq!(lints[0].range.start.line, 3, "lint should be on line 3");
        assert!(lints[0].message.to_lowercase().contains("path"));
    }

    #[test]
    fn does_not_misread_body_lines_as_requests_after_broadening() {
        // A valid body whose interior lines start with letters (a bare `true`
        // and an unquoted-looking continuation) must be consumed as the body,
        // not re-scanned as new request lines once request detection no longer
        // requires a path.
        let doc = "POST /_search\n{\n  \"a\": true,\n  \"b\": false\n}";
        let lints = analyze(doc);
        assert!(
            lints.is_empty(),
            "valid body with letter-led lines must not be flagged, got: {lints:?}"
        );
    }

    // --- Per-line bulk / msearch validation (Slice 5) ---------------------

    #[test]
    fn flags_one_malformed_line_in_bulk_body() {
        // Second body line is malformed JSON; it should be flagged on its line.
        let doc = "POST /_bulk\n{ \"index\": {} }\n{ \"oops\" }\n{ \"ok\": 1 }";
        let lints = analyze(doc);
        assert_eq!(lints.len(), 1, "one malformed bulk line expected, got: {lints:?}");
        assert_eq!(lints[0].range.start.line, 2, "lint should be on the bad line (2)");
        assert!(lints[0].message.to_lowercase().contains("json"));
    }

    #[test]
    fn flags_each_malformed_line_in_bulk_body() {
        // Two bad lines -> two diagnostics, one per offending line.
        let doc = "POST /_bulk\n{ \"bad\" }\n{ \"ok\": 1 }\n{ \"alsobad\" }";
        let lints = analyze(doc);
        assert_eq!(lints.len(), 2, "two malformed bulk lines expected, got: {lints:?}");
        assert_eq!(lints[0].range.start.line, 1);
        assert_eq!(lints[1].range.start.line, 3);
    }

    #[test]
    fn validates_msearch_body_line_by_line() {
        // _msearch is also NDJSON; a malformed line is flagged.
        let doc = "GET /_msearch\n{}\n{ \"query\": }";
        let lints = analyze(doc);
        assert_eq!(lints.len(), 1, "one malformed msearch line expected, got: {lints:?}");
        assert_eq!(lints[0].range.start.line, 2);
    }

    #[test]
    fn bulk_error_column_is_offset_within_its_line() {
        // The error position should be within the offending line, not column 0,
        // so the squiggle lands on the actual problem.
        let doc = "POST /_bulk\n{ \"index\": {} }\n{ \"k\": }";
        let lints = analyze(doc);
        assert_eq!(lints.len(), 1, "one lint expected, got: {lints:?}");
        assert_eq!(lints[0].range.start.line, 2);
        assert!(
            lints[0].range.start.character > 0,
            "error column should be within the line, got {}",
            lints[0].range.start.character
        );
    }

    // --- UTF-16 position correctness (Slice 6) ----------------------------
    // LSP `Position.character` is a UTF-16 code-unit offset, not a byte or char
    // offset. serde_json reports byte columns, so a multibyte character before a
    // JSON error must not push the squiggle to the right.

    #[test]
    fn body_error_column_is_utf16_for_two_byte_char() {
        // `é` is 2 UTF-8 bytes but 1 UTF-16 unit. The error is at `?`. A
        // byte-based column would report 8; the correct UTF-16 column is 7.
        let doc = "POST /_doc\n{ \"é\": ? }";
        let lints = analyze(doc);
        assert_eq!(lints.len(), 1, "one malformed-body lint expected, got: {lints:?}");
        assert_eq!(lints[0].range.start.line, 1);
        assert_eq!(
            lints[0].range.start.character, 7,
            "character must be a UTF-16 offset (7), not a byte offset (8)"
        );
    }

    #[test]
    fn body_error_column_handles_surrogate_pair() {
        // `𐐷` (U+10437) is 4 UTF-8 bytes and 2 UTF-16 units. The error is at `?`.
        // A byte-based column would report 10; the correct UTF-16 column is 8.
        let doc = "POST /_doc\n{ \"𐐷\": ? }";
        let lints = analyze(doc);
        assert_eq!(lints.len(), 1, "one malformed-body lint expected, got: {lints:?}");
        assert_eq!(lints[0].range.start.line, 1);
        assert_eq!(
            lints[0].range.start.character, 8,
            "a surrogate pair counts as 2 UTF-16 units; expected character 8"
        );
    }
}
