// crates/weaveback-macro/src/lexer/tests.rs

use crate::lexer::Lexer;
use crate::types::{Token, TokenKind};

/// Collect tokens from the lexer (non-EOF tokens only).
fn collect_tokens_with_timeout(input: &str) -> Result<Vec<Token>, String> {
    collect_tokens_with_sigil(input, '%')
}

fn collect_tokens_with_sigil(input: &str, sigil: char) -> Result<Vec<Token>, String> {
    let (tokens, errors) = Lexer::new(input, sigil, 0).lex();
    if !errors.is_empty() {
        // Errors are non-fatal for these tests; just return what was produced.
        let _ = errors;
    }
    Ok(tokens
        .into_iter()
        .filter(|t| t.kind != TokenKind::EOF)
        .collect())
}

/// Helper to assert tokens match an expected sequence of (TokenKind, &str).
/// We compare both `kind` and the `length` of the text (since we can't store real text easily).
fn assert_tokens(input: &str, expected: &[(TokenKind, &str)]) {
    let result = collect_tokens_with_timeout(input).expect("Failed to collect tokens");
    let tokens = result;

    assert_eq!(
        tokens.len(),
        expected.len(),
        "Wrong number of tokens: expected {}, got {}. Tokens: {:?}",
        expected.len(),
        tokens.len(),
        tokens
    );

    for (i, (token, (exp_kind, exp_text))) in tokens.iter().zip(expected.iter()).enumerate() {
        assert_eq!(
            token.kind, *exp_kind,
            "Token {} kind mismatch: expected {:?}, got {:?}",
            i, exp_kind, token.kind
        );
        let got_len = token.length;
        let exp_len = exp_text.len();
        assert_eq!(
            got_len, exp_len,
            "Token {} length mismatch: expected {}, got {} (expected text='{}')",
            i, exp_len, got_len, exp_text
        );
    }
}

fn assert_tokens_with_sigil(input: &str, sigil: char, expected: &[(TokenKind, &str)]) {
    let result = collect_tokens_with_sigil(input, sigil)
        .expect("Failed to collect tokens with custom sigil");
    let tokens = result;

    assert_eq!(
        tokens.len(),
        expected.len(),
        "Wrong number of tokens: expected {}, got {}. Tokens: {:?}",
        expected.len(),
        tokens.len(),
        tokens
    );

    for (i, (token, (exp_kind, exp_text))) in tokens.iter().zip(expected.iter()).enumerate() {
        assert_eq!(
            token.kind, *exp_kind,
            "Token {} kind mismatch: expected {:?}, got {:?}",
            i, exp_kind, token.kind
        );
        let got_len = token.length;
        let exp_len = exp_text.len();
        assert_eq!(
            got_len, exp_len,
            "Token {} length mismatch: expected {}, got {} (expected text='{}')",
            i, exp_len, got_len, exp_text
        );
    }
}

//-------------------------------------------------------------------------
// Tests
//-------------------------------------------------------------------------

#[test]
fn test_error_cases() {
    assert_tokens(
        "%{incomplete",
        &[
            (TokenKind::BlockOpen, "%{"),
            (TokenKind::Text, "incomplete"),
        ],
    );
    assert_tokens(
        "%macro(incomplete",
        &[
            (TokenKind::Macro, "%macro("),
            (TokenKind::Ident, "incomplete"),
        ],
    );
    assert_tokens(
        "%/* unfinished",
        &[
            (TokenKind::CommentOpen, "%/*"),
            (TokenKind::Text, " unfinished"),
        ],
    );
}

#[test]
fn test_bare_percent_inside_comment() {
    assert_tokens(
        "%/* 100% done %*/",
        &[
            (TokenKind::CommentOpen, "%/*"),
            (TokenKind::Text, " 100% done "),
            (TokenKind::CommentClose, "%*/"),
        ],
    );
    let (_, errors) = Lexer::new("%/* 100% done %*/", '%', 0).lex();
    assert!(errors.is_empty(), "unexpected errors: {:?}", errors);
}

#[test]
fn test_nested_comment() {
    let input = "%/* outer comment %/* inner %*/ outer %*/";
    assert_tokens(
        input,
        &[
            (TokenKind::CommentOpen, "%/*"),
            (TokenKind::Text, " outer comment "),
            (TokenKind::CommentOpen, "%/*"),
            (TokenKind::Text, " inner "),
            (TokenKind::CommentClose, "%*/"),
            (TokenKind::Text, " outer "),
            (TokenKind::CommentClose, "%*/"),
        ],
    );
}

