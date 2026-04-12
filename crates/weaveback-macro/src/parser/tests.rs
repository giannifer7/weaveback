// src/parser/tests.rs

use crate::lexer::Lexer;
use crate::line_index::LineIndex;
use crate::parser::Parser;
use crate::types::{Token, TokenKind};

// -----------------------------------------------------------------------
// Tagged block helpers
// -----------------------------------------------------------------------

fn lex_parse(src: &str) -> Result<(), String> {
    let (tokens, lex_errors) = Lexer::new(src, '%', 0).lex();
    assert!(lex_errors.is_empty(), "unexpected lex errors: {:?}", lex_errors);
    let line_index = LineIndex::new(src);
    let mut parser = Parser::new();
    parser.parse(&tokens, src.as_bytes(), &line_index).map_err(|e| e.to_string())
}

#[test]
fn test_tagged_block_match() {
    assert!(lex_parse("%foo{ content %foo}").is_ok());
}

#[test]
fn test_anonymous_block_match() {
    assert!(lex_parse("%{ content %}").is_ok());
}

#[test]
fn test_tagged_block_mismatch() {
    let err = lex_parse("%foo{ content %bar}").unwrap_err();
    assert!(err.contains("foo"), "expected 'foo' in error: {}", err);
    assert!(err.contains("bar"), "expected 'bar' in error: {}", err);
}

#[test]
fn test_tagged_vs_anonymous_mismatch() {
    let err = lex_parse("%foo{ content %}").unwrap_err();
    assert!(err.contains("foo"), "expected 'foo' in error: {}", err);
    assert!(err.contains("anonymous"), "expected 'anonymous' in error: {}", err);
}

#[test]
fn test_anonymous_vs_tagged_mismatch() {
    let err = lex_parse("%{ content %foo}").unwrap_err();
    assert!(err.contains("foo"), "expected 'foo' in error: {}", err);
    assert!(err.contains("anonymous"), "expected 'anonymous' in error: {}", err);
}

#[test]
fn test_nested_tagged_blocks_match() {
    assert!(lex_parse("%outer{ %inner{ content %inner} %outer}").is_ok());
}

#[test]
fn test_unclosed_tagged_block() {
    let err = lex_parse("%foo{ no close").unwrap_err();
    assert!(err.contains("foo"), "expected tag name in error: {}", err);
}

#[test]
fn test_unclosed_anonymous_block() {
    let err = lex_parse("%{ no close").unwrap_err();
    assert!(err.contains("anonymous"), "expected 'anonymous' in error: {}", err);
}

#[test]
fn test_basic_parsing() {
    let tokens = vec![
        Token {
            src: 0,
            kind: TokenKind::Text,
            pos: 0,
            length: 5,
        },
        Token {
            src: 0,
            kind: TokenKind::BlockOpen,
            pos: 5,
            length: 2,
        },
        Token {
            src: 0,
            kind: TokenKind::Text,
            pos: 7,
            length: 3,
        },
        Token {
            src: 0,
            kind: TokenKind::BlockClose,
            pos: 10,
            length: 2,
        },
    ];

    let mut parser = Parser::new();
    assert!(parser.parse(&tokens, &[], &LineIndex::from_bytes(&[])).is_ok());

    let json = parser.to_json();
    assert!(!json.is_empty());
}

#[test]
fn test_empty_input() {
    let tokens = vec![];
    let mut parser = Parser::new();
    assert!(parser.parse(&tokens, &[], &LineIndex::from_bytes(&[])).is_ok());
}

#[test]
fn test_macro_parsing() {
    let tokens = vec![
        Token {
            src: 0,
            kind: TokenKind::Macro,
            pos: 0,
            length: 2,
        },
        Token {
            src: 0,
            kind: TokenKind::Ident,
            pos: 2,
            length: 4,
        },
        Token {
            src: 0,
            kind: TokenKind::Space,
            pos: 6,
            length: 1,
        },
        Token {
            src: 0,
            kind: TokenKind::Equal,
            pos: 7,
            length: 1,
        },
        Token {
            src: 0,
            kind: TokenKind::CloseParen,
            pos: 8,
            length: 1,
        },
    ];

    let mut parser = Parser::new();
    assert!(parser.parse(&tokens, &[], &LineIndex::from_bytes(&[])).is_ok());

    let json = parser.to_json();
    assert!(!json.is_empty());
}

#[test]
fn test_token_kind_conversion() {
    use std::convert::TryFrom;
    assert!(TokenKind::try_from(0).is_ok());
    assert!(TokenKind::try_from(16).is_ok());
    assert!(TokenKind::try_from(-1).is_err());
    assert!(TokenKind::try_from(17).is_err());
}

// -----------------------------------------------------------------------
// Helper: lex+parse expecting an error from either stage
// -----------------------------------------------------------------------

