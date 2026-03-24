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

    let input_content = "%def(greeting, Hello)\n%greeting(), World!";
    let input_file = create_temp_file(input_content);

    let output_file = temp_dir.path().join("output.txt");

    eval_file_with_config(input_file.path(), &output_file, EvalConfig::default()).unwrap();

    let result = std::fs::read_to_string(output_file).unwrap();
    assert_eq!(result, "\nHello, World!");
}

#[test]
fn test_eval_file_overwrite_protection() {
    let input_file = create_temp_file("content");
    let result = eval_file_with_config(
        input_file.path(),
        input_file.path(),
        EvalConfig::default(),
    );
    assert!(matches!(result, Err(EvalError::Runtime(_))));
}

#[test]
fn test_eval_files() {
    let temp_dir = TempDir::new().unwrap();
    let output_dir = temp_dir.path().join("output");

    let file1 = create_temp_file("%def(shared, Shared content)");
    let file2 = create_temp_file("%shared()");

    eval_files_with_config(
        &[file1.path().to_path_buf(), file2.path().to_path_buf()],
        &output_dir,
        EvalConfig::default(),
    )
    .unwrap();

    let files: Vec<_> = std::fs::read_dir(&output_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    assert_eq!(files.len(), 2);
}