#[test]
fn test_unfinished_sigil() {
    assert_tokens("%something", &[(TokenKind::Text, "%something")]);
}

#[test]
fn test_percent_identifier_no_error() {
    let (_, errors) = Lexer::new("%something", '%', 0).lex();
    assert!(errors.is_empty(), "expected no errors for %identifier, got: {:?}", errors);
}

#[test]
fn test_percent_identifier_mid_text() {
    assert_tokens(
        "%something more text",
        &[
            (TokenKind::Text, "%something"),
            (TokenKind::Text, " more text"),
        ],
    );
}

#[test]
fn test_printf_format_specifiers() {
    let input = "%d %s %f";
    let (_, errors) = Lexer::new(input, '%', 0).lex();
    assert!(errors.is_empty(), "expected no errors for printf specifiers, got: {:?}", errors);
    assert_tokens(
        input,
        &[
            (TokenKind::Text, "%d"),
            (TokenKind::Text, " "),
            (TokenKind::Text, "%s"),
            (TokenKind::Text, " "),
            (TokenKind::Text, "%f"),
        ],
    );
}

#[test]
fn test_simple_completion() {
    let result = collect_tokens_with_timeout("a");
    assert!(result.is_ok());
}

#[test]
fn test_basic_tokens() {
    assert_tokens(
        "Hello %name(world)",
        &[
            (TokenKind::Text, "Hello "),
            (TokenKind::Macro, "%name("),
            (TokenKind::Ident, "world"),
            (TokenKind::CloseParen, ")"),
        ],
    );
}

#[test]
fn test_comments() {
    assert_tokens(
        "text %// line comment\nmore text",
        &[
            (TokenKind::Text, "text "),
            (TokenKind::LineComment, "%// line comment\n"),
            (TokenKind::Text, "more text"),
        ],
    );
    assert_tokens(
        "before %/* multi\nline %*/ after",
        &[
            (TokenKind::Text, "before "),
            (TokenKind::CommentOpen, "%/*"),
            (TokenKind::Text, " multi\nline "),
            (TokenKind::CommentClose, "%*/"),
            (TokenKind::Text, " after"),
        ],
    );
}

#[test]
fn test_nested_blocks() {
    assert_tokens(
        "%{outer %{inner%}%}",
        &[
            (TokenKind::BlockOpen, "%{"),
            (TokenKind::Text, "outer "),
            (TokenKind::BlockOpen, "%{"),
            (TokenKind::Text, "inner"),
            (TokenKind::BlockClose, "%}"),
            (TokenKind::BlockClose, "%}"),
        ],
    );
}

#[test]
fn test_verbatim_blocks_are_opaque() {
    let input = "%[outer %macro(x) %(v) %// not a comment %]";
    let tokens = collect_tokens_with_timeout(input).expect("Failed to collect tokens");

    assert_eq!(tokens.first().map(|t| t.kind), Some(TokenKind::VerbatimOpen));
    assert_eq!(tokens.last().map(|t| t.kind), Some(TokenKind::VerbatimClose));
    assert!(tokens[1..tokens.len() - 1]
        .iter()
        .all(|t| t.kind == TokenKind::Text));

    let inner = &input[2..input.len() - 2];
    let rebuilt = tokens[1..tokens.len() - 1]
        .iter()
        .map(|t| &input[t.pos..t.pos + t.length])
        .collect::<String>();
    assert_eq!(rebuilt, inner);
}

#[test]
fn test_nested_tagged_verbatim_blocks() {
    assert_tokens(
        "%py[one %inner[two%inner] three%py]",
        &[
            (TokenKind::VerbatimOpen, "%py["),
            (TokenKind::Text, "one "),
            (TokenKind::VerbatimOpen, "%inner["),
            (TokenKind::Text, "two"),
            (TokenKind::VerbatimClose, "%inner]"),
            (TokenKind::Text, " three"),
            (TokenKind::VerbatimClose, "%py]"),
        ],
    );
}

