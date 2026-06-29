; Bracket matching for Elasticsearch (.es) files.
;
; The only brackets in an .es file appear inside JSON request bodies, which
; are highlighted via JSON language injection (see injections.scm). Zed's
; injected JSON grammar provides its own bracket matching for that region,
; so no bracket rules are needed here.
;
; This file is intentionally minimal. Bracket pairs for editing/auto-close
; are configured in config.toml.
