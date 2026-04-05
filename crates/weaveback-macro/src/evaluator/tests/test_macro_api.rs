// crates/weaveback-macro/src/evaluator/tests/test_macro_api.rs

use crate::evaluator::{EvalConfig, Evaluator};
use crate::macro_api::{
    process_file, process_files_from_config, process_string, process_string_defaults,
};
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use tempfile::TempDir;

fn create_temp_file(dir: &TempDir, name: &str, content: &str) -> PathBuf {
    let path = dir.path().join(name);
    let mut file = fs::File::create(&path).unwrap();
    write!(file, "{}", content).unwrap();
    path
}

#[test]
fn test_process_string_basic() {
    let result = process_string_defaults("Hello %def(test, World) %test()").unwrap();
    assert_eq!(String::from_utf8(result).unwrap(), "Hello  World");
}

#[test]
fn test_include_basic() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = TempDir::new()?;

    let _include_file = create_temp_file(&temp_dir, "include.txt", "test");

    let main_file = create_temp_file(&temp_dir, "main.txt", "%include(include.txt)");

    let config = EvalConfig {
        include_paths: vec![temp_dir.path().to_path_buf()],
        ..EvalConfig::default()
    };
    let mut evaluator = Evaluator::new(config);

    let output_file = temp_dir.path().join("output.txt");

    process_file(&main_file, &output_file, &mut evaluator)?;

    let result = fs::read_to_string(output_file)?;
    assert_eq!(result.trim(), "test");

    Ok(())
}

#[test]
fn test_process_string_with_error() {
    let result = process_string_defaults("%undefined_macro()");
    assert!(result.is_err());
}

#[test]
fn test_process_string_with_nested_macros() {
    let source = r#"
        %def(inner, value, Inside: %(value))
        %def(outer, arg, Outside: %inner(%(arg)))
        %outer(test)
    "#;

    let result = process_string_defaults(source).unwrap();
    let output = String::from_utf8(result).unwrap();
    assert!(output.contains("Outside: Inside: test"));
}

#[test]
fn test_process_string_with_special_chars() {
    let config = EvalConfig {
        special_char: '@',
        ..EvalConfig::default()
    };
    let mut evaluator = Evaluator::new(config);

    let result = process_string(
        "@def(test, value, Result: @(value))@test(works)",
        None,
        &mut evaluator,
    )
    .unwrap();

    assert_eq!(String::from_utf8(result).unwrap().trim(), "Result: works");
}

#[test]
fn test_process_files_with_shared_macros() {
    let temp_dir = TempDir::new().unwrap();
    let file1 = create_temp_file(&temp_dir, "file1.txt", "%def(shared, Shared content)");
    let file2 = create_temp_file(&temp_dir, "file2.txt", "%shared()");

    let output_file = temp_dir.path().join("output.txt");

    let config = EvalConfig::default();
    process_files_from_config(&[file1, file2], &output_file, config).unwrap();

    let output = fs::read_to_string(&output_file).unwrap();
    assert_eq!(output.trim(), "Shared content");
}
