// src/parser/lib.rs

use crate::types::{ASTNode, NodeKind, ParseNode, Token, TokenKind};
use std::fs::File;
use std::io::{self, BufRead, BufReader};
use thiserror::Error;

#[cfg(test)]
mod tests;

/// The parser-specific error type (like your old ParserError)
#[derive(Error, Debug)]
pub enum ParserError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Invalid token data: {0}")]
    TokenData(String),

    #[error("Parse error: {0}")]
    Parse(String),
}

impl From<String> for ParserError {
    fn from(s: String) -> Self {
        ParserError::TokenData(s)
    }
}

impl Token {
    pub fn to_json(&self) -> String {
        format!(
            "[{},{},{},{}]",
            self.src, self.kind as i32, self.pos, self.length
        )
    }
}

impl ParseNode {
    pub fn to_json(&self) -> String {
        format!(
            "[{},{},{},{},{}]",
            self.kind as i32,
            self.src,
            self.token.to_json(),
            self.end_pos,
            if self.parts.is_empty() {
                "[]".to_string()
            } else {
                format!(
                    "[{}]",
                    self.parts
                        .iter()
                        .map(|i| i.to_string())
                        .collect::<Vec<_>>()
                        .join(",")
                )
            }
        )
    }
}

#[derive(Debug, Clone, Copy)]
enum ParserState {
    Block,
    Macro,
    Comment,
}

/// The Parser struct
pub struct Parser {
    nodes: Vec<ParseNode>,
    stack: Vec<(ParserState, usize)>,
}

impl Default for Parser {
    fn default() -> Self {
        Self::new()
    }
}

impl Parser {
    /// Create a new parser
    pub fn new() -> Self {
        Parser {
            nodes: Vec::new(),
            stack: Vec::new(),
        }
    }

    fn create_node(&mut self, kind: NodeKind, src: u32, token: Token) -> usize {
        let node = ParseNode {
            kind,
            src,
            token,
            end_pos: 0,
            parts: Vec::new(),
        };
        self.nodes.push(node);
        self.nodes.len() - 1
    }

    fn add_child(&mut self, parent_idx: usize, child_idx: usize) {
        if let Some(parent) = self.nodes.get_mut(parent_idx) {
            parent.parts.push(child_idx);
        }
    }

    fn create_add_node(&mut self, kind: NodeKind, src: u32, token: Token) -> usize {
        let new_idx = self.create_node(kind, src, token);
        // Attach it to the node on top of the stack
        if let Some(&(_, parent_idx)) = self.stack.last() {
            self.add_child(parent_idx, new_idx);
        }
        new_idx
    }

    fn close_node(&mut self, token: Token) {
        if let Some(&(_, node_idx)) = self.stack.last()
            && let Some(node) = self.nodes.get_mut(node_idx)
        {
            node.end_pos = token.pos + token.length;
        }
    }

    /// Main parse function
    pub fn parse(&mut self, tokens: &[Token]) -> Result<(), ParserError> {
        if tokens.is_empty() {
            return Ok(());
        }

        // Start with a "Block" node at the root
        let dummy = Token {
            kind: TokenKind::Text,
            src: tokens[0].src,
            pos: 0,
            length: 0,
        };
        let root_idx = self.create_node(NodeKind::Block, tokens[0].src, dummy);
        self.stack.push((ParserState::Block, root_idx));

        for token in tokens {
            let token = *token;

            // Which state are we in?
            match self.stack.last().map(|&(st, _)| st) {
                Some(ParserState::Block) => {
                    if token.kind == TokenKind::BlockClose {
                        // Close current block
                        self.close_node(token);
                        self.stack.pop();
                        continue;
                    }
                }
                Some(ParserState::Macro) => {
                    match token.kind {
                        TokenKind::Comma => {
                            // Param boundary
                            self.close_node(token);
                            self.stack.pop();
                            let new_idx = self.create_add_node(NodeKind::Param, token.src, token);
                            self.stack.push((ParserState::Macro, new_idx));
                            continue;
                        }
                        TokenKind::CloseParen => {
                            // close Param, close Macro
                            self.close_node(token);
                            self.stack.pop();
                            self.close_node(token);
                            self.stack.pop();
                            continue;
                        }
                        TokenKind::Ident => {
                            self.create_add_node(NodeKind::Ident, token.src, token);
                            continue;
                        }
                        TokenKind::Space => {
                            self.create_add_node(NodeKind::Space, token.src, token);
                            continue;
                        }
                        TokenKind::Equal => {
                            self.create_add_node(NodeKind::Equal, token.src, token);
                            continue;
                        }
                        _ => {}
                    }
                }
                Some(ParserState::Comment) => {
                    match token.kind {
                        TokenKind::CommentClose => {
                            self.close_node(token);
                            self.stack.pop();
                            continue;
                        }
                        TokenKind::CommentOpen => {
                            // nested comment
                            let new_idx =
                                self.create_add_node(NodeKind::BlockComment, token.src, token);
                            self.stack.push((ParserState::Comment, new_idx));
                            continue;
                        }
                        _ => {
                            // inside a comment, everything is "ignored" except open/close
                            continue;
                        }
                    }
                }
                None => break,
            }

            // Otherwise, handle "new" tokens
            match token.kind {
                TokenKind::BlockOpen => {
                    let new_idx = self.create_add_node(NodeKind::Block, token.src, token);
                    self.stack.push((ParserState::Block, new_idx));
                }
                TokenKind::Macro => {
                    // Start a Macro node + a Param node
                    let macro_idx = self.create_add_node(NodeKind::Macro, token.src, token);
                    self.stack.push((ParserState::Macro, macro_idx));
                    let param_token = Token {
                        kind: TokenKind::Text,
                        src: token.src,
                        pos: token.pos,
                        length: 0, // just a placeholder
                    };
                    let param_idx = self.create_add_node(NodeKind::Param, token.src, param_token);
                    self.stack.push((ParserState::Macro, param_idx));
                }
                TokenKind::CommentOpen => {
                    let new_idx = self.create_add_node(NodeKind::BlockComment, token.src, token);
                    self.stack.push((ParserState::Comment, new_idx));
                }
                TokenKind::Var => {
                    self.create_add_node(NodeKind::Var, token.src, token);
                }
                TokenKind::LineComment => {
                    self.create_add_node(NodeKind::LineComment, token.src, token);
                }
                _ => {
                    // default => treat as text
                    self.create_add_node(NodeKind::Text, token.src, token);
                }
            }
        }

        // close + pop the root if it’s still open
        if let Some((_, _root_idx)) = self.stack.pop()
            && let Some(last_token) = tokens.last()
        {
            self.close_node(*last_token);
        }

        Ok(())
    }

