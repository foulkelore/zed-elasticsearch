; Bracket matching for Elasticsearch (.es) files.
;
; Brackets appear inside JSON request bodies, which we now parse with our own
; grammar. These rules tell Zed which tokens form a matching pair so it can
; highlight the partner bracket when the cursor is on one.
;
; (Auto-close behavior while typing is configured separately in config.toml.)

; Object braces: { ... }
(object
  "{" @open
  "}" @close)

; Array brackets: [ ... ]
(array
  "[" @open
  "]" @close)
