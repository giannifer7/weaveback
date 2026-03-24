// crates/weaveback-macro/src/ast/mod.rs — generated from ast.adoc
use crate::parser::Parser;
use crate::types::{ASTNode, NodeKind, Token};
use thiserror::Error;
pub mod serialization;

pub use serialization::{dump_macro_ast, serialize_ast_nodes};

/// Three-state DFA used by `analyze_param` to classify a parameter node.
#[derive(Debug)]
enum ParamState {
    /// Initial state — no significant token seen yet.
    Start,
    /// Saw `Ident`; waiting for `=` or a non-skip token that ends the scan.
    SeenName { name: Token, name_idx: usize },
    /// Saw `Ident =`; waiting for the first non-skip value token.
    SeenEqual { name: Token },
}

#[cfg(test)]
mod tests;

/// Returns `true` for node kinds that are transparent in both parameter
/// scanning (`analyze_param`) and whitespace stripping
/// (`strip_space_before_comments`): `Space`, `LineComment`, `BlockComment`.
#[inline]
fn is_skippable(kind: NodeKind) -> bool {
    matches!(kind, NodeKind::Space | NodeKind::LineComment | NodeKind::BlockComment)
}

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

/// Analyse a parameter node: classify as positional or named and collect parts.
fn analyze_param(parser: &Parser, node_idx: usize) -> Result<Option<ASTNode>, ASTError> {
    let node = parser
        .get_node(node_idx)
        .ok_or(ASTError::NodeNotFound(node_idx))?;

    let mut state = ParamState::Start;
    let mut first_not_skippable: Option<usize> = None;
    let mut first_good_after_equal: Option<usize> = None;

    // First pass: walk children through a three-state DFA.
    'scan: for (i, &part_idx) in node.parts.iter().enumerate() {
        let part = parser
            .get_node(part_idx)
            .ok_or(ASTError::NodeNotFound(part_idx))?;

        if is_skippable(part.kind) {
            continue;
        }

        first_not_skippable.get_or_insert(i);

        match &state {
            ParamState::Start => {
                if part.kind == NodeKind::Ident {
                    state = ParamState::SeenName { name: part.token, name_idx: i };
                    // keep scanning — an `=` may follow
                } else {
                    break 'scan; // positional: first non-skip is not an Ident
                }
            }
            ParamState::SeenName { name, .. } => {
                if part.kind == NodeKind::Equal {
                    let name = *name; // Token is Copy
                    state = ParamState::SeenEqual { name };
                    // keep scanning — a value item may follow
                } else {
                    break 'scan; // positional: Ident not followed by =
                }
            }
            ParamState::SeenEqual { .. } => {
                first_good_after_equal = Some(i);
                break 'scan; // named param; value starts here
            }
        }
    }

    // Determine start index and param name from the final DFA state.
    let (start_idx, param_name) = match state {
        ParamState::Start => match first_not_skippable {
            None => {
                // Completely empty param.
                return Ok(Some(ASTNode {
                    kind: NodeKind::Param,
                    src: node.src,
                    token: node.token,
                    end_pos: node.end_pos,
                    parts: vec![],
                    name: None,
                }));
            }
            Some(i) => (i, None), // positional: starts from first non-skip
        },
        ParamState::SeenName { name_idx, .. } => (name_idx, None),
        ParamState::SeenEqual { name } => match first_good_after_equal {
            None => {
                // Named param with blank value: `foo =`.
                return Ok(Some(ASTNode {
                    kind: NodeKind::Param,
                    src: node.src,
                    token: node.token,
                    end_pos: node.end_pos,
                    parts: vec![],
                    name: Some(name),
                }));
            }
            Some(i) => (i, Some(name)),
        },
    };

    // Second pass: collect and clean the value parts.
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
        name: param_name,
    }))
}

/// Recursively convert a `ParseNode` arena entry to an owned `ASTNode` tree.
///
/// Returns `None` for comment nodes (stripped from the AST entirely) and
/// delegates `Param` nodes to `analyze_param`.
fn clean_node(parser: &Parser, node_idx: usize) -> Result<Option<ASTNode>, ASTError> {
    let node = parser
        .get_node(node_idx)
        .ok_or(ASTError::NodeNotFound(node_idx))?;

    // Strip comments entirely.
    if matches!(node.kind, NodeKind::LineComment | NodeKind::BlockComment) {
        return Ok(None);
    }

    // Parameter nodes require name/value analysis.
    if node.kind == NodeKind::Param {
        return analyze_param(parser, node_idx);
    }

    // Structural invariant: leaf node kinds should never have children.
    // A violation here indicates a parser bug, not user input.
    debug_assert!(
        !matches!(
            node.kind,
            NodeKind::Equal | NodeKind::Ident | NodeKind::Text | NodeKind::Space
        ) || node.parts.is_empty(),
        "leaf {:?} node at index {} should have no children, found {}",
        node.kind,
        node_idx,
        node.parts.len()
    );

    // Recurse into children.
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

    // Analysis phase: walk forward; when we hit a comment, walk back over
    // all consecutive Space nodes preceding it.
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

                if is_line_comment || block_comment_newline {
                    // Walk back over ALL consecutive Space nodes.
                    let mut j = i;
                    while j > 0 {
                        let prev_idx = node.parts[j - 1];
                        let prev = parser
                            .get_node(prev_idx)
                            .ok_or(ASTError::NodeNotFound(prev_idx))?;
                        if prev.kind == NodeKind::Space {
                            to_remove.push(j - 1);
                            j -= 1;
                        } else {
                            // Not a Space — trim trailing spaces from a
                            // preceding Text node, then stop.
                            if prev.kind == NodeKind::Text {
                                spaces_to_strip.push(prev_idx);
                            }
                            break;
                        }
                    }
                }
            }
            i += 1;
        }
    }

    // Modification phase
    if !to_remove.is_empty() {
        // De-duplicate (a single Space may be adjacent to two comments).
        to_remove.sort_unstable();
        to_remove.dedup();
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
