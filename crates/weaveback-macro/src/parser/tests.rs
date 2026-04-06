// src/parser/tests.rs

use crate::lexer::Lexer;
use crate::line_index::LineIndex;
use crate::parser::Parser;
use crate::types::{NodeKind, ParseNode, Token, TokenKind};
use super::{block_tag_label, ParseContext};
use std::env;

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

fn parse_ok(src: &str, sigil: char) -> Parser {
    let (tokens, lex_errors) = Lexer::new(src, sigil, 0).lex();
    assert!(lex_errors.is_empty(), "unexpected lex errors: {:?}", lex_errors);
    let line_index = LineIndex::new(src);
    let mut parser = Parser::new();
    parser
        .parse(&tokens, src.as_bytes(), &line_index)
        .unwrap_or_else(|e| panic!("unexpected parse error for {src:?}: {e}"));
    parser
}

fn assert_parse_tree_invariants(parser: &Parser, source_len: usize, label: &str) {
    let root_idx = parser.get_root_index().expect("root should exist");
    let root = parser.get_node(root_idx).expect("root node should exist");
    assert_eq!(root.kind, NodeKind::Block, "{label}: root must be a block");
    assert_eq!(root.token.pos, 0, "{label}: root token pos should be zero");
    assert_eq!(root.end_pos, source_len, "{label}: root end_pos should reach source end");

    for idx in 0.. {
        let Some(node) = parser.get_node(idx) else {
            break;
        };
        assert!(
            node.token.pos <= source_len,
            "{label}: node {idx} starts past end: {node:?}"
        );
        assert!(
            node.end_pos >= node.token.end(),
            "{label}: node {idx} end_pos {} must cover token end {}",
            node.end_pos,
            node.token.end()
        );
        assert!(
            node.end_pos <= source_len,
            "{label}: node {idx} end_pos {} past source end {}",
            node.end_pos,
            source_len
        );
        for &child_idx in &node.parts {
            let child = parser
                .get_node(child_idx)
                .unwrap_or_else(|| panic!("{label}: child index {child_idx} missing"));
            assert!(
                child.token.pos >= node.token.pos,
                "{label}: child {child_idx} starts before parent {idx}"
            );
            assert!(
                child.end_pos <= node.end_pos,
                "{label}: child {child_idx} ends after parent {idx}"
            );
        }
    }
}

fn pseudo_fuzz_input(seed: u64, len: usize) -> String {
    let alphabet = [
        "a", "b", "x", "0", "_", " ", "\n", ",", "=", "(", ")", "{", "}", "/", "-", "#", "%",
        "§", "α", "世", "界",
    ];
    let mut state = seed;
    let mut out = String::new();
    while out.len() < len {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        let idx = (state % alphabet.len() as u64) as usize;
        out.push_str(alphabet[idx]);
    }
    while out.len() > len {
        out.pop();
    }
    out
}

