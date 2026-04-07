// src/ast/tests.rs
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

#[test]
fn test_param_identifier_only() {
    let mut parser = Parser::new();
    let mut builder = NodeBuilder::new();
    builder.space(1);
    builder.ident(3);
    builder.space(1);
    let param_idx = builder.param(&mut parser);
    let result = analyze_param(&parser, param_idx).unwrap().unwrap();
    check_node(&result, NodeKind::Param, 2);
    check_node(&result.parts[0], NodeKind::Ident, 0);
    check_node(&result.parts[1], NodeKind::Space, 0);
}

#[test]
fn test_empty_param() {
    // analyze_param returns Some for empty param; trailing-empty trimming
    // happens at Macro level via trim_trailing_empty_params.
    let mut parser = Parser::new();
    let builder = NodeBuilder::new();
    let param_idx = builder.param(&mut parser);
    let result = analyze_param(&parser, param_idx).unwrap();
    assert!(result.is_some());
    assert!(result.unwrap().parts.is_empty());
}

#[test]
fn test_param_with_comments() {
    let mut parser = Parser::new();
    let mut builder = NodeBuilder::new();
    builder.comment(1);
    builder.ident(3);
    builder.comment(1);
    let param_idx = builder.param(&mut parser);
    let result = analyze_param(&parser, param_idx).unwrap().unwrap();
    check_node(&result, NodeKind::Param, 1);
    check_node(&result.parts[0], NodeKind::Ident, 0);
    assert_eq!(result.parts[0].token.pos, 1);
    assert_eq!(result.parts[0].token.length, 3);
}

#[test]
fn test_param_value_only() {
    let mut parser = Parser::new();
    let mut builder = NodeBuilder::new();
    builder.space(1);
    builder.text(3);
    builder.space(1);
    let param_idx = builder.param(&mut parser);
    let result = analyze_param(&parser, param_idx).unwrap().unwrap();
    check_node(&result, NodeKind::Param, 2);
    assert!(result.name.is_none());
    check_node(&result.parts[0], NodeKind::Text, 0);
    check_node(&result.parts[1], NodeKind::Space, 0);
}

#[test]
fn test_param_name_equals_value() {
    let mut parser = Parser::new();
    let mut builder = NodeBuilder::new();
    builder.ident(3);
    builder.equals();
    builder.text(4);
    let param_idx = builder.param(&mut parser);
    let result = analyze_param(&parser, param_idx).unwrap().unwrap();
    check_node(&result, NodeKind::Param, 1);
    assert!(result.name.is_some());
    check_node(&result.parts[0], NodeKind::Text, 0);
    let name = result.name.unwrap();
    assert_eq!(name.pos, 0);
    assert_eq!(name.length, 3);
}

#[test]
fn test_param_equals_without_ident() {
    let mut parser = Parser::new();
    let mut builder = NodeBuilder::new();
    builder.equals();
    builder.text(4);
    let param_idx = builder.param(&mut parser);
    let result = analyze_param(&parser, param_idx).unwrap().unwrap();
    check_node(&result, NodeKind::Param, 2);
    assert!(result.name.is_none());
    check_node(&result.parts[0], NodeKind::Equal, 0);
    check_node(&result.parts[1], NodeKind::Text, 0);
}

#[test]
fn test_param_equals_with_blank() {
    let mut parser = Parser::new();
    let mut builder = NodeBuilder::new();
    builder.ident(3);
    builder.equals();
    let param_idx = builder.param(&mut parser);
    let result = analyze_param(&parser, param_idx).unwrap().unwrap();
    check_node(&result, NodeKind::Param, 0);
    assert!(result.name.is_some());
    let name = result.name.unwrap();
    assert_eq!(name.pos, 0);
    assert_eq!(name.length, 3);
}

#[test]
fn test_param_equals_only_comment() {
    let mut parser = Parser::new();
    let mut builder = NodeBuilder::new();
    builder.ident(3);
    builder.equals();
    builder.comment(5);
    let param_idx = builder.param(&mut parser);
    let result = analyze_param(&parser, param_idx).unwrap().unwrap();
    check_node(&result, NodeKind::Param, 0);
    assert!(result.name.is_some());
    let name = result.name.unwrap();
    assert_eq!(name.pos, 0);
    assert_eq!(name.length, 3);
}

