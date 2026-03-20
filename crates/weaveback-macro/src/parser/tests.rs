// src/parser/tests.rs

use crate::parser::Parser;
use crate::types::{Token, TokenKind};

#[test]
fn test_basic_parsing() {
    let tokens = vec![
        Token {
            src: 0,
            kind: TokenKind::Text,
            pos: 0,
            length: 5,
        },
        Token {
            src: 0,
            kind: TokenKind::BlockOpen,
            pos: 5,
            length: 2,
        },
        Token {
            src: 0,
            kind: TokenKind::Text,
            pos: 7,
            length: 3,
        },
        Token {
            src: 0,
            kind: TokenKind::BlockClose,
            pos: 10,
            length: 2,
        },
    ];

    let mut parser = Parser::new();
    assert!(parser.parse(&tokens).is_ok());

    let json = parser.to_json();
    assert!(!json.is_empty());
}

#[test]
fn test_empty_input() {
    let tokens = vec![];
    let mut parser = Parser::new();
    assert!(parser.parse(&tokens).is_ok());
}

#[test]
fn test_macro_parsing() {
    let tokens = vec![
        Token {
            src: 0,
            kind: TokenKind::Macro,
            pos: 0,
            length: 2,
        },
        Token {
            src: 0,
            kind: TokenKind::Ident,
            pos: 2,
            length: 4,
        },
        Token {
            src: 0,
            kind: TokenKind::Space,
            pos: 6,
            length: 1,
        },
        Token {
            src: 0,
            kind: TokenKind::Equal,
            pos: 7,
            length: 1,
        },
        Token {
            src: 0,
            kind: TokenKind::CloseParen,
            pos: 8,
            length: 1,
        },
    ];

    let mut parser = Parser::new();
    assert!(parser.parse(&tokens).is_ok());

    let json = parser.to_json();
    assert!(!json.is_empty());
}

#[test]
fn test_token_kind_conversion() {
    // If you want to test the i32 => TokenKind conversion
    use std::convert::TryFrom;
    assert!(TokenKind::try_from(0).is_ok());
    assert!(TokenKind::try_from(14).is_ok());
    assert!(TokenKind::try_from(-1).is_err());
    assert!(TokenKind::try_from(15).is_err());
}
