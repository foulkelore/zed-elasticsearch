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

        // Find the first token (the method candidate) and where it starts.
        // `char_indices` gives byte offsets; for now our inputs are ASCII so
        // byte offset == character offset. We will revisit non-ASCII later.
        let start_col = match line.find(|c: char| !c.is_whitespace()) {
            Some(col) => col,
            None => continue, // blank or whitespace-only line: nothing to check
        };

        // The token runs until the next whitespace (or end of line).
        let rest = &line[start_col..];
        let token_len = rest
            .find(|c: char| c.is_whitespace())
            .unwrap_or(rest.len());
        let token = &rest[..token_len];

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
}
