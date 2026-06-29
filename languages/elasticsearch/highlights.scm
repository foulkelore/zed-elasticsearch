; Syntax highlighting for Elasticsearch Console (.es) files.
;
; Each pattern matches a node from the grammar and tags it with a @capture
; name. Zed maps these captures to colors from the active theme.

; ---------------------------------------------------------------------------
; Request line: method, path, query params
; ---------------------------------------------------------------------------

; HTTP methods (GET, POST, PUT, ...) are highlighted like keywords.
(method) @keyword

; The request path (e.g. my-index/_search) reads like a function/route.
(path) @function

; Query parameters (?pretty&size=10).
(query_params) @string.special

; ---------------------------------------------------------------------------
; JSON request body
; ---------------------------------------------------------------------------
; We parse the body ourselves (no JSON injection), so we highlight its nodes
; here. Order matters: the generic string rule comes first, then the more
; specific "object key" rule overrides it so keys and values can differ.

; Any string literal (default: a value string).
(string) @string

; Object keys take precedence over the generic string rule above.
(pair
  key: (string) @property)

; Numbers, booleans, and null.
(number) @number
(boolean) @boolean
(null) @constant.builtin

; Structural punctuation.
"{" @punctuation.bracket
"}" @punctuation.bracket
"[" @punctuation.bracket
"]" @punctuation.bracket
":" @punctuation.delimiter
"," @punctuation.delimiter

; ---------------------------------------------------------------------------
; Comments
; ---------------------------------------------------------------------------

; Line comments (# ... or // ...).
(comment) @comment