fn lex_parse_err(src: &str) -> String {
    let (tokens, lex_errors) = Lexer::new(src, '%', 0).lex();
    if !lex_errors.is_empty() {
        return lex_errors.iter().map(|e| e.message.as_str()).collect::<Vec<_>>().join("; ");
    }
    let line_index = LineIndex::new(src);
    let mut parser = Parser::new();
    parser
        .parse(&tokens, src.as_bytes(), &line_index)
        .err()
        .map(|e| e.to_string())
        .unwrap_or_default()
}

// -----------------------------------------------------------------------
// Tagged block — valid structures
// -----------------------------------------------------------------------

#[test]
fn test_empty_tagged_block() {
    assert!(lex_parse("%a{%a}").is_ok());
}

#[test]
fn test_single_char_tag() {
    assert!(lex_parse("%x{ content %x}").is_ok());
}

#[test]
fn test_tag_with_underscores_and_digits() {
    assert!(lex_parse("%block_1{ body %block_1}").is_ok());
}

#[test]
fn test_sequential_same_tag() {
    // The same tag can be reused after closing — no state leak.
    assert!(lex_parse("%foo{first%foo}%foo{second%foo}").is_ok());
}

#[test]
fn test_sequential_different_tags() {
    assert!(lex_parse("%a{x%a}%b{y%b}%c{z%c}").is_ok());
}

#[test]
fn test_three_level_nesting() {
    assert!(lex_parse("%a{ %b{ %c{ deep %c} %b} %a}").is_ok());
}

#[test]
fn test_four_level_nesting() {
    assert!(lex_parse("%a{%b{%c{%d{deep%d}%c}%b}%a}").is_ok());
}

#[test]
fn test_tagged_inside_anonymous() {
    assert!(lex_parse("%{ %foo{ inner %foo} %}").is_ok());
}

#[test]
fn test_anonymous_inside_tagged() {
    assert!(lex_parse("%foo{ %{ inner %} %foo}").is_ok());
}

#[test]
fn test_anonymous_nested_three_deep() {
    assert!(lex_parse("%{ %{ %{ deep %} %} %}").is_ok());
}

#[test]
fn test_text_before_and_after_block() {
    assert!(lex_parse("before %foo{ inside %foo} after").is_ok());
}

#[test]
fn test_var_inside_tagged_block() {
    assert!(lex_parse("%foo{ %(x) %(y) %foo}").is_ok());
}

#[test]
fn test_macro_call_inside_tagged_block() {
    assert!(lex_parse("%foo{ %bar(arg1, arg2) %foo}").is_ok());
}

#[test]
fn test_line_comment_inside_block() {
    assert!(lex_parse("%foo{\n%// this is a comment\ncontent\n%foo}").is_ok());
}

#[test]
fn test_block_comment_inside_block() {
    assert!(lex_parse("%foo{ %/* ignored %*/ content %foo}").is_ok());
}

#[test]
fn test_unicode_content_in_block() {
    // Tags are ASCII identifiers; content can be arbitrary UTF-8.
    assert!(lex_parse("%foo{ héllo wörld %foo}").is_ok());
}

#[test]
fn test_long_tag_name() {
    let src = "%very_long_tag_name_here{ body %very_long_tag_name_here}";
    assert!(lex_parse(src).is_ok());
}

#[test]
fn test_multiple_macros_across_blocks() {
    let src = "%a{ %def(x, hello) %(x) %a}%b{ %(x) %b}";
    // Just structural validity — tag matching and lex/parse pass.
    assert!(lex_parse(src).is_ok());
}

// -----------------------------------------------------------------------
// Tagged block — mismatch errors
// -----------------------------------------------------------------------

#[test]
fn test_mismatch_different_names() {
    let err = lex_parse("%foo{ %bar}").unwrap_err();
    assert!(err.contains("foo") && err.contains("bar"), "bad error: {}", err);
}

#[test]
fn test_wrong_nesting_order_causes_unclosed_error() {
    // '%a}' structurally closes the top block ('%b{'), leaving '%a{' unclosed.
    // The lexer reports '%a{' as unclosed; the parser never sees a tag mismatch.
    let err = lex_parse_err("%a{ %b{ content %a}");
    assert!(!err.is_empty(), "expected an error");
    assert!(err.contains('a'), "expected 'a' in error: {}", err);
}

#[test]
fn test_wrong_close_in_three_levels_causes_unclosed() {
    // '%b}' closes the top (Block c), leaving '%a{' and '%b{' both unclosed.
    let err = lex_parse_err("%a{ %b{ %c{ deep %b}");
    assert!(!err.is_empty(), "expected errors");
    // Both 'a' and 'b' should be reported as unclosed.
    assert!(err.contains('a'), "expected 'a' in error: {}", err);
    assert!(err.contains('b'), "expected 'b' in error: {}", err);
}

