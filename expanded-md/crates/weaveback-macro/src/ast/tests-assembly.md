---
title: |-
  weaveback-macro AST tests
description: |-
  Literate source for crates/weaveback-macro/src/ast/tests.rs
toc: left
toclevels: 3
---
# weaveback-macro AST tests

The AST test root keeps shared parser/node helpers and delegates behavior checks to focused child modules. This keeps the Rust test file small while preserving the same private test-only helpers for all children.

```rust
// <[@file weaveback-macro/src/ast/tests.rs]>=
// weaveback-macro/src/ast/tests.rs
// I'd Really Rather You Didn't edit this generated file.

mod params_basic;
mod params_nested;
mod params_dfa;
mod node_kinds;
mod serialization;
mod strip_comments;
mod pipeline;

use super::*;
use crate::ParseNode;
use crate::parser::Parser;
use crate::types::{NodeKind, Token, TokenKind};

/// Helper to create a basic token
fn t(kind: TokenKind, pos: usize, length: usize) -> Token {
    Token {
        src: 0,
        kind,
        pos,
        length,
    }
}

/// Helper to create a node and add it to parser, returning its index
fn n(parser: &mut Parser, kind: NodeKind, pos: usize, length: usize, parts: Vec<usize>) -> usize {
    parser.add_node(ParseNode {
        kind,
        src: 0,
        token: t(TokenKind::Text, pos, length),
        end_pos: pos + length,
        parts,
    })
}

/// Builder to create sequence of nodes
struct NodeBuilder {
    pos: usize,
    nodes: Vec<(NodeKind, usize, usize)>, // Store (kind, pos, length)
}

impl NodeBuilder {
    fn new() -> Self {
        Self {
            pos: 0,
            nodes: Vec::new(),
        }
    }

    fn space(&mut self, length: usize) -> usize {
        let idx = self.nodes.len();
        self.nodes.push((NodeKind::Space, self.pos, length));
        self.pos += length;
        idx
    }

    fn text(&mut self, length: usize) -> usize {
        let idx = self.nodes.len();
        self.nodes.push((NodeKind::Text, self.pos, length));
        self.pos += length;
        idx
    }

    fn ident(&mut self, length: usize) -> usize {
        let idx = self.nodes.len();
        self.nodes.push((NodeKind::Ident, self.pos, length));
        self.pos += length;
        idx
    }

    fn comment(&mut self, length: usize) -> usize {
        let idx = self.nodes.len();
        self.nodes.push((NodeKind::LineComment, self.pos, length));
        self.pos += length;
        idx
    }

    fn equals(&mut self) -> usize {
        let idx = self.nodes.len();
        self.nodes.push((NodeKind::Equal, self.pos, 1));
        self.pos += 1;
        idx
    }

    fn build_nodes(&self, parser: &mut Parser) -> Vec<usize> {
        let mut indices = Vec::new();
        for &(kind, pos, length) in &self.nodes {
            indices.push(n(parser, kind, pos, length, vec![]));
        }
        indices
    }

    fn param(&self, parser: &mut Parser) -> usize {
        let parts = self.build_nodes(parser);
        n(parser, NodeKind::Param, 0, self.pos, parts)
    }
}

/// Helper to verify AST node structure
fn check_node(node: &ASTNode, expected_kind: NodeKind, expected_parts: usize) {
    assert_eq!(node.kind, expected_kind);
    assert_eq!(node.parts.len(), expected_parts);
}

// @
```

