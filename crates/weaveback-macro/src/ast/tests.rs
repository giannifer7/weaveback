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
    /*
    fn block(&self, parser: &mut Parser) -> usize {
        let parts = self.build_nodes(parser);
        n(parser, NodeKind::Block, 0, self.pos, parts)
    }*/
}

/// Helper to verify AST node structure
fn check_node(node: &ASTNode, expected_kind: NodeKind, expected_parts: usize) {
    assert_eq!(node.kind, expected_kind);
    assert_eq!(node.parts.len(), expected_parts);
}

// Now let's write a test using the fixed builder
#[test]
fn test_param_identifier_only() {
    let mut parser = Parser::new();
    let mut builder = NodeBuilder::new();

    // Build sequence of nodes
    builder.space(1); // Leading space
    builder.ident(3); // Identifier
    builder.space(1); // Trailing space

    // Create parameter with all nodes
    let param_idx = builder.param(&mut parser);

    let result = analyze_param(&parser, param_idx).unwrap().unwrap();
    check_node(&result, NodeKind::Param, 2); // Ident and trailing space
    check_node(&result.parts[0], NodeKind::Ident, 0);
    check_node(&result.parts[1], NodeKind::Space, 0);
}

#[test]
fn test_empty_param() {
    let mut parser = Parser::new();
    let builder = NodeBuilder::new();

    let param_idx = builder.param(&mut parser);

    let result = analyze_param(&parser, param_idx).unwrap().unwrap();
    check_node(&result, NodeKind::Param, 0);
    assert!(result.name.is_none());
}

#[test]
fn test_param_with_comments() {
    let mut parser = Parser::new();
    let mut builder = NodeBuilder::new();

    builder.comment(1); // Line comment
    builder.ident(3); // Identifier
    builder.comment(1); // Block comment

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

    builder.space(1); // Leading space
    builder.text(3); // Value
    builder.space(1); // Trailing space

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

    builder.ident(3); // Name
    builder.equals(); // =
    builder.text(4); // Value

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

    builder.equals(); // =
    builder.text(4); // Value

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

    builder.ident(3); // Name
    builder.equals(); // =

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

    builder.ident(3); // Name
    builder.equals(); // =
    builder.comment(5); // Comment

    let param_idx = builder.param(&mut parser);

    let result = analyze_param(&parser, param_idx).unwrap().unwrap();
    check_node(&result, NodeKind::Param, 0);
    assert!(result.name.is_some());
    let name = result.name.unwrap();
    assert_eq!(name.pos, 0);
    assert_eq!(name.length, 3);
}

#[test]
fn test_param_complex_spacing() {
    let mut parser = Parser::new();
    let mut builder = NodeBuilder::new();

    builder.space(2); // Leading spaces
    builder.comment(4); // Comment
    builder.space(1); // More space
    builder.ident(3); // Name
    builder.space(2); // Space
    builder.comment(5); // Block comment
    builder.equals(); // =
    builder.space(3); // Space
    builder.comment(4); // Line comment
    builder.text(4); // Value

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

    builder.ident(3); // First ident
    builder.equals(); // First equals
    builder.text(2); // Text
    builder.equals(); // Second equals
    builder.text(2); // More text

    let param_idx = builder.param(&mut parser);

    let result = analyze_param(&parser, param_idx).unwrap().unwrap();
    assert!(result.name.is_some());
    assert_eq!(result.parts.len(), 3); // Everything after first equals preserved
    check_node(&result.parts[0], NodeKind::Text, 0);
    check_node(&result.parts[1], NodeKind::Equal, 0);
    check_node(&result.parts[2], NodeKind::Text, 0);
}

