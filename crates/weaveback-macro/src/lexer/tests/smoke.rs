// weaveback-macro/src/lexer/tests/smoke.rs
// I'd Really Rather You Didn't edit this generated file.

use super::*;

#[test]
fn test_no_error() {
    let input = "Hello %macro(arg)";
    let tokens_res = collect_tokens_with_timeout(input);
    assert!(tokens_res.is_ok());
    let tokens = tokens_res.unwrap();
    assert!(!tokens.is_empty());
}

