// crates/weaveback-macro/src/ast/mod.rs — generated from ast.adoc
use crate::parser::Parser;
use crate::types::{ASTNode, NodeKind, Token};
use thiserror::Error;
pub mod serialization;

pub use serialization::{dump_macro_ast, serialize_ast_nodes};

const NOT_FOUND: i32 = -1;

#[cfg(test)]
mod tests;

#[derive(Error, Debug)]
pub enum ASTError {
    #[error("Parser error: {0}")]
    Parser(String),
    #[error("Node not found: {0}")]
    NodeNotFound(usize),
    #[error("Processing error: {0}")]
    Other(String),
}

impl From<String> for ASTError {
    fn from(error: String) -> Self {
        ASTError::Other(error)
    }
}

/// Main entry point that unwraps the Option
pub fn build_ast(parser: &Parser) -> Result<ASTNode, ASTError> {
    let root_idx = parser
        .get_root_index()
        .ok_or_else(|| ASTError::Parser("Empty parse tree".into()))?;

    clean_node(parser, root_idx)?
        .ok_or_else(|| ASTError::Parser("Root node was skipped".into()))
}

/// Analyze a parameter node to find name, equals, and determine the parts
fn analyze_param(parser: &Parser, node_idx: usize) -> Result<Option<ASTNode>, ASTError> {
    let node = parser
        .get_node(node_idx)
        .ok_or(ASTError::NodeNotFound(node_idx))?;

    let mut param_name: Option<Token> = None;
    let mut first_not_skippable = NOT_FOUND;
    let mut name_index = NOT_FOUND;
    let mut first_good_after_equal = NOT_FOUND;
    let mut seen_equal = false;

    // First pass: analyze structure
    for (i, &part_idx) in node.parts.iter().enumerate() {
        let part = parser
            .get_node(part_idx)
            .ok_or(ASTError::NodeNotFound(part_idx))?;

        if matches!(
            part.kind,
            NodeKind::Space | NodeKind::LineComment | NodeKind::BlockComment
        ) {
            continue;
        }

        if first_not_skippable == NOT_FOUND {
            first_not_skippable = i as i32;
        }

        if param_name.is_none() && !seen_equal && part.kind == NodeKind::Ident {
            param_name = Some(part.token);
            name_index = i as i32;
            continue;
        }

        if param_name.is_some() && !seen_equal && part.kind == NodeKind::Equal {
            seen_equal = true;
            continue;
        }

        if seen_equal {
            first_good_after_equal = i as i32;
        }
        break;
    }

    // Determine which parts to process
    let start_idx = if seen_equal && first_good_after_equal != NOT_FOUND {
        first_good_after_equal as usize
    } else if seen_equal && first_good_after_equal == NOT_FOUND {
        // name = <blank>
        return Ok(Some(ASTNode {
            kind: NodeKind::Param,
            src: node.src,
            token: node.token,
            end_pos: node.end_pos,
            parts: vec![],
            name: param_name,
        }));
    } else if first_not_skippable == NOT_FOUND {
        // completely empty
        return Ok(Some(ASTNode {
            kind: NodeKind::Param,
            src: node.src,
            token: node.token,
            end_pos: node.end_pos,
            parts: vec![],
            name: None,
        }));
    } else if param_name.is_some() {
        name_index as usize
    } else {
        first_not_skippable as usize
    };

    // Process the parts
    let mut value_parts = Vec::new();
    for &part_idx in &node.parts[start_idx..] {
        if let Some(part_node) = clean_node(parser, part_idx)? {
            value_parts.push(part_node);
        }
    }

    Ok(Some(ASTNode {
        kind: NodeKind::Param,
        src: node.src,
        token: node.token,
        end_pos: node.end_pos,
        parts: value_parts,
        name: if seen_equal { param_name } else { None },
    }))
}

/// Create clean AST node, skipping comments
fn clean_node(parser: &Parser, node_idx: usize) -> Result<Option<ASTNode>, ASTError> {
    let node = parser
        .get_node(node_idx)
        .ok_or(ASTError::NodeNotFound(node_idx))?;

    // Skip comments
    if matches!(node.kind, NodeKind::LineComment | NodeKind::BlockComment) {
        return Ok(None);
    }

    // Special handling for parameters
    if node.kind == NodeKind::Param {
        return analyze_param(parser, node_idx);
    }

    // Process children recursively
    let mut child_nodes = Vec::new();
    for &child_idx in &node.parts {
        if let Some(child) = clean_node(parser, child_idx)? {
            child_nodes.push(child);
        }
    }

    Ok(Some(ASTNode {
        kind: node.kind,
        src: node.src,
        token: node.token,
        end_pos: node.end_pos,
        parts: child_nodes,
        name: None,
    }))
}

pub fn strip_space_before_comments(
    content: &[u8],
    parser: &mut Parser,
    node_idx: usize,
) -> Result<(), ASTError> {
    let mut to_remove: Vec<usize> = Vec::new();
    let mut spaces_to_strip: Vec<usize> = Vec::new();

    // Analysis phase
    {
        let node = parser
            .get_node(node_idx)
            .ok_or(ASTError::NodeNotFound(node_idx))?;

        let mut i = 0;
        while i < node.parts.len() {
            let part_idx = node.parts[i];
            let part = parser
                .get_node(part_idx)
                .ok_or(ASTError::NodeNotFound(part_idx))?;

            let is_line_comment = part.kind == NodeKind::LineComment;
            let is_block_comment = part.kind == NodeKind::BlockComment;

            if is_line_comment || is_block_comment {
                let block_comment_newline = if is_block_comment {
                    is_followed_by_newline(content, parser, part_idx)?
                } else {
                    false
                };

                if (is_line_comment || block_comment_newline) && i > 0 {
                    let prev_idx = node.parts[i - 1];
                    let prev = parser
                        .get_node(prev_idx)
                        .ok_or(ASTError::NodeNotFound(prev_idx))?;

                    match prev.kind {
                        NodeKind::Space => to_remove.push(i - 1),
                        NodeKind::Text => spaces_to_strip.push(prev_idx),
                        _ => {}
                    }
                }
            }
            i += 1;
        }
    }

    // Modification phase
    if !to_remove.is_empty() {
        let node = parser
            .get_node_mut(node_idx)
            .ok_or(ASTError::NodeNotFound(node_idx))?;
        for &idx in to_remove.iter().rev() {
            node.parts.remove(idx);
        }
    }

    for idx in spaces_to_strip {
        parser.strip_ending_space(content, idx)?;
    }

    // Recurse into children (re-read after modification to skip removed nodes)
    let children: Vec<usize> = parser
        .get_node(node_idx)
        .ok_or(ASTError::NodeNotFound(node_idx))?
        .parts
        .clone();
    for child_idx in children {
        strip_space_before_comments(content, parser, child_idx)?;
    }

    Ok(())
}

fn is_followed_by_newline(
    content: &[u8],
    parser: &Parser,
    node_idx: usize,
) -> Result<bool, ASTError> {
    let node = parser
        .get_node(node_idx)
        .ok_or(ASTError::NodeNotFound(node_idx))?;
    let end_pos = node.end_pos;

    Ok(end_pos < content.len() && content[end_pos] == b'\n')
}
