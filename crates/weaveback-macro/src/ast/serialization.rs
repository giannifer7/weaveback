// crates/weaveback-macro/src/ast/serialization.rs

use crate::evaluator::EvalError;
use crate::evaluator::lex_parse_content;
/*
use crate::evaluator::lexer_parser::lex_parse_content;
use crate::evaluator::EvalError;
use crate::evaluator::{lex_parse_content, EvalError};
*/
use crate::types::{ASTNode, Token};
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::PathBuf;

fn serialize_token(token: &Token) -> String {
    format!("{},{},{}", token.kind as i32, token.pos, token.length)
}

pub fn serialize_ast_nodes(root: &ASTNode) -> Vec<String> {
    let mut nodes = Vec::new();
    let mut queue = vec![(root, 0)];
    let mut next_idx = 1; // Start at 1 because root is index 0

    // We don't need to write src because we process one file at a time and the caller knows which
    while let Some((node, _parent_idx)) = queue.pop() {
        let node_info = format!(
            "{},{},{}",
            node.kind as i32,
            serialize_token(&node.token),
            node.end_pos
        );

        // Calculate child indices starting from next_idx
        let child_indices: Vec<usize> = (next_idx..next_idx + node.parts.len()).collect();
        next_idx += node.parts.len();

        let parts = if child_indices.is_empty() {
            "[]".to_string()
        } else {
            format!(
                "[{}]",
                child_indices
                    .iter()
                    .map(|i| i.to_string())
                    .collect::<Vec<_>>()
                    .join(",")
            )
        };
        nodes.push(format!("[{node_info},{parts}]"));

        // Queue children in reverse order to maintain order
        for child in node.parts.iter().rev() {
            queue.push((child, nodes.len() - 1));
        }
    }

    nodes
}

pub fn write_ast<W: Write>(nodes: &[String], writer: &mut W) -> io::Result<()> {
    for line in nodes {
        writeln!(writer, "{}", line)?;
    }
    Ok(())
}

pub fn write_ast_to_file(nodes: &[String], output_path: &PathBuf) -> io::Result<()> {
    if output_path.to_str() == Some("-") {
        let stdout = io::stdout();
        let mut handle = stdout.lock();
        write_ast(nodes, &mut handle)
    } else {
        let mut file = File::create(output_path)?;
        write_ast(nodes, &mut file)
    }
}

fn read_input(input: &PathBuf) -> io::Result<String> {
    if input.to_str() == Some("-") {
        let mut buffer = String::new();
        io::stdin().read_to_string(&mut buffer)?;
        Ok(buffer)
    } else {
        std::fs::read_to_string(input)
    }
}

pub fn dump_macro_ast(special: char, input_files: &[PathBuf]) -> Result<(), EvalError> {
    for input in input_files {
        let content = read_input(input).map_err(|e| {
            EvalError::Runtime(format!("Failed to read {}: {}", input.display(), e))
        })?;

        let ast = lex_parse_content(&content, special, 0)?;
        let nodes = serialize_ast_nodes(&ast);

        let output = if input.to_str() == Some("-") {
            PathBuf::from("-")
        } else {
            input.with_extension("ast")
        };

        write_ast_to_file(&nodes, &output).map_err(|e| {
            EvalError::Runtime(format!("Failed to write {}: {}", output.display(), e))
        })?;
    }
    Ok(())
}