fn stress_iterations() -> usize {
    env::var("WB_MACRO_STRESS_ITERS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(3000)
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
    assert!(TokenKind::try_from(14).is_ok());
    assert!(TokenKind::try_from(-1).is_err());
    assert!(TokenKind::try_from(15).is_err());
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

#[test]
fn test_block_tag_label_formats_named_and_anonymous_blocks() {
    assert_eq!(block_tag_label("", '{'), "(anonymous)");
    assert_eq!(block_tag_label("", '}'), "(anonymous)");
    assert_eq!(block_tag_label("foo", '{'), "%foo{");
    assert_eq!(block_tag_label("foo", '}'), "%foo}");
}

#[test]
fn test_parse_context_tags_match_handles_named_anonymous_and_oob() {
    let src = "%foo{ x %foo}";
    let index = LineIndex::new(src);
    let ctx = ParseContext::new(src.as_bytes(), &index);

    assert!(ctx.tags_match((1, 3), (9, 3)));
    assert!(ctx.tags_match((0, 0), (0, 0)));
    assert!(!ctx.tags_match((1, 3), (0, 0)));
    assert!(!ctx.tags_match((1, 3), (99, 3)));
    assert!(!ctx.tags_match((99, 3), (1, 3)));
}

#[test]
fn test_parse_context_tag_str_handles_valid_and_invalid_spans() {
    let src = "%foo{ x %foo}";
    let index = LineIndex::new(src);
    let ctx = ParseContext::new(src.as_bytes(), &index);

    assert_eq!(ctx.tag_str(1, 3), "foo");
    assert_eq!(ctx.tag_str(0, 0), "");
}

#[test]
fn test_block_tag_uses_utf8_sigil_width() {
    let src = "§name{ body §name}";
    let token = Token {
        kind: TokenKind::BlockOpen,
        src: 0,
        pos: 0,
        length: "§name{".len(),
    };
    let (tag_pos, tag_len) = Parser::block_tag(&token, src.as_bytes());
    assert_eq!(&src.as_bytes()[tag_pos..tag_pos + tag_len], "name".as_bytes());
}

#[test]
fn test_parse_token_from_parts_errors_are_precise() {
    let err = Parser::parse_token_from_parts(vec!["1", "2", "3"]).unwrap_err();
    assert!(err.to_string().contains("Invalid token data"));

    let err = Parser::parse_token_from_parts(vec!["src", "2", "3", "4"]).unwrap_err();
    assert!(err.to_string().contains("Invalid src"));

    let err = Parser::parse_token_from_parts(vec!["1", "kind", "3", "4"]).unwrap_err();
    assert!(err.to_string().contains("Invalid kind"));

    let err = Parser::parse_token_from_parts(vec!["1", "2", "pos", "4"]).unwrap_err();
    assert!(err.to_string().contains("Invalid pos"));

    let err = Parser::parse_token_from_parts(vec!["1", "2", "3", "length"]).unwrap_err();
    assert!(err.to_string().contains("Invalid length"));
}

#[test]
fn test_parse_tokens_reads_multiple_lines() {
    let lines = vec![
        Ok("0,0,0,5".to_string()),
        Ok("0,14,5,0".to_string()),
    ];
    let tokens = Parser::parse_tokens(lines.into_iter()).unwrap();
    assert_eq!(tokens.len(), 2);
    assert_eq!(tokens[0].kind, TokenKind::Text);
    assert_eq!(tokens[1].kind, TokenKind::EOF);
}

#[test]
fn test_parse_tokens_reports_line_read_errors() {
    let lines = vec![Err(std::io::Error::other("boom"))];
    let err = Parser::parse_tokens(lines.into_iter()).unwrap_err();
    assert!(err.to_string().contains("Failed to read line"));
}

#[test]
fn test_get_node_info_and_strip_ending_space() {
    let mut parser = Parser::new();
    let idx = parser.add_node(ParseNode {
        kind: NodeKind::Text,
        src: 0,
        token: Token {
            kind: TokenKind::Text,
            src: 0,
            pos: 0,
            length: 6,
        },
        end_pos: 6,
        parts: Vec::new(),
    });
    let (node, kind) = parser.get_node_info(idx).expect("node info should exist");
    assert_eq!(kind, NodeKind::Text);
    assert_eq!(node.end_pos, 6);

    parser.strip_ending_space(b"abc \n\t", idx).unwrap();
    let node = parser.get_node(idx).unwrap();
    assert_eq!(node.token.length, 3);
    assert_eq!(node.end_pos, 3);
}

#[test]
fn test_strip_ending_space_is_noop_out_of_bounds() {
    let mut parser = Parser::new();
    let idx = parser.add_node(ParseNode {
        kind: NodeKind::Text,
        src: 0,
        token: Token {
            kind: TokenKind::Text,
            src: 0,
            pos: 100,
            length: 5,
        },
        end_pos: 105,
        parts: Vec::new(),
    });
    parser.strip_ending_space(b"short", idx).unwrap();
    let node = parser.get_node(idx).unwrap();
    assert_eq!(node.token.length, 5);
    assert_eq!(node.end_pos, 105);
}

#[test]
fn test_process_ast_and_build_ast_on_simple_input() {
    let src = "hello %(name)";
    let parser = parse_ok(src, '%');
    let ast = parser.build_ast().expect("build_ast should succeed");
    assert_eq!(ast.kind, NodeKind::Block);
    assert_eq!(ast.end_pos, src.len());

    let mut parser = parse_ok(src, '%');
    let ast = parser
        .process_ast(src.as_bytes())
        .expect("process_ast should succeed");
    assert_eq!(ast.kind, NodeKind::Block);
    assert_eq!(ast.end_pos, src.len());
}

#[test]
fn test_parse_tree_invariants_under_pseudo_fuzz() {
    for seed in 0..200u64 {
        let input = pseudo_fuzz_input(seed * 19 + 3, 96);
        let (tokens, _lex_errors) = Lexer::new(&input, '%', 0).lex();
        let mut parser = Parser::new();
        let line_index = LineIndex::new(&input);
        if parser.parse(&tokens, input.as_bytes(), &line_index).is_ok() {
            assert_parse_tree_invariants(&parser, input.len(), &format!("seed={seed}"));
        }
    }
}

#[test]
fn test_parse_tree_invariants_under_unicode_sigil_pseudo_fuzz() {
    for seed in 0..120u64 {
        let mut input = pseudo_fuzz_input(seed * 29 + 5, 96);
        input.push_str(" §foo(世界) §bar{ x §bar}");
        let (tokens, _lex_errors) = Lexer::new(&input, '§', 0).lex();
        let mut parser = Parser::new();
        let line_index = LineIndex::new(&input);
        if parser.parse(&tokens, input.as_bytes(), &line_index).is_ok() {
            assert_parse_tree_invariants(&parser, input.len(), &format!("unicode-seed={seed}"));
        }
    }
}

#[test]
#[ignore = "long-running deterministic stress harness"]
fn stress_parse_tree_invariants_many_inputs() {
    let iterations = stress_iterations();
    for seed in 0..iterations as u64 {
        let len = ((seed as usize * 29) % 384) + 1;
        let input = pseudo_fuzz_input(seed ^ 0xA076_1D64_78BD_642F, len);
        let (tokens, lex_errors) = Lexer::new(&input, '%', 0).lex();
        if !lex_errors.is_empty() {
            continue;
        }
        let line_index = LineIndex::new(&input);
        let mut parser = Parser::new();
        if parser.parse(&tokens, input.as_bytes(), &line_index).is_ok() {
            assert_parse_tree_invariants(&parser, input.len(), "stress-many");
        }
    }
}

#[test]
#[ignore = "long-running deterministic stress harness"]
fn stress_parse_tree_invariants_many_inputs_unicode_sigil() {
    let iterations = stress_iterations();
    for seed in 0..iterations as u64 {
        let len = ((seed as usize * 41) % 384) + 1;
        let input = pseudo_fuzz_input(seed ^ 0xE703_7ED1_A0B4_28DB, len);
        let (tokens, lex_errors) = Lexer::new(&input, '§', 0).lex();
        if !lex_errors.is_empty() {
            continue;
        }
        let line_index = LineIndex::new(&input);
        let mut parser = Parser::new();
        if parser.parse(&tokens, input.as_bytes(), &line_index).is_ok() {
            assert_parse_tree_invariants(&parser, input.len(), "stress-unicode");
        }
    }
}
