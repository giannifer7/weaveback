; tree-sitter-weaveback/queries/injections.scm
;
; Inject host-language grammars into scripted macro bodies.
; The body is the last argument of %pydef — in practice
; always wrapped in a %{...%} block.  We capture the block node so
; editors render its text content with the appropriate sub-grammar.
; The %{ and %} delimiters will appear as noise in the sub-parser,
; which is acceptable.

; Python inside %pydef bodies
((macro_call
   name: (macro_name) @_name
   arg: (argument
     [
       (block)
       (verbatim_block)
     ] @injection.content))
 (#eq? @_name "%pydef")
 (#set! injection.language "python"))
