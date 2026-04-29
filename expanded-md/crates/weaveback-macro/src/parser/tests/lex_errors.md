---
title: |-
  Parser Lex Error Tests
description: |-
  Literate source for crates/weaveback-macro/src/parser/tests/lex_errors.rs
toc: left
toclevels: 3
---
# Parser Lex Error Tests

```rust
// <[@file weaveback-macro/src/parser/tests/lex_errors.rs]>=
// weaveback-macro/src/parser/tests/lex_errors.rs
// I'd Really Rather You Didn't edit this generated file.

use super::*;

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

// @
```