#[test]
fn test_trailing_comma_empty_param_is_ignored() {
    // analyze_param preserves empty params; trailing-empty trimming
    // at Macro level removes them from the Macro node's parts.
    let mut parser = Parser::new();
    let builder = NodeBuilder::new();
    let param_idx = builder.param(&mut parser);
    let result = analyze_param(&parser, param_idx).unwrap();
    assert!(result.is_some());
}

#[test]
fn test_param_complex_spacing() {
    let mut parser = Parser::new();
    let mut builder = NodeBuilder::new();
    builder.space(2);
    builder.comment(4);
    builder.space(1);
    builder.ident(3);
    builder.space(2);
    builder.comment(5);
    builder.equals();
    builder.space(3);
    builder.comment(4);
    builder.text(4);
    let param_idx = builder.param(&mut parser);
    let result = analyze_param(&parser, param_idx).unwrap().unwrap();
    check_node(&result, NodeKind::Param, 1);
    assert!(result.name.is_some());
    check_node(&result.parts[0], NodeKind::Text, 0);
}

#[test]
fn test_param_multiple_equals() {
    let mut parser = Parser::new();
    let mut builder = NodeBuilder::new();
    builder.ident(3);
    builder.equals();
    builder.text(2);
    builder.equals();
    builder.text(2);
    let param_idx = builder.param(&mut parser);
    let result = analyze_param(&parser, param_idx).unwrap().unwrap();
    assert!(result.name.is_some());
    assert_eq!(result.parts.len(), 3);
    check_node(&result.parts[0], NodeKind::Text, 0);
    check_node(&result.parts[1], NodeKind::Equal, 0);
    check_node(&result.parts[2], NodeKind::Text, 0);
}

#[test]
fn test_param_multiple_idents() {
    let mut parser = Parser::new();
    let mut builder = NodeBuilder::new();
    builder.ident(3);
    builder.space(1);
    builder.ident(3);
    let param_idx = builder.param(&mut parser);
    let result = analyze_param(&parser, param_idx).unwrap().unwrap();
    assert!(result.name.is_none());
    assert_eq!(result.parts.len(), 3);
    check_node(&result.parts[0], NodeKind::Ident, 0);
    check_node(&result.parts[1], NodeKind::Space, 0);
    check_node(&result.parts[2], NodeKind::Ident, 0);
}

#[test]
fn test_param_mixed_content() {
    let mut parser = Parser::new();
    let mut builder = NodeBuilder::new();
    builder.text(2);
    builder.space(1);
    builder.ident(3);
    builder.equals();
    builder.text(2);
    let param_idx = builder.param(&mut parser);
    let result = analyze_param(&parser, param_idx).unwrap().unwrap();
    assert!(result.name.is_none());
    assert_eq!(result.parts.len(), 5);
    check_node(&result.parts[0], NodeKind::Text, 0);
    check_node(&result.parts[1], NodeKind::Space, 0);
    check_node(&result.parts[2], NodeKind::Ident, 0);
    check_node(&result.parts[3], NodeKind::Equal, 0);
    check_node(&result.parts[4], NodeKind::Text, 0);
}

