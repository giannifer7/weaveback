; tree-sitter-weaveback: asciidoc injection
;
; install.py appends this block to ~/.config/helix/runtime/queries/asciidoc/injections.scm

; ── [source,weaveback] listing blocks ─────────────────────────────────────────────
((section_block
  (element_attr
    (element_attr_marker)
    (attr_value) @_lang
    (element_attr_marker))
  (listing_block
    (listing_block_start_marker)
    (listing_block_body) @injection.content
    (listing_block_end_marker)))
 (#match? @_lang "weaveback")
 (#set! injection.language "weaveback"))