#[test]
fn test_mismatch_tagged_close_on_anonymous_open() {
    let err = lex_parse("%{ content %foo}").unwrap_err();
    assert!(err.contains("anonymous"), "expected 'anonymous' in error: {}", err);
    assert!(err.contains("foo"), "expected 'foo' in error: {}", err);
}

#[test]
fn test_mismatch_anonymous_close_on_tagged_open() {
    let err = lex_parse("%foo{ content %}").unwrap_err();
    assert!(err.contains("foo"), "expected 'foo' in error: {}", err);
    assert!(err.contains("anonymous"), "expected 'anonymous' in error: {}", err);
}

// -----------------------------------------------------------------------
// Unclosed block errors
// -----------------------------------------------------------------------

#[test]
fn test_unclosed_tagged_reports_tag_name() {
    let err = lex_parse("%myblock{ no close").unwrap_err();
    assert!(err.contains("myblock"), "expected tag name: {}", err);
}

#[test]
fn test_unclosed_innermost_reported() {
    // The innermost block ('%c{') has no '%' chars after it, so run_block_state
    // consumes the trailing text as Text and returns false (self-pops at EOF).
    // The remaining unclosed blocks on the stack are '%a{' and '%b{'.
    // The parser sees the error from lex stage; use lex_parse_err.
    let err = lex_parse_err("%a{ %b{ %c{ deep");
    assert!(!err.is_empty(), "expected an error");
    assert!(err.contains('a') || err.contains('b'), "expected 'a' or 'b' in error: {}", err);
}

#[test]
fn test_deeply_unclosed_reports_remaining() {
    // '%c{' pops naturally at EOF; '%a{' and '%b{' remain on the lexer stack.
    // Both are reported as unclosed (not '%c{', which was already popped).
    let err = lex_parse_err("%a{ %b{ %c{ deep");
    assert!(err.contains('a'), "expected '%a{{' in error: {}", err);
    assert!(err.contains('b'), "expected '%b{{' in error: {}", err);
}

#[test]
fn test_unclosed_after_valid_content() {
    let err = lex_parse("some text %foo{ more text").unwrap_err();
    assert!(err.contains("foo"), "expected 'foo': {}", err);
}

// -----------------------------------------------------------------------
// Unclosed macro / lex-level errors
// -----------------------------------------------------------------------

#[test]
fn test_unclosed_macro_args_lex_error() {
    let (_, errors) = Lexer::new("%foo(arg", '%', 0).lex();
    assert!(!errors.is_empty(), "expected a lex error for unclosed macro args");
    let msg = &errors[0].message;
    assert!(
        msg.contains("macro") || msg.contains("Unclosed"),
        "unexpected error message: {}",
        msg
    );
}

#[test]
fn test_unclosed_block_comment_lex_error() {
    let (_, errors) = Lexer::new("%/* not closed", '%', 0).lex();
    assert!(!errors.is_empty(), "expected lex error for unclosed comment");
    assert!(errors[0].message.contains("comment") || errors[0].message.contains("Unclosed"));
}

// -----------------------------------------------------------------------
// EOF token must not appear as a Text node in the AST
// -----------------------------------------------------------------------

#[test]
fn test_eof_token_not_in_ast() {
    use crate::types::NodeKind;
    let src = "hello";
    let (tokens, _) = Lexer::new(src, '%', 0).lex();
    let mut parser = Parser::new();
    parser.parse(&tokens, src.as_bytes(), &LineIndex::new(src)).unwrap();
    // Walk all nodes — none should have NodeKind::Text with length 0 from EOF.
    // More directly: the EOF token has length 0 and kind EOF.
    // If it were added as Text it would appear as a zero-length Text child of root.
    let root = parser.get_node(0).unwrap();
    for &child_idx in &root.parts {
        let child = parser.get_node(child_idx).unwrap();
        // EOF token (length 0, kind Text) must not appear
        assert!(
            !(child.kind == NodeKind::Text && child.token.length == 0
                && child.token.pos == src.len()),
            "EOF token leaked into AST as Text node"
        );
    }
}

// -----------------------------------------------------------------------
// Root block end_pos is set correctly
// -----------------------------------------------------------------------

#[test]
fn test_root_end_pos_set() {
    let src = "hello world";
    let (tokens, _) = Lexer::new(src, '%', 0).lex();
    let mut parser = Parser::new();
    parser.parse(&tokens, src.as_bytes(), &LineIndex::new(src)).unwrap();
    let root = parser.get_node(0).unwrap();
    assert_eq!(root.end_pos, src.len(), "root end_pos should equal input length");
}

#[test]
fn test_root_end_pos_with_block() {
    let src = "%foo{ content %foo}";
    let (tokens, _) = Lexer::new(src, '%', 0).lex();
    let mut parser = Parser::new();
    parser.parse(&tokens, src.as_bytes(), &LineIndex::new(src)).unwrap();
    let root = parser.get_node(0).unwrap();
    assert_eq!(root.end_pos, src.len());
}