#[test]
fn test_param_complex_nesting() {
    let mut parser = Parser::new();
    let text1_idx     = n(&mut parser, NodeKind::Text,  1, 3, vec![]);
    let var_idx       = n(&mut parser, NodeKind::Var,   4, 5, vec![]);
    let space_idx     = n(&mut parser, NodeKind::Space, 9, 1, vec![]);
    let macro_text_idx = n(&mut parser, NodeKind::Text, 11, 3, vec![]);
    let text2_idx     = n(&mut parser, NodeKind::Text, 18, 2, vec![]);
    let macro_param_idx = n(&mut parser, NodeKind::Param, 11, 3, vec![macro_text_idx]);
    let macro_idx     = n(&mut parser, NodeKind::Macro, 10, 8, vec![macro_param_idx]);
    let block_idx     = n(&mut parser, NodeKind::Block,  0, 20,
                          vec![text1_idx, var_idx, space_idx, macro_idx, text2_idx]);
    let param_idx     = n(&mut parser, NodeKind::Param,  0, 20, vec![block_idx]);
    let result = analyze_param(&parser, param_idx).unwrap().unwrap();
    assert!(result.name.is_none());
    assert_eq!(result.parts.len(), 1);
    let block = &result.parts[0];
    assert_eq!(block.kind, NodeKind::Block);
    assert_eq!(block.parts.len(), 5);
    check_node(&block.parts[0], NodeKind::Text, 0);
    check_node(&block.parts[1], NodeKind::Var, 0);
    check_node(&block.parts[2], NodeKind::Space, 0);
    check_node(&block.parts[3], NodeKind::Macro, 1);
    check_node(&block.parts[4], NodeKind::Text, 0);
}

#[test]
fn test_param_nested_equals() {
    let mut parser = Parser::new();
    let ident_idx  = n(&mut parser, NodeKind::Ident, 0, 3, vec![]);
    let equal1_idx = n(&mut parser, NodeKind::Equal, 3, 1, vec![]);
    let text1_idx  = n(&mut parser, NodeKind::Text,  4, 3, vec![]);
    let equal2_idx = n(&mut parser, NodeKind::Equal, 7, 1, vec![]);
    let text2_idx  = n(&mut parser, NodeKind::Text,  8, 4, vec![]);
    let block_idx  = n(&mut parser, NodeKind::Block, 4, 8,
                       vec![text1_idx, equal2_idx, text2_idx]);
    let param_idx  = n(&mut parser, NodeKind::Param, 0, 12,
                       vec![ident_idx, equal1_idx, block_idx]);
    let result = analyze_param(&parser, param_idx).unwrap().unwrap();
    assert!(result.name.is_some());
    assert_eq!(result.parts.len(), 1);
    let block = &result.parts[0];
    assert_eq!(block.kind, NodeKind::Block);
    assert_eq!(block.parts.len(), 3);
    check_node(&block.parts[0], NodeKind::Text, 0);
    check_node(&block.parts[1], NodeKind::Equal, 0);
    check_node(&block.parts[2], NodeKind::Text, 0);
}

#[test]
fn test_param_with_block() {
    let mut parser = Parser::new();
    let name_idx  = n(&mut parser, NodeKind::Ident, 0, 3, vec![]);
    let equal_idx = n(&mut parser, NodeKind::Equal, 3, 1, vec![]);
    let text1_idx = n(&mut parser, NodeKind::Text,  5, 3, vec![]);
    let space_idx = n(&mut parser, NodeKind::Space, 8, 1, vec![]);
    let text2_idx = n(&mut parser, NodeKind::Text,  9, 4, vec![]);
    let block_idx = n(&mut parser, NodeKind::Block, 4, 10,
                      vec![text1_idx, space_idx, text2_idx]);
    let param_idx = n(&mut parser, NodeKind::Param, 0, 14,
                      vec![name_idx, equal_idx, block_idx]);
    let result = analyze_param(&parser, param_idx).unwrap().unwrap();
    assert!(result.name.is_some());
    assert_eq!(result.parts.len(), 1);
    let block = &result.parts[0];
    assert_eq!(block.kind, NodeKind::Block);
    assert_eq!(block.parts.len(), 3);
    check_node(&block.parts[0], NodeKind::Text, 0);
    check_node(&block.parts[1], NodeKind::Space, 0);
    check_node(&block.parts[2], NodeKind::Text, 0);
}

#[test]
fn test_param_with_var() {
    let mut parser = Parser::new();
    let text1_idx = n(&mut parser, NodeKind::Text,  0, 3, vec![]);
    let space_idx = n(&mut parser, NodeKind::Space, 3, 1, vec![]);
    let var_idx   = n(&mut parser, NodeKind::Var,   4, 5, vec![]);
    let text2_idx = n(&mut parser, NodeKind::Text,  9, 2, vec![]);
    let param_idx = n(&mut parser, NodeKind::Param, 0, 11,
                      vec![text1_idx, space_idx, var_idx, text2_idx]);
    let result = analyze_param(&parser, param_idx).unwrap().unwrap();
    assert!(result.name.is_none());
    assert_eq!(result.parts.len(), 4);
    check_node(&result.parts[0], NodeKind::Text, 0);
    check_node(&result.parts[1], NodeKind::Space, 0);
    check_node(&result.parts[2], NodeKind::Var, 0);
    check_node(&result.parts[3], NodeKind::Text, 0);
}

