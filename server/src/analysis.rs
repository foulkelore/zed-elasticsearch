// Pure diagnostic logic for Elasticsearch (.es) files.
//
// This module knows nothing about LSP or I/O. It takes document text and
// returns a list of `Lint`s (problems found). Keeping it pure makes it fast
// and easy to unit-test in isolation; `main.rs` is responsible for turning
// these `Lint`s into LSP diagnostics and sending them to the editor.

/// A zero-based position in the document: which line, and which UTF-16-ish
/// character offset within that line. We use zero-based to match LSP, which
/// the transport layer expects.
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

/// The HTTP methods Elasticsearch / Kibana Dev Tools accept on a request line.
const VALID_METHODS: [&str; 7] = ["GET", "POST", "PUT", "DELETE", "HEAD", "PATCH", "OPTIONS"];

/// Path fragments whose request bodies are NDJSON (one JSON value per line),
/// not a single JSON document. We skip JSON validation for these to avoid
/// false positives; proper per-line validation can come later.
const NDJSON_PATH_MARKERS: [&str; 2] = ["_bulk", "_msearch"];

/// Analyze document text and return all lints found.
///
/// Currently detects two kinds of problem:
/// - a request line whose first token is not a valid HTTP method;
/// - a request body that is not valid JSON (single-object bodies only).
pub fn analyze(text: &str) -> Vec<Lint> {
    let mut lints = Vec::new();
    let lines: Vec<&str> = text.lines().collect();

    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let line_number = i as u32;

        // Find the first non-whitespace character and where it starts.
        // `char_indices`/`find` give byte offsets; for now our inputs are ASCII
        // so byte offset == character offset. We will revisit non-ASCII later.
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

        // Only HTTP request lines are processed. A request line looks like
        // `WORD /path...`: an all-letters first token followed by whitespace
        // and then a `/`. This skips JSON body lines (`{`, `"`, `}`), comments
        // (`#`, `//`), and anything else that is not a request.
        if !is_request_line(token, rest, token_len) {
            i += 1;
            continue;
        }

        // Check the method itself.
        if !VALID_METHODS.contains(&token) {
            let start_char = start_col as u32;
            let end_char = (start_col + token_len) as u32;
            lints.push(Lint {
                message: format!(
                    "`{token}` is not a valid HTTP method. Expected one of: {}.",
                    VALID_METHODS.join(", ")
                ),
                range: Range {
                    start: Position { line: line_number, character: start_char },
                    end: Position { line: line_number, character: end_char },
                },
            });
        }

        // Collect the request's body: the consecutive lines after the request
        // line, up to a blank line or the next request line.
        let path = rest[token_len..].trim_start();
        let body_start = i + 1;
        let mut body_end = body_start; // exclusive
        while body_end < lines.len() {
            let body_line = lines[body_end];
            if body_line.trim().is_empty() || looks_like_request_line(body_line) {
                break;
            }
            body_end += 1;
        }

        // Validate the body as JSON unless it is empty or an NDJSON endpoint.
        if body_end > body_start && !is_ndjson_path(path) {
            if let Some(lint) = validate_json_body(&lines[body_start..body_end], body_start as u32) {
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
/// (Reuses the same shape rule as `is_request_line` but recomputes the token.)
fn looks_like_request_line(line: &str) -> bool {
    let start_col = match line.find(|c: char| !c.is_whitespace()) {
        Some(col) => col,
        None => return false,
    };
    let rest = &line[start_col..];
    let token_len = rest.find(|c: char| c.is_whitespace()).unwrap_or(rest.len());
    let token = &rest[..token_len];
    is_request_line(token, rest, token_len)
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
            // serde_json reports 1-based line and column within `body_text`.
            // Convert to 0-based and offset by where the body starts.
            let err_line_in_body = err.line().saturating_sub(1) as u32;
            let err_col = err.column().saturating_sub(1) as u32;
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

/// Decide whether a line is an HTTP request line that should be method-checked.
///
/// `token` is the first whitespace-delimited token, `rest` is the line from the
/// first non-whitespace character onward, and `token_len` is the byte length of
/// `token` within `rest`.
///
/// A request line looks like `WORD /path...`: the first token is one or more
/// ASCII letters, and what follows (after the whitespace gap) begins with `/`.
fn is_request_line(token: &str, rest: &str, token_len: usize) -> bool {
    // First token must be one or more ASCII letters (e.g. GET, FOO, get).
    if token.is_empty() || !token.chars().all(|c| c.is_ascii_alphabetic()) {
        return false;
    }

    // After the token there must be whitespace, then a `/`-rooted path.
    let after_token = &rest[token_len..];
    let after_trimmed = after_token.trim_start();
    after_trimmed.starts_with('/')
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
    fn skips_bulk_bodies_to_avoid_false_positives() {
        // _bulk bodies are NDJSON (multiple objects, one per line), which is not
        // a single valid JSON document. We defer NDJSON validation, so this must
        // NOT be flagged.
        let doc = "POST /_bulk\n{ \"index\": {} }\n{ \"field\": 1 }";
        let lints = analyze(doc);
        assert!(lints.is_empty(), "bulk NDJSON body must not be flagged, got: {lints:?}");
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
}
