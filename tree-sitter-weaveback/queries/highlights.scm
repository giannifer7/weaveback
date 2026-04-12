; tree-sitter-weaveback/queries/highlights.scm

; --- Keywords (builtin macro names) ----------------------------------------
(macro_call
  name: (macro_name) @keyword
  (#match? @keyword
    "^%(def|redef|pydef|pydef_raw|set|if|equal|include|import|eval|here|export|env|pyset|pyget|capitalize|decapitalize|convert_case|to_snake_case|to_camel_case|to_pascal_case|to_screaming_case)$"))

; --- User-defined macro calls -----------------------------------------------
(macro_call
  name: (macro_name) @function.macro)

; --- Variable interpolation -------------------------------------------------
(variable
  "%(" @punctuation.special
  name: (identifier) @variable
  ")" @punctuation.special)

; --- Block delimiters -------------------------------------------------------
(block_open)  @punctuation.bracket
(block_close) @punctuation.bracket

; --- Comments ---------------------------------------------------------------
(line_comment)  @comment
(block_comment) @comment

; --- Escaped special --------------------------------------------------------
(escaped_special) @string.escape