#[test]
fn test_param_with_nested_macro() {
    let mut parser = Parser::new();
    let name_idx       = n(&mut parser, NodeKind::Ident, 0, 3, vec![]);
    let equal_idx      = n(&mut parser, NodeKind::Equal, 3, 1, vec![]);
    let text_idx       = n(&mut parser, NodeKind::Text,  5, 3, vec![]);
    let macro_param_idx = n(&mut parser, NodeKind::Param, 5, 3, vec![text_idx]);
    let macro_idx      = n(&mut parser, NodeKind::Macro, 4, 8, vec![macro_param_idx]);
    let param_idx      = n(&mut parser, NodeKind::Param, 0, 12,
                           vec![name_idx, equal_idx, macro_idx]);
    let result = analyze_param(&parser, param_idx).unwrap().unwrap();
    assert!(result.name.is_some());
    assert_eq!(result.parts.len(), 1);
    check_node(&result.parts[0], NodeKind::Macro, 1);
}

// ── DFA edge cases ─────────────────────────────────────────────────────

#[test]
fn test_param_double_equals_value_starts_with_equal() {
    // `ident = = text`: second Equal is the first value token.
    // Distinct from test_param_multiple_equals which is `ident = text = text`.
    let mut parser = Parser::new();
    let mut builder = NodeBuilder::new();
    builder.ident(3);  // foo
    builder.equals();  // first =  → SeenEqual
    builder.equals();  // second = → first_good_after_equal, value starts here
    builder.text(3);   // bar
    let param_idx = builder.param(&mut parser);
    let result = analyze_param(&parser, param_idx).unwrap().unwrap();
    assert!(result.name.is_some(), "should be named param");
    assert_eq!(result.name.unwrap().length, 3, "name should be 'foo'");
    // Value part list starts at the second Equal, so parts = [Equal, Text].
    assert_eq!(result.parts.len(), 2);
    check_node(&result.parts[0], NodeKind::Equal, 0);
    check_node(&result.parts[1], NodeKind::Text,  0);
}

#[test]
fn test_param_var_as_first_token_is_positional() {
    // Only Ident can start the named-detection branch; Var must produce positional.
    let mut parser = Parser::new();
    let var_idx   = n(&mut parser, NodeKind::Var,   0, 5, vec![]);
    let text_idx  = n(&mut parser, NodeKind::Text,  5, 3, vec![]);
    let param_idx = n(&mut parser, NodeKind::Param, 0, 8, vec![var_idx, text_idx]);
    let result = analyze_param(&parser, param_idx).unwrap().unwrap();
    assert!(result.name.is_none(), "Var-prefixed param should be positional");
    assert_eq!(result.parts.len(), 2);
    check_node(&result.parts[0], NodeKind::Var,  0);
    check_node(&result.parts[1], NodeKind::Text, 0);
}

#[test]
fn test_param_block_as_first_token_is_positional() {
    // Block before Ident: DFA breaks immediately in Start state.
    // Even though `= text` follows, the whole param is positional.
    let mut parser = Parser::new();
    let inner_idx = n(&mut parser, NodeKind::Text,  1, 3, vec![]);
    let block_idx = n(&mut parser, NodeKind::Block, 0, 5, vec![inner_idx]);
    let equal_idx = n(&mut parser, NodeKind::Equal, 5, 1, vec![]);
    let text_idx  = n(&mut parser, NodeKind::Text,  7, 3, vec![]);
    let param_idx = n(&mut parser, NodeKind::Param, 0, 10,
                      vec![block_idx, equal_idx, text_idx]);
    let result = analyze_param(&parser, param_idx).unwrap().unwrap();
    assert!(result.name.is_none(), "Block-prefixed param should be positional");
    assert_eq!(result.parts.len(), 3);
    check_node(&result.parts[0], NodeKind::Block, 1); // block with its inner Text
    check_node(&result.parts[1], NodeKind::Equal, 0);
    check_node(&result.parts[2], NodeKind::Text,  0);
}