#[test]
fn test_macro_with_args() {
    assert_tokens(
        "%func(a, b, c)",
        &[
            (TokenKind::Macro, "%func("),
            (TokenKind::Ident, "a"),
            (TokenKind::Comma, ","),
            (TokenKind::Space, " "),
            (TokenKind::Ident, "b"),
            (TokenKind::Comma, ","),
            (TokenKind::Space, " "),
            (TokenKind::Ident, "c"),
            (TokenKind::CloseParen, ")"),
        ],
    );
}

#[test]
fn test_unicode() {
    assert_tokens(
        "Hello 世界 %macro(名前)",
        &[
            (TokenKind::Text, "Hello 世界 "),
            (TokenKind::Macro, "%macro("),
            (TokenKind::Text, "名前"),
            (TokenKind::CloseParen, ")"),
        ],
    );
}

#[test]
fn test_unicode_sigil() {
    assert_tokens_with_sigil(
        "§macro(名前) §§done §/* note §*/",
        '§',
        &[
            (TokenKind::Macro, "§macro("),
            (TokenKind::Text, "名前"),
            (TokenKind::CloseParen, ")"),
            (TokenKind::Text, " "),
            (TokenKind::Special, "§§"),
            (TokenKind::Text, "done "),
            (TokenKind::CommentOpen, "§/*"),
            (TokenKind::Text, " note "),
            (TokenKind::CommentClose, "§*/"),
        ],
    );
}

#[test]
fn test_sigil_sequences() {
    assert_tokens(
        "%%double",
        &[(TokenKind::Special, "%%"), (TokenKind::Text, "double")],
    );
}

#[test]
fn test_comment_styles() {
    assert_tokens(
        "%# hash comment\n%// double slash\n%-- dash comment",
        &[
            (TokenKind::LineComment, "%# hash comment\n"),
            (TokenKind::LineComment, "%// double slash\n"),
            (TokenKind::LineComment, "%-- dash comment"),
        ],
    );
}

#[test]
fn test_lexer_completion() {
    assert_tokens("", &[]);
    assert_tokens("a", &[(TokenKind::Text, "a")]);
    assert_tokens(
        "text%",
        &[(TokenKind::Text, "text"), (TokenKind::Text, "%")],
    );
    assert_tokens(
        "text %",
        &[(TokenKind::Text, "text "), (TokenKind::Text, "%")],
    );
}

#[test]
fn test_lexer_buffer_boundaries() {
    assert_tokens(
        "%token( rest",
        &[
            (TokenKind::Macro, "%token("),
            (TokenKind::Space, " "),
            (TokenKind::Ident, "rest"),
        ],
    );
    assert_tokens(
        "start %token(",
        &[(TokenKind::Text, "start "), (TokenKind::Macro, "%token(")],
    );
    assert_tokens(
        " % ",
        &[
            (TokenKind::Text, " "),
            (TokenKind::Text, "%"),
            (TokenKind::Text, " "),
        ],
    );
}

#[test]
fn test_leading_trailing_spaces() {
    assert_tokens("   Hello   ", &[(TokenKind::Text, "   Hello   ")]);
}

#[test]
fn test_macro_without_arguments() {
    assert_tokens(
        "%macro()",
        &[(TokenKind::Macro, "%macro("), (TokenKind::CloseParen, ")")],
    );
}

#[test]
fn test_comment_immediately_following_block() {
    assert_tokens(
        "%{ hi %}%//comment\nleftover",
        &[
            (TokenKind::BlockOpen, "%{"),
            (TokenKind::Text, " hi "),
            (TokenKind::BlockClose, "%}"),
            (TokenKind::LineComment, "%//comment\n"),
            (TokenKind::Text, "leftover"),
        ],
    );
}

#[test]
fn test_multiple_unmatched_percents() {
    assert_tokens(
        "text % some % more",
        &[
            (TokenKind::Text, "text "),
            (TokenKind::Text, "%"),
            (TokenKind::Text, " some "),
            (TokenKind::Text, "%"),
            (TokenKind::Text, " more"),
        ],
    );
}

#[test]
fn test_unicode_identifier_in_macro() {
    assert_tokens(
        "%macro(привет)",
        &[
            (TokenKind::Macro, "%macro("),
            (TokenKind::Text, "привет"),
            (TokenKind::CloseParen, ")"),
        ],
    );
}

