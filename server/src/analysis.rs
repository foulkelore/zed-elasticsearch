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

/// Analyze document text and return all lints found.
///
/// Slice 1 goal: flag a request line whose first token is not a valid HTTP
/// method (GET, POST, PUT, DELETE, HEAD, PATCH, OPTIONS).
pub fn analyze(text: &str) -> Vec<Lint> {
    let mut lints = Vec::new();

    for (line_index, line) in text.lines().enumerate() {
        let line_number = line_index as u32;

        // Find the first non-whitespace character and where it starts.
        // `char_indices`/`find` give byte offsets; for now our inputs are ASCII
        // so byte offset == character offset. We will revisit non-ASCII later.
        let start_col = match line.find(|c: char| !c.is_whitespace()) {
            Some(col) => col,
            None => continue, // blank or whitespace-only line: nothing to check
        };

        // The first token runs until the next whitespace (or end of line).
        let rest = &line[start_col..];
        let token_len = rest
            .find(|c: char| c.is_whitespace())
            .unwrap_or(rest.len());
        let token = &rest[..token_len];

        // Only HTTP request lines are subject to the method check. A request
        // line looks like `WORD /path...`: an all-letters first token followed
        // by whitespace and then a `/`. This skips JSON body lines (`{`, `"`,
        // `}`), comments (`#`, `//`), and anything else that is not a request.
        if !is_request_line(token, rest, token_len) {
            continue;
        }

        if VALID_METHODS.contains(&token) {
            continue; // valid method: no lint
        }

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

    lints
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
}