// ── NodeKind discriminants — regression guard ──────────────────────────

#[test]
fn test_node_kind_discriminants() {
    // NotUsed=0 is intentional: Python IntEnum starts at 1 by default,
    // so reserving 0 keeps Rust and Python discriminants aligned.
    assert_eq!(NodeKind::NotUsed as i32, 0);
    assert_eq!(NodeKind::Text as i32, 1);
    assert_eq!(NodeKind::Space as i32, 2);
    assert_eq!(NodeKind::Ident as i32, 3);
    assert_eq!(NodeKind::LineComment as i32, 4);
    assert_eq!(NodeKind::BlockComment as i32, 5);
    assert_eq!(NodeKind::Var as i32, 6);
    assert_eq!(NodeKind::Equal as i32, 7);
    assert_eq!(NodeKind::Param as i32, 8);
    assert_eq!(NodeKind::Macro as i32, 9);
    assert_eq!(NodeKind::Block as i32, 10);
}

// ── serialize_ast_nodes — BFS ordering ────────────────────────────────

#[test]
fn test_serialize_bfs_child_indices() {
    // Tree: Root[A, B], A[C, D], B[], C[], D[]
    // With the old DFS traversal B landed at index 4 instead of 2.
    // BFS guarantees: Root→[1,2], A→[3,4], B/C/D are leaves at 2/3/4.
    let tok = |pos| Token { src: 0, kind: TokenKind::Text, pos, length: 1 };
    let c = ASTNode { kind: NodeKind::Text,  src: 0, token: tok(3), end_pos: 4, parts: vec![], name: None };
    let d = ASTNode { kind: NodeKind::Text,  src: 0, token: tok(4), end_pos: 5, parts: vec![], name: None };
    let b = ASTNode { kind: NodeKind::Text,  src: 0, token: tok(2), end_pos: 3, parts: vec![], name: None };
    let a = ASTNode { kind: NodeKind::Macro, src: 0, token: tok(1), end_pos: 6, parts: vec![c, d], name: None };
    let root = ASTNode { kind: NodeKind::Block, src: 0, token: tok(0), end_pos: 7, parts: vec![a, b], name: None };
    let nodes = serialize_ast_nodes(&root);
    assert_eq!(nodes.len(), 5);
    assert!(nodes[0].contains("[1,2]"),  "root children: {}", nodes[0]);
    assert!(nodes[1].contains("[3,4]"),  "A children: {}",    nodes[1]);
    assert!(nodes[2].ends_with(",[]]"), "B should be leaf: {}", nodes[2]);
    assert!(nodes[3].ends_with(",[]]"), "C should be leaf: {}", nodes[3]);
    assert!(nodes[4].ends_with(",[]]"), "D should be leaf: {}", nodes[4]);
}

#[test]
fn test_serialize_bfs_deep_linear_chain() {
    // A → B → C → D (linear, each node has exactly one child).
    // BFS guarantees: A's child is at index 1, B's at 2, C's at 3, D is a leaf.
    let tok = |pos| Token { src: 0, kind: TokenKind::Text, pos, length: 1 };
    let d = ASTNode { kind: NodeKind::Text,  src: 0, token: tok(3), end_pos: 4, parts: vec![], name: None };
    let c = ASTNode { kind: NodeKind::Macro, src: 0, token: tok(2), end_pos: 4, parts: vec![d], name: None };
    let b = ASTNode { kind: NodeKind::Macro, src: 0, token: tok(1), end_pos: 4, parts: vec![c], name: None };
    let a = ASTNode { kind: NodeKind::Block, src: 0, token: tok(0), end_pos: 4, parts: vec![b], name: None };
    let nodes = serialize_ast_nodes(&a);
    assert_eq!(nodes.len(), 4);
    assert!(nodes[0].contains("[1]"), "A should point to child at 1: {}", nodes[0]);
    assert!(nodes[1].contains("[2]"), "B should point to child at 2: {}", nodes[1]);
    assert!(nodes[2].contains("[3]"), "C should point to child at 3: {}", nodes[2]);
    assert!(nodes[3].ends_with(",[]]"), "D should be a leaf: {}", nodes[3]);
}