    // -----------------------------------------------------------------------
    // If you want the read_tokens(...) logic from your old code:
    // -----------------------------------------------------------------------
    fn parse_token_from_parts(parts: Vec<&str>) -> Result<Token, ParserError> {
        if parts.len() != 4 {
            return Err(ParserError::TokenData(format!(
                "Invalid token data: {}",
                parts.join(",")
            )));
        }
        Ok(Token {
            src: parts[0]
                .parse()
                .map_err(|e| ParserError::TokenData(format!("Invalid src: {}", e)))?,
            kind: parts[1]
                .parse::<i32>()
                .map_err(|e| ParserError::TokenData(format!("Invalid kind: {}", e)))?
                .try_into()?,
            pos: parts[2]
                .parse()
                .map_err(|e| ParserError::TokenData(format!("Invalid pos: {}", e)))?,
            length: parts[3]
                .parse()
                .map_err(|e| ParserError::TokenData(format!("Invalid length: {}", e)))?,
        })
    }

    fn parse_tokens<I>(lines: I) -> Result<Vec<Token>, ParserError>
    where
        I: Iterator<Item = Result<String, std::io::Error>>,
    {
        let mut tokens = Vec::new();
        for line in lines {
            let line =
                line.map_err(|e| ParserError::TokenData(format!("Failed to read line: {}", e)))?;
            let parts: Vec<&str> = line.split(',').collect();
            tokens.push(Self::parse_token_from_parts(parts)?);
        }
        Ok(tokens)
    }

    pub fn read_tokens(path: &str) -> Result<Vec<Token>, ParserError> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        Self::parse_tokens(reader.lines())
    }

    pub fn read_tokens_from_stdin() -> Result<Vec<Token>, ParserError> {
        let stdin = io::stdin();
        Self::parse_tokens(stdin.lock().lines())
    }

    /// Get a reference to a node by index
    pub fn get_node(&self, idx: usize) -> Option<&ParseNode> {
        self.nodes.get(idx)
    }

    /// Get a mutable reference to a node by index
    pub fn get_node_mut(&mut self, idx: usize) -> Option<&mut ParseNode> {
        self.nodes.get_mut(idx)
    }
    pub fn get_node_info(&self, idx: usize) -> Option<(&ParseNode, NodeKind)> {
        self.nodes.get(idx).map(|node| (node, node.kind))
    }

    /// Conditional JSON serialization
    #[cfg(any(debug_assertions, test))]
    pub fn to_json(&self) -> String {
        self.nodes
            .iter()
            .map(|node| node.to_json())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Release mode provides empty string to avoid allocation
    #[cfg(not(any(debug_assertions, test)))]
    pub fn to_json(&self) -> String {
        String::new()
    }

    /// Get the root node's index (usually 0 if parse succeeded)
    pub fn get_root_index(&self) -> Option<usize> {
        if self.nodes.is_empty() {
            None
        } else {
            Some(0) // The root is always the first node
        }
    }

    /// Process AST including space stripping
    pub fn process_ast(&mut self, content: &[u8]) -> Result<ASTNode, String> {
        // First phase - mutable for space stripping
        let root_idx = self
            .get_root_index()
            .ok_or_else(|| "Empty parse tree".to_string())?;

        crate::ast::strip_space_before_comments(content, self, root_idx)
            .map_err(|e| e.to_string())?;

        // Second phase - build AST
        crate::ast::build_ast(self).map_err(|e| e.to_string())
    }

    /// Direct build without space stripping
    pub fn build_ast(&self) -> Result<ASTNode, String> {
        crate::ast::build_ast(self).map_err(|e| e.to_string())
    }

    /// Strip ending spaces from a node's token
    pub fn strip_ending_space(&mut self, content: &[u8], node_idx: usize) -> Result<(), String> {
        let node = self
            .get_node_mut(node_idx)
            .ok_or_else(|| format!("Node index {} not found", node_idx))?;

        let start = node.token.pos;
        let end = node.token.pos + node.token.length;
        if start >= content.len() {
            return Ok(());
        }

        let mut space_count = 0;
        for c in content[start..end.min(content.len())].iter().rev() {
            if *c == b'\n' || *c == b'\r' || *c == b' ' || *c == b'\t' {
                space_count += 1;
            } else {
                break;
            }
        }

        if space_count > 0 {
            node.token.length = node.token.length.saturating_sub(space_count);
            node.end_pos = node.token.pos + node.token.length;
        }

        Ok(())
    }

    #[cfg(test)]
    pub fn add_node(&mut self, node: ParseNode) -> usize {
        self.nodes.push(node);
        self.nodes.len() - 1
    }
}
