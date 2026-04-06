// crates/weaveback-macro/src/evaluator/tests/test_eval_api.rs

use std::io::Write;
use std::path::Path;
use tempfile::{NamedTempFile, TempDir};

use crate::evaluator::{
    errors::EvalError,
    eval_api::{
        eval_file, eval_file_with_config, eval_files, eval_files_with_config, eval_string,
        eval_string_with_defaults,
    },
    state::EvalConfig,
    Evaluator,
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

#[test]
fn test_eval_string_with_real_path_sets_current_file() {
    let mut evaluator = Evaluator::new(EvalConfig::default());
    let real = Path::new("/tmp/example.txt");
    let result = eval_string("plain text", Some(real), &mut evaluator).unwrap();
    assert_eq!(result, "plain text");
}

#[test]
fn test_eval_file_creates_parent_directories() {
    let temp_dir = TempDir::new().unwrap();
    let input_file = create_temp_file("hello");
    let output_file = temp_dir.path().join("nested/dir/output.txt");

    eval_file_with_config(input_file.path(), &output_file, EvalConfig::default()).unwrap();

    let result = std::fs::read_to_string(&output_file).unwrap();
    assert_eq!(result, "hello");
}

#[test]
fn test_eval_file_reports_missing_input() {
    let temp_dir = TempDir::new().unwrap();
    let input = temp_dir.path().join("missing.txt");
    let output = temp_dir.path().join("out.txt");

    let err = eval_file_with_config(&input, &output, EvalConfig::default()).unwrap_err();
    let message = err.to_string();
    assert!(message.contains("Cannot resolve input path") || message.contains("Cannot read"));
}

#[test]
fn test_eval_files_stops_on_missing_input() {
    let temp_dir = TempDir::new().unwrap();
    let output_dir = temp_dir.path().join("out");
    let good = create_temp_file("good");
    let missing = temp_dir.path().join("missing.txt");

    let err = eval_files_with_config(
        &[good.path().to_path_buf(), missing],
        &output_dir,
        EvalConfig::default(),
    )
    .unwrap_err();
    assert!(matches!(err, EvalError::Runtime(_)));
}

#[test]
fn test_eval_file_direct_api_reuses_evaluator_state() {
    let temp_dir = TempDir::new().unwrap();
    let input1 = create_temp_file("%def(shared, hello)");
    let input2 = create_temp_file("%shared()");
    let output1 = temp_dir.path().join("out1.txt");
    let output2 = temp_dir.path().join("out2.txt");
    let mut evaluator = Evaluator::new(EvalConfig::default());

    eval_file(input1.path(), &output1, &mut evaluator).unwrap();
    eval_file(input2.path(), &output2, &mut evaluator).unwrap();

    assert_eq!(std::fs::read_to_string(output2).unwrap(), "hello");
}

#[test]
fn test_eval_files_direct_api_uses_output_file_name() {
    let temp_dir = TempDir::new().unwrap();
    let output_dir = temp_dir.path().join("out");
    let input1 = create_temp_file("first");
    let input2 = create_temp_file("second");
    let name1 = input1.path().file_name().unwrap().to_owned();
    let name2 = input2.path().file_name().unwrap().to_owned();
    let mut evaluator = Evaluator::new(EvalConfig::default());

    eval_files(
        &[input1.path().to_path_buf(), input2.path().to_path_buf()],
        &output_dir,
        &mut evaluator,
    )
    .unwrap();

    assert_eq!(std::fs::read_to_string(output_dir.join(name1)).unwrap(), "first");
    assert_eq!(std::fs::read_to_string(output_dir.join(name2)).unwrap(), "second");
}

#[test]
fn test_eval_string_with_defaults_helper() {
    let result = eval_string_with_defaults("%def(hi, there)%hi()").unwrap();
    assert_eq!(result, "there");
}
