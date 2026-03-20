// crates/weaveback-macro/src/types.rs

use serde::{Deserialize, Serialize};
use std::convert::TryFrom;

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
    EOF = 14,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum NodeKind {
    NotUsed = 0,
    Text = 1,
    Space = 2,
    Ident = 3,
    LineComment = 4,
    BlockComment = 5,
    Var = 6,
    Equal = 7,
    Punct = 8,
    Composite = 9,
    Param = 10,
    Macro = 11,
    Block = 12,
}

#[derive(Debug, Clone, Copy)]
pub struct Token {
    pub kind: TokenKind,
    pub src: u32,
    pub pos: usize,
    pub length: usize,
}

#[derive(Debug)]
pub struct LexerError {
    pub row: usize,
    pub col: usize,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct ParseNode {
    pub kind: NodeKind,
    pub src: u32,
    pub token: Token,
    pub end_pos: usize,
    pub parts: Vec<usize>,
}

#[derive(Debug, Clone)]
pub struct ASTNode {
    pub kind: NodeKind,
    pub src: u32,
    pub token: Token,
    pub end_pos: usize,
    pub parts: Vec<ASTNode>,
    pub name: Option<Token>,
}

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
            14 => Ok(TokenKind::EOF),
            _ => Err(format!("Invalid token kind: {}", value)),
        }
    }
}

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
            8 => Ok(NodeKind::Punct),
            9 => Ok(NodeKind::Composite),
            10 => Ok(NodeKind::Param),
            11 => Ok(NodeKind::Macro),
            12 => Ok(NodeKind::Block),
            _ => Err(format!("Invalid NodeKind: {value}")),
        }
    }
}
