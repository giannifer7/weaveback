use asciidoc_parser::Parser;
use std::env;
use std::fs;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <file.adoc>", args[0]);
        std::process::exit(1);
    }

    let file_path = &args[1];
    let source = match fs::read_to_string(file_path) {
        Ok(content) => content,
        Err(e) => {
            eprintln!("Failed to read file {}: {}", file_path, e);
            std::process::exit(1);
        }
    };

    let mut parser = Parser::default();
    
    // Parse the UTF-8 source text
    let document = parser.parse(&source);
    
    println!("{:#?}", document);
}
