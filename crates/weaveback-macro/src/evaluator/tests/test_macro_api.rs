// crates/weaveback-macro/src/evaluator/tests/test_macro_api.rs

use crate::evaluator::{EvalConfig, Evaluator};
use crate::macro_api::{
    process_file, process_file_with_writer, process_files, process_files_from_config,
    process_string, process_string_defaults, process_string_precise, process_string_tracing,
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
fn test_process_string_with_sigil_chars() {
    let config = EvalConfig {
        sigil: '@',
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
fn test_process_string_with_unicode_sigil() {
    let config = EvalConfig {
        sigil: '§',
        ..EvalConfig::default()
    };
    let mut evaluator = Evaluator::new(config);

    let result = process_string(
        "§def(test, value, Result: §(value))§test(works)",
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

#[test]
fn test_process_string_tracing_returns_output_and_entries() {
    let mut evaluator = Evaluator::new(EvalConfig::default());
    let (bytes, entries) = process_string_tracing("hello %(x)", None, &mut evaluator).unwrap();
    assert_eq!(String::from_utf8(bytes).unwrap(), "hello ");
    assert!(!entries.is_empty(), "expected tracing entries for literal output");
}

#[test]
fn test_process_string_precise_returns_spans() {
    let mut evaluator = Evaluator::new(EvalConfig::default());
    let (output, spans) = process_string_precise("abc", None, &mut evaluator).unwrap();
    assert_eq!(output, "abc");
    assert!(!spans.is_empty(), "expected at least one precise span");
}

#[test]
fn test_process_file_with_writer_adds_input_context_on_eval_error() {
    let temp_dir = TempDir::new().unwrap();
    let input = create_temp_file(&temp_dir, "bad.txt", "%undefined_macro()");
    let mut sink = Vec::new();
    let mut evaluator = Evaluator::new(EvalConfig::default());

    let err = process_file_with_writer(&input, &mut sink, &mut evaluator).unwrap_err();
    let message = err.to_string();
    assert!(message.contains("bad.txt"), "missing input file context: {message}");
}

struct FailingWriter;

impl Write for FailingWriter {
    fn write(&mut self, _buf: &[u8]) -> std::io::Result<usize> {
        Err(std::io::Error::other("nope"))
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn test_process_file_with_writer_reports_write_failure() {
    let temp_dir = TempDir::new().unwrap();
    let input = create_temp_file(&temp_dir, "ok.txt", "hello");
    let mut writer = FailingWriter;
    let mut evaluator = Evaluator::new(EvalConfig::default());

    let err = process_file_with_writer(&input, &mut writer, &mut evaluator).unwrap_err();
    assert!(err.to_string().contains("Cannot write to output"));
}

#[test]
fn test_process_file_creates_parent_directories() {
    let temp_dir = TempDir::new().unwrap();
    let input = create_temp_file(&temp_dir, "in.txt", "hello");
    let output = temp_dir.path().join("nested/out.txt");
    let mut evaluator = Evaluator::new(EvalConfig::default());

    process_file(&input, &output, &mut evaluator).unwrap();
    assert_eq!(fs::read_to_string(output).unwrap(), "hello");
}

#[test]
fn test_process_file_reports_missing_input() {
    let temp_dir = TempDir::new().unwrap();
    let input = temp_dir.path().join("missing.txt");
    let output = temp_dir.path().join("out.txt");
    let mut evaluator = Evaluator::new(EvalConfig::default());

    let err = process_file(&input, &output, &mut evaluator).unwrap_err();
    assert!(err.to_string().contains("Cannot read"));
}

#[test]
fn test_process_files_creates_parent_directories_for_output_file() {
    let temp_dir = TempDir::new().unwrap();
    let input = create_temp_file(&temp_dir, "in.txt", "hello");
    let output = temp_dir.path().join("nested/out.txt");
    let mut evaluator = Evaluator::new(EvalConfig::default());

    process_files(&[input], &output, &mut evaluator).unwrap();
    assert_eq!(fs::read_to_string(output).unwrap(), "hello");
}

#[test]
fn test_process_files_stdout_path_succeeds() {
    let temp_dir = TempDir::new().unwrap();
    let input = create_temp_file(&temp_dir, "in.txt", "hello");
    let mut evaluator = Evaluator::new(EvalConfig::default());

    process_files(&[input], std::path::Path::new("-"), &mut evaluator).unwrap();
}

#[test]
fn test_process_file_direct_api_reuses_evaluator_state() {
    let temp_dir = TempDir::new().unwrap();
    let input1 = create_temp_file(&temp_dir, "defs.txt", "%def(shared, hello)");
    let input2 = create_temp_file(&temp_dir, "use.txt", "%shared()");
    let output1 = temp_dir.path().join("out1.txt");
    let output2 = temp_dir.path().join("out2.txt");
    let mut evaluator = Evaluator::new(EvalConfig::default());

    process_file(&input1, &output1, &mut evaluator).unwrap();
    process_file(&input2, &output2, &mut evaluator).unwrap();

    assert_eq!(fs::read_to_string(output2).unwrap(), "hello");
}

#[test]
fn test_process_files_reports_missing_input() {
    let temp_dir = TempDir::new().unwrap();
    let input = temp_dir.path().join("missing.txt");
    let output = temp_dir.path().join("out.txt");
    let mut evaluator = Evaluator::new(EvalConfig::default());

    let err = process_files(&[input], &output, &mut evaluator).unwrap_err();
    assert!(err.to_string().contains("Cannot read"));
}
