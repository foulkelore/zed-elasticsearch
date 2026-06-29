; Syntax highlighting for Elasticsearch Console (.es) files.
;
; Each pattern matches a node from the grammar and tags it with a @capture
; name. Zed maps these captures to colors from the active theme.

; HTTP methods (GET, POST, PUT, ...) are highlighted like keywords.
(method) @keyword

; The request path (e.g. my-index/_search) reads like a function/route.
(path) @function

; Query parameters (?pretty&size=10).
(query_params) @string.special

; Line comments (# ... or // ...).
(comment) @comment
