---
title: |-
  Shared types
toc: left
---
# Shared types

`types.rs` defines the primitive vocabulary shared by every layer of the
pipeline: the token and node enumerations produced by the lexer, the concrete
`Token` / `ParseNode` / `ASTNode` structs consumed by the parser and evaluator,
and `TryFrom<i32>` impls that allow Python and serialised representations to
round-trip through integer discriminants.

## Design rationale

### `repr(u8)` on `TokenKind`

`TokenKind` carries `#[repr(u8)]` so its discriminants are stable integers in
serde round-trips.  The variants fit in a single byte; the `TryFrom<i32>`
impl validates the range on deserialisation.

### `NodeKind::NotUsed = 0`

Discriminant 0 is intentionally left empty.  Python `IntEnum` defaults start
at 1, so keeping 0 unused means `NodeKind::Text = 1` aligns with Python's
`NodeKind.Text = 1` without any manual offset arithmetic.

### `Token` is `Copy`

Tokens carry only an index (`src: u32`), byte offset (`pos: usize`), and
length — no heap allocation.  `Copy` lets the parser stash tokens freely in
`ParseNode` and `ASTNode` without clone overhead.

### `ParseNode` vs `ASTNode`

`ParseNode.parts` stores indices into a flat arena (the parser's `Vec<ParseNode>`).
`ASTNode.parts` stores owned child nodes — the AST is a proper tree.  The
conversion pass in `ast/mod.rs` materialises the arena into the tree.

### `Token::synthetic`

Some structural nodes (the root block, implicit first parameters) have no
corresponding source token.  `Token::synthetic(src, pos)` creates a
zero-length placeholder so that `end_pos` computation remains correct.

## File structure

```rust
// <[@file weaveback-macro/src/types.rs]>=
// weaveback-macro/src/types.rs
// I'd Really Rather You Didn't edit this generated file.

// <[types preamble]>
// <[token kind]>
// <[node kind]>
// <[token struct]>
// <[lexer error]>
// <[parse node]>
// <[ast node]>
// <[try from token kind]>
// <[try from node kind]>
// <[types tests]>

// @
```


## Preamble

```rust
// <[types preamble]>=
// crates/weaveback-macro/src/types.rs

use serde::{Deserialize, Serialize};
use std::convert::TryFrom;
// @
```


## `TokenKind`

Token kinds produced by the lexer.  See
xref:../lexer/lexer.adoc[lexer] for the state machine that emits them.

```rust
// <[token kind]>=
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[repr(u8)]
pub enum TokenKind {
    Text = 0,
    Space = 1,
    Special = 2,
    BlockOpen = 3,
    BlockClose = 4,
    Macro = 5,
    Var = 6,
    Ident = 7,
    Comma = 8,
    CloseParen = 9,
    Equal = 10,
    LineComment = 11,
    CommentOpen = 12,
    CommentClose = 13,
    VerbatimOpen = 14,
    VerbatimClose = 15,
    EOF = 16,
}
// @
```


## `NodeKind`

Node kinds used in `ParseNode` and `ASTNode`.  Discriminant 0 (`NotUsed`) is
reserved so that Python `IntEnum` discriminants (which start at 1 by default)
align without an offset.

```rust
// <[node kind]>=
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum NodeKind {
    /// Discriminant 0 is intentionally reserved so Rust and Python discriminants align:
    /// Python IntEnum starts at 1 by default, matching Text=1 here.
    NotUsed = 0,
    Text = 1,
    Space = 2,
    Ident = 3,
    LineComment = 4,
    BlockComment = 5,
    Var = 6,
    Equal = 7,
    Param = 8,
    Macro = 9,
    Block = 10,
}
// @
```


## `Token`

A lightweight (all `Copy`) source reference.  `src` is an index into the
evaluator's `SourceManager`; `pos` and `length` are byte offsets into that
source string.

```rust
// <[token struct]>=
#[derive(Debug, Clone, Copy)]
pub struct Token {
    pub kind: TokenKind,
    pub src: u32,
    pub pos: usize,
    pub length: usize,
}

impl Token {
    /// One-past-the-end byte offset: `pos + length`.
    pub fn end(&self) -> usize {
        self.pos + self.length
    }

    /// Create a zero-length synthetic token for structural parse nodes that
    /// have no corresponding source token (root block, implicit first param).
    /// `pos` anchors the node in the source for `end_pos` computation.
    pub fn synthetic(src: u32, pos: usize) -> Self {
        Token {
            kind: TokenKind::Text,
            src,
            pos,
            length: 0,
        }
    }
}
// @
```


## `LexerError`

```rust
// <[lexer error]>=
#[derive(Debug)]
pub struct LexerError {
    pub pos: usize,
    pub message: String,
}
// @
```


## `ParseNode`

Produced by the parser; children are stored as indices into the parser's flat
arena.  Converted to `ASTNode` by the post-pass in `ast/mod.rs`.

```rust
// <[parse node]>=
#[derive(Debug, Clone)]
pub struct ParseNode {
    pub kind: NodeKind,
    pub src: u32,
    pub token: Token,
    pub end_pos: usize,
    pub parts: Vec<usize>,
}
// @
```


## `ASTNode`

The materialised tree form.  `parts` contains owned child nodes;
`name` carries the identifier token for `Macro` and `Var` nodes.

```rust
// <[ast node]>=
#[derive(Debug, Clone)]
pub struct ASTNode {
    pub kind: NodeKind,
    pub src: u32,
    pub token: Token,
    pub end_pos: usize,
    pub parts: Vec<ASTNode>,
    pub name: Option<Token>,
}
// @
```


## `TryFrom<i32>` for `TokenKind`

```rust
// <[try from token kind]>=
impl TryFrom<i32> for TokenKind {
    type Error = String;
    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(TokenKind::Text),
            1 => Ok(TokenKind::Space),
            2 => Ok(TokenKind::Special),
            3 => Ok(TokenKind::BlockOpen),
            4 => Ok(TokenKind::BlockClose),
            5 => Ok(TokenKind::Macro),
            6 => Ok(TokenKind::Var),
            7 => Ok(TokenKind::Ident),
            8 => Ok(TokenKind::Comma),
            9 => Ok(TokenKind::CloseParen),
            10 => Ok(TokenKind::Equal),
            11 => Ok(TokenKind::LineComment),
            12 => Ok(TokenKind::CommentOpen),
            13 => Ok(TokenKind::CommentClose),
            14 => Ok(TokenKind::VerbatimOpen),
            15 => Ok(TokenKind::VerbatimClose),
            16 => Ok(TokenKind::EOF),
            _ => Err(format!("Invalid token kind: {}", value)),
        }
    }
}
// @
```


## `TryFrom<i32>` for `NodeKind`

```rust
// <[try from node kind]>=
impl TryFrom<i32> for NodeKind {
    type Error = String;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(NodeKind::NotUsed),
            1 => Ok(NodeKind::Text),
            2 => Ok(NodeKind::Space),
            3 => Ok(NodeKind::Ident),
            4 => Ok(NodeKind::LineComment),
            5 => Ok(NodeKind::BlockComment),
            6 => Ok(NodeKind::Var),
            7 => Ok(NodeKind::Equal),
            8 => Ok(NodeKind::Param),
            9 => Ok(NodeKind::Macro),
            10 => Ok(NodeKind::Block),
            _ => Err(format!("Invalid NodeKind: {value}")),
        }
    }
}
// @
```


## Tests

```rust
// <[types tests]>=
#[cfg(test)]
mod tests {
    use super::{NodeKind, Token, TokenKind};
    use std::convert::TryFrom;

    #[test]
    fn token_end_and_synthetic_are_consistent() {
        let token = Token {
            kind: TokenKind::Macro,
            src: 7,
            pos: 11,
            length: 5,
        };
        assert_eq!(token.end(), 16);

        let synthetic = Token::synthetic(9, 23);
        assert_eq!(synthetic.kind, TokenKind::Text);
        assert_eq!(synthetic.src, 9);
        assert_eq!(synthetic.pos, 23);
        assert_eq!(synthetic.length, 0);
        assert_eq!(synthetic.end(), 23);
    }

    #[test]
    fn token_kind_try_from_accepts_full_range() {
        let expected = [
            TokenKind::Text,
            TokenKind::Space,
            TokenKind::Special,
            TokenKind::BlockOpen,
            TokenKind::BlockClose,
            TokenKind::Macro,
            TokenKind::Var,
            TokenKind::Ident,
            TokenKind::Comma,
            TokenKind::CloseParen,
            TokenKind::Equal,
            TokenKind::LineComment,
            TokenKind::CommentOpen,
            TokenKind::CommentClose,
            TokenKind::VerbatimOpen,
            TokenKind::VerbatimClose,
            TokenKind::EOF,
        ];
        for (i, kind) in expected.into_iter().enumerate() {
            assert_eq!(TokenKind::try_from(i as i32).unwrap(), kind);
        }
    }

    #[test]
    fn token_kind_try_from_rejects_invalid_values() {
        assert!(TokenKind::try_from(-1).is_err());
        assert!(TokenKind::try_from(17).is_err());
        assert!(TokenKind::try_from(99).is_err());
    }

    #[test]
    fn node_kind_try_from_accepts_full_range() {
        let expected = [
            NodeKind::NotUsed,
            NodeKind::Text,
            NodeKind::Space,
            NodeKind::Ident,
            NodeKind::LineComment,
            NodeKind::BlockComment,
            NodeKind::Var,
            NodeKind::Equal,
            NodeKind::Param,
            NodeKind::Macro,
            NodeKind::Block,
        ];
        for (i, kind) in expected.into_iter().enumerate() {
            assert_eq!(NodeKind::try_from(i as i32).unwrap(), kind);
        }
    }

    #[test]
    fn node_kind_try_from_rejects_invalid_values() {
        assert!(NodeKind::try_from(-1).is_err());
        assert!(NodeKind::try_from(11).is_err());
        assert!(NodeKind::try_from(99).is_err());
    }
}
// @
```

