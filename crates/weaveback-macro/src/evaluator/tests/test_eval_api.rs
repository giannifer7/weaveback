// crates/weaveback-macro/src/evaluator/tests/test_eval_api.rs

use std::io::Write;
use std::path::{Path, PathBuf};
use tempfile::{NamedTempFile, TempDir};

use crate::evaluator::{
    errors::{EvalError, EvalResult},
    eval_api::{eval_file_with_config, eval_files_with_config, eval_string_with_defaults},
    state::EvalConfig,
};

fn create_temp_file(content: &str) -> NamedTempFile {
    let mut file = NamedTempFile::new().unwrap();
    write!(file, "{}", content).unwrap();
    file
}

#[test]
fn test_eval_string_basic() {
    let result = eval_string_with_defaults("%def(hello, World)\nHello %hello()!").unwrap();
    assert_eq!(result, "\nHello World!");
}

#[test]
fn test_eval_file() {
    let temp_dir = TempDir::new().unwrap();

    // Create input file
    let input_content = "%def(greeting, Hello)\n%greeting(), World!";
    let input_file = create_temp_file(input_content);

    // Set up output file
    let output_file = temp_dir.path().join("output.txt");

    // Process
    eval_file_with_config(input_file.path(), &output_file, EvalConfig::default()).unwrap();

    // Verify
    let result = std::fs::read_to_string(output_file).unwrap();
    assert_eq!(result, "\nHello, World!");
}

#[test]
fn test_eval_multiple_files() {
    let temp_dir = TempDir::new().unwrap();

    // Create input files
    let file1 = create_temp_file("%def(msg, File1)\n%msg()");
    let file2 = create_temp_file("%def(msg, File2)\n%msg()");

    let output_dir = temp_dir.path().join("output");

    // Process both files
    eval_files_with_config(
        &[file1.path().to_path_buf(), file2.path().to_path_buf()],
        &output_dir,
        EvalConfig::default(),
    )
    .unwrap();

    // Verify outputs
    let result1 = std::fs::read_to_string(
        output_dir
            .join(file1.path().file_name().unwrap())
            .with_extension("txt"),
    )
    .unwrap();
    let result2 = std::fs::read_to_string(
        output_dir
            .join(file2.path().file_name().unwrap())
            .with_extension("txt"),
    )
    .unwrap();

    assert_eq!(result1.trim(), "File1");
    assert_eq!(result2.trim(), "File2");
}

#[test]
fn test_error_handling() {
    // Test undefined macro
    let result = eval_string_with_defaults("%undefined()");
    assert!(matches!(result, Err(EvalError::UndefinedMacro(_))));

    // Test missing input file
    let result = eval_file_with_config(
        Path::new("nonexistent.txt"),
        Path::new("out.txt"),
        EvalConfig::default(),
    );
    assert!(matches!(result, Err(EvalError::Runtime(_))));
}

#[test]
fn test_nested_macros() {
    let source = r#"
        %def(inner, text, Inner: %(text))
        %def(outer, arg, %inner(%(arg)))
        %outer(test)
    "#;

    let result = eval_string_with_defaults(source).unwrap();
    assert!(result.contains("Inner: test"));
}

#[test]
fn test_include_handling() {
    let temp_dir = TempDir::new().unwrap();

    // Create an included file
    let include_file = create_temp_file("%def(included, content)\nIncluded %included()");
    let include_path = include_file.path().to_path_buf();

    // Create main file that includes it
    let main_content = format!("%include({})", include_path.display());
    let main_file = create_temp_file(&main_content);

    // Set up output
    let output_file = temp_dir.path().join("output.txt");

    // Configure with correct include path
    let mut config = EvalConfig::default();
    config.include_paths.push(temp_dir.path().to_path_buf());

    // Process
    eval_file_with_config(main_file.path(), &output_file, config).unwrap();

    // Verify
    let result = std::fs::read_to_string(output_file).unwrap();
    assert!(result.contains("Included content"));
}