#[test]
fn test_param_multiple_idents() {
    let mut parser = Parser::new();
    let mut builder = NodeBuilder::new();

    builder.ident(3); // First ident
    builder.space(1); // Space
    builder.ident(3); // Second ident

    let param_idx = builder.param(&mut parser);

    let result = analyze_param(&parser, param_idx).unwrap().unwrap();
    assert!(result.name.is_none()); // No name since no equals
    assert_eq!(result.parts.len(), 3); // All parts preserved
    check_node(&result.parts[0], NodeKind::Ident, 0);
    check_node(&result.parts[1], NodeKind::Space, 0);
    check_node(&result.parts[2], NodeKind::Ident, 0);
}

#[test]
fn test_param_mixed_content() {
    let mut parser = Parser::new();
    let mut builder = NodeBuilder::new();

    builder.text(2); // Text first
    builder.space(1); // Space
    builder.ident(3); // Then ident
    builder.equals(); // Equals
    builder.text(2); // More text

    let param_idx = builder.param(&mut parser);

    let result = analyze_param(&parser, param_idx).unwrap().unwrap();
    assert!(result.name.is_none()); // No name because text comes before ident
    assert_eq!(result.parts.len(), 5); // Everything preserved as content
    check_node(&result.parts[0], NodeKind::Text, 0);
    check_node(&result.parts[1], NodeKind::Space, 0);
    check_node(&result.parts[2], NodeKind::Ident, 0);
    check_node(&result.parts[3], NodeKind::Equal, 0);
    check_node(&result.parts[4], NodeKind::Text, 0);
}

#[test]
fn test_param_complex_nesting() {
    let mut parser = Parser::new();

    // Create all the leaf nodes first
    let text1_idx = n(&mut parser, NodeKind::Text, 1, 3, vec![]);
    let var_idx = n(&mut parser, NodeKind::Var, 4, 5, vec![]);
    let space_idx = n(&mut parser, NodeKind::Space, 9, 1, vec![]);
    let macro_text_idx = n(&mut parser, NodeKind::Text, 11, 3, vec![]);
    let text2_idx = n(&mut parser, NodeKind::Text, 18, 2, vec![]);

    // Build up structure from leaves
    let macro_param_idx = n(&mut parser, NodeKind::Param, 11, 3, vec![macro_text_idx]);
    let macro_idx = n(&mut parser, NodeKind::Macro, 10, 8, vec![macro_param_idx]);

    // Combine into block
    let block_idx = n(
        &mut parser,
        NodeKind::Block,
        0,
        20,
        vec![text1_idx, var_idx, space_idx, macro_idx, text2_idx],
    );

    // Finally into param
    let param_idx = n(&mut parser, NodeKind::Param, 0, 20, vec![block_idx]);

    let result = analyze_param(&parser, param_idx).unwrap().unwrap();
    assert!(result.name.is_none());
    assert_eq!(result.parts.len(), 1); // Just the block

    let block = &result.parts[0];
    assert_eq!(block.kind, NodeKind::Block);
    assert_eq!(block.parts.len(), 5); // All block contents preserved
    check_node(&block.parts[0], NodeKind::Text, 0);
    check_node(&block.parts[1], NodeKind::Var, 0);
    check_node(&block.parts[2], NodeKind::Space, 0);
    check_node(&block.parts[3], NodeKind::Macro, 1); // Macro has 1 param
    check_node(&block.parts[4], NodeKind::Text, 0);
}

