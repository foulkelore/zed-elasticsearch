; Auto-indentation for Elasticsearch (.es) files.
;
; Indentation only applies inside JSON request bodies, which we now parse with
; our own grammar. These rules mirror Zed's built-in JSON indent rules:
; the whole container is marked @indent, and its closing bracket @end, so that
; pressing Enter inside the container indents and the closing bracket dedents.

(object
  "}" @end) @indent

(array
  "]" @end) @indent