#[test]
fn test_serialize_token_src_field_present() {
    // token.src must appear in the output so external evaluators can trace
    // which source file a node came from.
    let root = ASTNode {
        kind: NodeKind::Block,
        src: 0,
        token: Token { src: 2, kind: TokenKind::Text, pos: 5, length: 3 },
        end_pos: 8,
        parts: vec![],
        name: None,
    };
    let nodes = serialize_ast_nodes(&root);
    assert_eq!(nodes.len(), 1);
    assert!(nodes[0].starts_with("[10,2,"), "token src not present: {}", nodes[0]);
}

// ── strip_space_before_comments ────────────────────────────────────────

#[test]
fn test_strip_removes_space_node_before_line_comment() {
    let content = b"hello %%// comment\n";
    let mut parser = Parser::new();
    let text_idx    = n(&mut parser, NodeKind::Text,        0,  5, vec![]);
    let space_idx   = n(&mut parser, NodeKind::Space,       5,  1, vec![]);
    let comment_idx = n(&mut parser, NodeKind::LineComment, 6, 12, vec![]);
    let root_idx    = n(&mut parser, NodeKind::Block,       0, 18,
                        vec![text_idx, space_idx, comment_idx]);
    strip_space_before_comments(content, &mut parser, root_idx).unwrap();
    let root = parser.get_node(root_idx).unwrap();
    assert_eq!(root.parts.len(), 2);
    assert_eq!(root.parts[0], text_idx);
    assert_eq!(root.parts[1], comment_idx);
}

#[test]
fn test_strip_trims_trailing_spaces_in_text_before_line_comment() {
    let content = b"hello   %%// comment\n";
    let mut parser = Parser::new();
    let text_idx    = n(&mut parser, NodeKind::Text,        0,  8, vec![]);
    let comment_idx = n(&mut parser, NodeKind::LineComment, 8, 12, vec![]);
    let root_idx    = n(&mut parser, NodeKind::Block,       0, 20,
                        vec![text_idx, comment_idx]);
    strip_space_before_comments(content, &mut parser, root_idx).unwrap();
    let root = parser.get_node(root_idx).unwrap();
    assert_eq!(root.parts.len(), 2);
    let text = parser.get_node(text_idx).unwrap();
    assert_eq!(text.token.length, 5, "trailing spaces should be stripped from text");
}

#[test]
fn test_strip_removes_space_before_block_comment_followed_by_newline() {
    let content = b" %/* c %*/\nmore";
    let mut parser = Parser::new();
    let space_idx   = n(&mut parser, NodeKind::Space,        0,  1, vec![]);
    let comment_idx = n(&mut parser, NodeKind::BlockComment, 1,  9, vec![]);
    let text_idx    = n(&mut parser, NodeKind::Text,        11,  4, vec![]);
    let root_idx    = n(&mut parser, NodeKind::Block,        0, 15,
                        vec![space_idx, comment_idx, text_idx]);
    strip_space_before_comments(content, &mut parser, root_idx).unwrap();
    let root = parser.get_node(root_idx).unwrap();
    assert_eq!(root.parts.len(), 2);
    assert_eq!(root.parts[0], comment_idx);
    assert_eq!(root.parts[1], text_idx);
}

#[test]
fn test_no_strip_before_inline_block_comment() {
    let content = b" %/* c %*/ more";
    let mut parser = Parser::new();
    let space_idx   = n(&mut parser, NodeKind::Space,        0,  1, vec![]);
    let comment_idx = n(&mut parser, NodeKind::BlockComment, 1,  9, vec![]);
    let text_idx    = n(&mut parser, NodeKind::Text,        10,  5, vec![]);
    let root_idx    = n(&mut parser, NodeKind::Block,        0, 15,
                        vec![space_idx, comment_idx, text_idx]);
    strip_space_before_comments(content, &mut parser, root_idx).unwrap();
    let root = parser.get_node(root_idx).unwrap();
    assert_eq!(root.parts.len(), 3);
}

