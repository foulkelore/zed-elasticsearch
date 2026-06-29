; Language injection for Elasticsearch (.es) request bodies.
;
; The grammar exposes each JSON request body as a single opaque (body) node.
; Here we tell Zed to parse and highlight that region using its built-in JSON
; grammar, so the body gets full JSON syntax highlighting for free.

((body) @injection.content
 (#set! injection.language "json"))