#[test]
fn test_param_nested_equals() {
    let mut parser = Parser::new();

    // Create all leaf nodes first
    let ident_idx = n(&mut parser, NodeKind::Ident, 0, 3, vec![]);
    let equal1_idx = n(&mut parser, NodeKind::Equal, 3, 1, vec![]);
    let text1_idx = n(&mut parser, NodeKind::Text, 4, 3, vec![]);
    let equal2_idx = n(&mut parser, NodeKind::Equal, 7, 1, vec![]);
    let text2_idx = n(&mut parser, NodeKind::Text, 8, 4, vec![]);

    // Create block containing second equals
    let block_idx = n(
        &mut parser,
        NodeKind::Block,
        4,
        8,
        vec![text1_idx, equal2_idx, text2_idx],
    );

    // Combine into param
    let param_idx = n(
        &mut parser,
        NodeKind::Param,
        0,
        12,
        vec![ident_idx, equal1_idx, block_idx],
    );

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

    // Create param nodes
    let name_idx = n(&mut parser, NodeKind::Ident, 0, 3, vec![]);
    let equal_idx = n(&mut parser, NodeKind::Equal, 3, 1, vec![]);

    // Create block contents
    let text1_idx = n(&mut parser, NodeKind::Text, 5, 3, vec![]);
    let space_idx = n(&mut parser, NodeKind::Space, 8, 1, vec![]);
    let text2_idx = n(&mut parser, NodeKind::Text, 9, 4, vec![]);

    // Build block
    let block_idx = n(
        &mut parser,
        NodeKind::Block,
        4,
        10,
        vec![text1_idx, space_idx, text2_idx],
    );

    // Combine into param
    let param_idx = n(
        &mut parser,
        NodeKind::Param,
        0,
        14,
        vec![name_idx, equal_idx, block_idx],
    );

    let result = analyze_param(&parser, param_idx).unwrap().unwrap();
    assert!(result.name.is_some());
    assert_eq!(result.parts.len(), 1); // One part - the block

    // Check block and its parts
    let block = &result.parts[0];
    assert_eq!(block.kind, NodeKind::Block);
    assert_eq!(block.parts.len(), 3); // Block has 3 children
    check_node(&block.parts[0], NodeKind::Text, 0);
    check_node(&block.parts[1], NodeKind::Space, 0);
    check_node(&block.parts[2], NodeKind::Text, 0);
}

#[test]
fn test_param_with_var() {
    let mut parser = Parser::new();

    // Create all nodes first
    let text1_idx = n(&mut parser, NodeKind::Text, 0, 3, vec![]);
    let space_idx = n(&mut parser, NodeKind::Space, 3, 1, vec![]);
    let var_idx = n(&mut parser, NodeKind::Var, 4, 5, vec![]); // %(var)
    let text2_idx = n(&mut parser, NodeKind::Text, 9, 2, vec![]);

    // Then combine into param
    let param_idx = n(
        &mut parser,
        NodeKind::Param,
        0,
        11,
        vec![text1_idx, space_idx, var_idx, text2_idx],
    );

    let result = analyze_param(&parser, param_idx).unwrap().unwrap();
    assert!(result.name.is_none());
    assert_eq!(result.parts.len(), 4); // All parts preserved
    check_node(&result.parts[0], NodeKind::Text, 0);
    check_node(&result.parts[1], NodeKind::Space, 0);
    check_node(&result.parts[2], NodeKind::Var, 0);
    check_node(&result.parts[3], NodeKind::Text, 0);
}

#[test]
fn test_param_with_nested_macro() {
    let mut parser = Parser::new();

    // Create name parts
    let name_idx = n(&mut parser, NodeKind::Ident, 0, 3, vec![]);
    let equal_idx = n(&mut parser, NodeKind::Equal, 3, 1, vec![]);

    // Create macro parts
    let text_idx = n(&mut parser, NodeKind::Text, 5, 3, vec![]);
    let macro_param_idx = n(&mut parser, NodeKind::Param, 5, 3, vec![text_idx]);
    let macro_idx = n(&mut parser, NodeKind::Macro, 4, 8, vec![macro_param_idx]);

    // Combine into param
    let param_idx = n(
        &mut parser,
        NodeKind::Param,
        0,
        12,
        vec![name_idx, equal_idx, macro_idx],
    );

    let result = analyze_param(&parser, param_idx).unwrap().unwrap();
    assert!(result.name.is_some());
    assert_eq!(result.parts.len(), 1); // Just the macro after name=
    check_node(&result.parts[0], NodeKind::Macro, 1); // Macro has 1 parameter
}
