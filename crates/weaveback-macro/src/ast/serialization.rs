// crates/weaveback-macro/src/ast/serialization.rs

use crate::evaluator::{lex_parse_content, EvalError};
use crate::types::{ASTNode, Token};
use std::collections::VecDeque;
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::PathBuf;

fn serialize_token(token: &Token) -> String {
    format!("{},{},{},{}", token.src, token.kind as i32, token.pos, token.length)
}

pub fn serialize_ast_nodes(root: &ASTNode) -> Vec<String> {
    let mut nodes = Vec::new();
    // BFS so that child indices assigned as next_idx..next_idx+n are contiguous
    // and land exactly where each node ends up in the output array.
    let mut queue: VecDeque<&ASTNode> = VecDeque::new();
    let mut next_idx = 1usize; // root is index 0

    // We don't need to write src because we process one file at a time and the caller knows which
    queue.push_back(root);
    while let Some(node) = queue.pop_front() {
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
        nodes.push(format!(
            "[{},{},{},{}]",
            node.kind as i32,
            serialize_token(&node.token),
            node.end_pos,
            parts,
        ));

        for child in &node.parts {
            queue.push_back(child);
        }
    }

    nodes
}

pub fn write_ast<W: Write>(header: &str, nodes: &[String], writer: &mut W) -> io::Result<()> {
    writeln!(writer, "{}", header)?;
    for line in nodes {
        writeln!(writer, "{}", line)?;
    }
    Ok(())
}

pub fn write_ast_to_file(header: &str, nodes: &[String], output_path: &PathBuf) -> io::Result<()> {
    if output_path.to_str() == Some("-") {
        let stdout = io::stdout();
        let mut handle = stdout.lock();
        write_ast(header, nodes, &mut handle)
    } else {
        let mut file = File::create(output_path)?;
        write_ast(header, nodes, &mut file)
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

        let (output, src_name) = if input.to_str() == Some("-") {
            (PathBuf::from("-"), "-".to_string())
        } else {
            (input.with_extension("ast"), input.display().to_string())
        };

        // Header line: maps src indices to source file paths.
        // Format: # src:<index>=<path>  (one per source file; currently always src:0)
        let header = format!("# src:0={}", src_name);

        write_ast_to_file(&header, &nodes, &output).map_err(|e| {
            EvalError::Runtime(format!("Failed to write {}: {}", output.display(), e))
        })?;
    }
    Ok(())
}