#[test]
fn test_strip_removes_multiple_spaces_before_line_comment() {
    // Text("hello") / Space / Space / LineComment — both Spaces must be removed.
    let content = b"hello  %%// comment\n";
    let mut parser = Parser::new();
    let text_idx     = n(&mut parser, NodeKind::Text,        0,  5, vec![]);
    let space1_idx   = n(&mut parser, NodeKind::Space,       5,  1, vec![]);
    let space2_idx   = n(&mut parser, NodeKind::Space,       6,  1, vec![]);
    let comment_idx  = n(&mut parser, NodeKind::LineComment, 7, 12, vec![]);
    let root_idx     = n(&mut parser, NodeKind::Block,       0, 19,
                         vec![text_idx, space1_idx, space2_idx, comment_idx]);
    strip_space_before_comments(content, &mut parser, root_idx).unwrap();
    let root = parser.get_node(root_idx).unwrap();
    // Both Space nodes must be gone; only Text + Comment remain.
    assert_eq!(root.parts.len(), 2,
        "expected 2 parts after stripping two spaces, got {}", root.parts.len());
    assert_eq!(root.parts[0], text_idx);
    assert_eq!(root.parts[1], comment_idx);
}

#[test]
fn test_strip_trims_trailing_tab_in_text_before_comment() {
    // `strip_ending_space` should strip tabs as well as ASCII spaces.
    // "hello\t" followed by a line comment — the tab must be stripped.
    let content = b"hello\t%%// c\n";
    let mut parser = Parser::new();
    // Text token covers "hello\t" (6 bytes), comment token covers "%%// c\n" (6 bytes).
    let text_idx    = n(&mut parser, NodeKind::Text,        0, 6, vec![]);
    let comment_idx = n(&mut parser, NodeKind::LineComment, 6, 6, vec![]);
    let root_idx    = n(&mut parser, NodeKind::Block,       0, 12,
                        vec![text_idx, comment_idx]);
    strip_space_before_comments(content, &mut parser, root_idx).unwrap();
    let text = parser.get_node(text_idx).unwrap();
    assert_eq!(text.token.length, 5,
        "trailing tab should be stripped — expected length 5, got {}", text.token.length);
}

#[test]
fn test_strip_removes_spaces_before_multiple_consecutive_comments() {
    // Text / Space / Comment1 / Space / Comment2:
    // both Space nodes must be removed (one before each comment).
    // Content layout: "text %%// c1\n %%// c2\n"
    //   0..4  "text"   (Text)
    //   4     " "      (Space1)
    //   5..11 "%%// c1" (LineComment1, ends at 11; next byte is \n at 11 but
    //                   we don't need newline accuracy for LineComment)
    //   12    " "      (Space2)
    //   13..19"%%// c2" (LineComment2)
    let content = b"text %%// c1\n %%// c2\n";
    let mut parser = Parser::new();
    let text_idx     = n(&mut parser, NodeKind::Text,        0,  4, vec![]);
    let space1_idx   = n(&mut parser, NodeKind::Space,       4,  1, vec![]);
    let comment1_idx = n(&mut parser, NodeKind::LineComment, 5,  7, vec![]);
    let space2_idx   = n(&mut parser, NodeKind::Space,      12,  1, vec![]);
    let comment2_idx = n(&mut parser, NodeKind::LineComment,13,  7, vec![]);
    let root_idx = n(&mut parser, NodeKind::Block, 0, 20,
                     vec![text_idx, space1_idx, comment1_idx, space2_idx, comment2_idx]);
    strip_space_before_comments(content, &mut parser, root_idx).unwrap();
    let root = parser.get_node(root_idx).unwrap();
    assert_eq!(root.parts.len(), 3,
        "expected [text, comment1, comment2], got {} parts", root.parts.len());
    assert_eq!(root.parts[0], text_idx);
    assert_eq!(root.parts[1], comment1_idx);
    assert_eq!(root.parts[2], comment2_idx);
}

