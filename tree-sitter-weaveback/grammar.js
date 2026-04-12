// tree-sitter-weaveback/grammar.js
//
// Grammar for the Weaveback macro language.
// Special character is hardcoded to '%' (the default).
// Users who configure a different special character must generate a
// modified grammar with the desired character substituted throughout.
//
// Context sensitivity: inside a macro arg list, ',' and ')' are
// separators, not text.  We handle this by using two different text
// tokens: `text` (outside args) and `arg_text` (inside args).
// Blocks %{...%} / %tag{...%tag} and verbatim blocks %[...%] /
// %tag[...%tag] escape back to the "anything goes" context, so nested
// commas and parens inside a block are fine.

module.exports = grammar({
  name: "weaveback",

  extras: ($) => [],  // whitespace is significant (passthrough text)

  rules: {
    // A source file is a sequence of top-level nodes
    source_file: ($) => repeat($._node),

    // Nodes valid at top-level and inside blocks
    _node: ($) =>
      choice(
        $.text,
        $.macro_call,
        $.variable,
        $.block,
        $.verbatim_block,
        $.line_comment,
        $.block_comment,
        $.escaped_special,
      ),

    // Nodes valid inside a macro argument (commas and ')' are special)
    _arg_node: ($) =>
      choice(
        $.arg_text,
        $.macro_call,
        $.variable,
        $.block,        // %{...%} restores full text inside
        $.verbatim_block,
        $.line_comment,
        $.block_comment,
        $.escaped_special,
      ),

    // -----------------------------------------------------------------------
    // Text tokens
    // -----------------------------------------------------------------------

    // Outside macro args: anything that isn't '%'
    text: (_) => /[^%]+/,

    // Inside macro args: anything that isn't '%', ',', '(', or ')'
    arg_text: (_) => /[^%,()]+/,

    // -----------------------------------------------------------------------
    // Escaped special: %% → literal %
    // -----------------------------------------------------------------------
    escaped_special: (_) => "%%",

    // -----------------------------------------------------------------------
    // Variable interpolation: %(name)
    // -----------------------------------------------------------------------
    variable: ($) =>
      seq(
        "%(",
        field("name", $.identifier),
        ")",
      ),

    // -----------------------------------------------------------------------
    // Macro call: %name(arg, arg, ...)
    // -----------------------------------------------------------------------
    macro_call: ($) =>
      seq(
        field("name", $.macro_name),
        "(",
        optional($._arg_list),
        ")",
      ),

    macro_name: (_) => token(seq("%", /[a-zA-Z_][a-zA-Z0-9_]*/)),

    _arg_list: ($) =>
      seq(
        field("arg", $.argument),
        repeat(seq(",", field("arg", $.argument))),
      ),

    argument: ($) => repeat1($._arg_node),

    // -----------------------------------------------------------------------
    // Blocks: %{...%}  or  %tag{...%tag}
    // Inside a block, top-level node rules apply (commas/parens are text).
    // -----------------------------------------------------------------------
    block: ($) =>
      seq(
        field("open", $.block_open),
        repeat($._node),
        field("close", $.block_close),
      ),

    // %{  or  %tag{
    block_open: (_) =>
      token(seq("%", optional(/[a-zA-Z_][a-zA-Z0-9_]*/), "{")),

    // %}  or  %tag}
    block_close: (_) =>
      token(seq("%", optional(/[a-zA-Z_][a-zA-Z0-9_]*/), "}")),

    // -----------------------------------------------------------------------
    // Verbatim blocks: %[...] / %tag[...%tag]
    // -----------------------------------------------------------------------
    verbatim_block: ($) =>
      seq(
        field("open", $.verbatim_open),
        repeat(choice($.verbatim_text, $.verbatim_block)),
        field("close", $.verbatim_close),
      ),

    verbatim_open: (_) =>
      token(seq("%", optional(/[a-zA-Z_][a-zA-Z0-9_]*/), "[")),

    verbatim_close: (_) =>
      token(seq("%", optional(/[a-zA-Z_][a-zA-Z0-9_]*/), "]")),

    verbatim_text: (_) =>
      token(
        choice(
          /[^%]+/,
          /%[^\[\]%][^%\[\]]*/,
          /%/,
        ),
      ),

    // -----------------------------------------------------------------------
    // Comments
    // -----------------------------------------------------------------------

    // %# ...  %// ...  %-- ...  (to end of line)
    line_comment: (_) =>
      token(
        seq(
          "%",
          choice("#", "//", "--"),
          /[^\n]*/,
        ),
      ),

    // %/* ... %*/  (we match the delimited span; nesting not enforced here)
    block_comment: ($) =>
      seq(
        "%/*",
        repeat(choice($.block_comment, /[^%]+/, /%[^*/]/)),
        "%*/",
      ),

    // -----------------------------------------------------------------------
    // Identifiers
    // -----------------------------------------------------------------------
    identifier: (_) => /[a-zA-Z_][a-zA-Z0-9_]*/,
  },
});
