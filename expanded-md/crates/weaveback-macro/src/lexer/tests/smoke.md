---
title: |-
  Lexer Smoke Tests
description: |-
  Literate source for crates/weaveback-macro/src/lexer/tests/smoke.rs
toc: left
toclevels: 3
---
# Lexer Smoke Tests

```rust
// <<@file weaveback-macro/src/lexer/tests/smoke.rs>>=
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

// @
```