// ── Full pipeline ──────────────────────────────────────────────────────

#[test]
fn test_pipeline_plain_text() {
    use crate::evaluator::lex_parse_content;
    let ast = lex_parse_content("hello world", '%', 0).unwrap();
    assert_eq!(ast.kind, NodeKind::Block);
    assert_eq!(ast.parts.len(), 1);
    assert_eq!(ast.parts[0].kind, NodeKind::Text);
}

#[test]
fn test_pipeline_comments_stripped_from_ast() {
    use crate::evaluator::lex_parse_content;
    let ast = lex_parse_content("before %// comment\nafter", '%', 0).unwrap();
    fn no_comments(node: &ASTNode) {
        assert_ne!(node.kind, NodeKind::LineComment, "LineComment leaked into AST");
        assert_ne!(node.kind, NodeKind::BlockComment, "BlockComment leaked into AST");
        for child in &node.parts { no_comments(child); }
    }
    no_comments(&ast);
    assert!(ast.parts.iter().any(|n| n.kind == NodeKind::Text));
}

#[test]
fn test_pipeline_var_node() {
    use crate::evaluator::lex_parse_content;
    let ast = lex_parse_content("%(x)", '%', 0).unwrap();
    assert!(ast.parts.iter().any(|n| n.kind == NodeKind::Var));
}

#[test]
fn test_pipeline_macro_with_named_param() {
    use crate::evaluator::lex_parse_content;
    let ast = lex_parse_content("%foo(a, b=val)", '%', 0).unwrap();
    let mac = ast.parts.iter().find(|n| n.kind == NodeKind::Macro)
        .expect("expected Macro node");
    assert_eq!(mac.parts.len(), 2);
    let unnamed = mac.parts.iter().find(|p| p.name.is_none()).expect("unnamed param");
    let named   = mac.parts.iter().find(|p| p.name.is_some()).expect("named param");
    assert_eq!(named.name.unwrap().length, 1);
    assert!(unnamed.parts.iter().any(|n| n.kind == NodeKind::Ident || n.kind == NodeKind::Text));
}

#[test]
fn test_pipeline_tagged_block() {
    use crate::evaluator::lex_parse_content;
    let ast = lex_parse_content("%foo{ content %foo}", '%', 0).unwrap();
    let block = ast.parts.iter().find(|n| n.kind == NodeKind::Block)
        .expect("expected Block node");
    assert!(!block.parts.is_empty());
}

#[test]
fn test_strip_is_idempotent() {
    use crate::Lexer;
    use crate::parser::Parser;
    let src = "hello %%// comment\nworld";
    let (tokens, _) = Lexer::new(src, '%', 0).lex();
    let li = crate::line_index::LineIndex::new(src);

    let mut parser = Parser::new();
    parser.parse(&tokens, src.as_bytes(), &li).unwrap();
    let root = 0;

    // First strip
    strip_space_before_comments(src.as_bytes(), &mut parser, root).unwrap();
    let ast1 = crate::ast::build_ast(&parser).unwrap();

    // Second strip on already-stripped parser
    strip_space_before_comments(src.as_bytes(), &mut parser, root).unwrap();
    let ast2 = crate::ast::build_ast(&parser).unwrap();

    // Both ASTs should have the same number of top-level parts
    assert_eq!(ast1.parts.len(), ast2.parts.len(), "strip is not idempotent");
}

#[test]
fn test_ast_no_comments_invariant() {
    use crate::evaluator::lex_parse_content;
    fn check_no_comments(node: &ASTNode) {
        assert!(
            !matches!(node.kind, NodeKind::LineComment | NodeKind::BlockComment),
            "comment node {:?} leaked into AST",
            node.kind
        );
        for child in &node.parts { check_no_comments(child); }
    }
    for src in &[
        "plain text",
        "before %%// line comment\nafter",
        "before %%/* block %%*/ mid after",
        "%%def(foo, body) %%foo()",
    ] {
        let ast = lex_parse_content(src, '%', 0).unwrap();
        check_no_comments(&ast);
    }
}
