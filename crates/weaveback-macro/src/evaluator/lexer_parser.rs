// weaveback/crates/weaveback-macro/src/evaluator/lexer_parser.rs

use crate::lexer::Lexer;
use crate::parser::Parser;
use crate::types::ASTNode;

pub fn lex_parse_content(source: &str, special_char: char, src: u32) -> Result<ASTNode, String> {
    let (tokens, lex_errors) = Lexer::new(source, special_char, src).lex();
    if !lex_errors.is_empty() {
        let errs = lex_errors
            .iter()
            .map(|e| format!("{:?}", e))
            .collect::<Vec<_>>()
            .join("; ");
        return Err(format!("Lexer errors: {}", errs));
    }

    let mut parser = Parser::new();
    parser
        .parse(&tokens)
        .map_err(|e| format!("Parse error: {:?}", e))?;

    let ast = parser
        .process_ast(source.as_bytes())
        .map_err(|e| format!("AST build error: {:?}", e))?;

    Ok(ast)
}