#[test]
fn test_trailing_whitespace_before_comment() {
    assert_tokens(
        "%{ hi %}  %//comment\nleftover",
        &[
            (TokenKind::BlockOpen, "%{"),
            (TokenKind::Text, " hi "),
            (TokenKind::BlockClose, "%}"),
            (TokenKind::Text, "  "),
            (TokenKind::LineComment, "%//comment\n"),
            (TokenKind::Text, "leftover"),
        ],
    );
}

#[test]
fn test_named_block() {
    assert_tokens(
        "%blockName{ inside content %blockName}",
        &[
            (TokenKind::BlockOpen, "%blockName{"),
            (TokenKind::Text, " inside content "),
            (TokenKind::BlockClose, "%blockName}"),
        ],
    );
}

#[test]
fn test_simple_var() {
    assert_tokens("%(foo)", &[(TokenKind::Var, "%(foo)")]);
}

#[test]
fn test_var_in_block() {
    assert_tokens(
        "%{ hello %(abc) world %}",
        &[
            (TokenKind::BlockOpen, "%{"),
            (TokenKind::Text, " hello "),
            (TokenKind::Var, "%(abc)"),
            (TokenKind::Text, " world "),
            (TokenKind::BlockClose, "%}"),
        ],
    );
}

#[test]
fn test_var_in_macro() {
    assert_tokens(
        "%func( %(myVar), 123 )",
        &[
            (TokenKind::Macro, "%func("),
            (TokenKind::Space, " "),
            (TokenKind::Var, "%(myVar)"),
            (TokenKind::Comma, ","),
            (TokenKind::Space, " "),
            (TokenKind::Text, "123"),
            (TokenKind::Space, " "),
            (TokenKind::CloseParen, ")"),
        ],
    );
}

#[test]
fn test_multiple_vars_in_text() {
    assert_tokens(
        "Here %(x) and %(y) then done",
        &[
            (TokenKind::Text, "Here "),
            (TokenKind::Var, "%(x)"),
            (TokenKind::Text, " and "),
            (TokenKind::Var, "%(y)"),
            (TokenKind::Text, " then done"),
        ],
    );
}

#[test]
fn test_incomplete_var() {
    assert_tokens(
        "%( %(abc something %( )",
        &[
            (TokenKind::Text, "%("),
            (TokenKind::Text, " "),
            (TokenKind::Text, "%(abc"),
            (TokenKind::Text, " something "),
            (TokenKind::Text, "%("),
            (TokenKind::Text, " )"),
        ],
    );
}

#[test]
fn test_real_world_macro_with_block_and_vars() {
    let input = r#"%def(shortTopCase,  case,  ch, impl, %blk{
// <[Macro_case]>=
case %(ch): {%(impl)}
// $$
%blk})"#;

    assert_tokens(
        input,
        &[
            (TokenKind::Macro, "%def("),
            (TokenKind::Ident, "shortTopCase"),
            (TokenKind::Comma, ","),
            (TokenKind::Space, "  "),
            (TokenKind::Ident, "case"),
            (TokenKind::Comma, ","),
            (TokenKind::Space, "  "),
            (TokenKind::Ident, "ch"),
            (TokenKind::Comma, ","),
            (TokenKind::Space, " "),
            (TokenKind::Ident, "impl"),
            (TokenKind::Comma, ","),
            (TokenKind::Space, " "),
            (TokenKind::BlockOpen, "%blk{"),
            (TokenKind::Text, "\n// <[Macro_case]>=\ncase "),
            (TokenKind::Var, "%(ch)"),
            (TokenKind::Text, ": {"),
            (TokenKind::Var, "%(impl)"),
            (TokenKind::Text, "}\n// $$\n"),
            (TokenKind::BlockClose, "%blk}"),
            (TokenKind::CloseParen, ")"),
        ],
    );
}

#[test]
fn test_escaped_pubfunc_not_macro() {
    assert_tokens(
        "%%pubfunc(%(name), Allocator* allo, %%{",
        &[
            (TokenKind::Special, "%%"),
            (TokenKind::Text, "pubfunc("),
            (TokenKind::Var, "%(name)"),
            (TokenKind::Text, ", Allocator* allo, "),
            (TokenKind::Special, "%%"),
            (TokenKind::Text, "{"),
        ],
    );
}

#[test]
fn test_no_error() {
    let input = "Hello %macro(arg)";
    let tokens_res = collect_tokens_with_timeout(input);
    assert!(tokens_res.is_ok());
    let tokens = tokens_res.unwrap();
    assert!(!tokens.is_empty());
}
