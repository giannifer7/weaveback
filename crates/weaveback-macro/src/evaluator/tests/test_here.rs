// weaveback-macro/src/evaluator/tests/test_here.rs
// I'd Really Rather You Didn't edit this generated file.

// crates/weaveback-macro/src/evaluator/tests/test_here.rs

use crate::evaluator::Evaluator;
use crate::evaluator::tests::test_utils::evaluator_in_temp_dir;
use crate::macro_api::process_string;
use std::fs;
use tempfile::TempDir;

fn create_evaluator_with_temp_dir(temp_dir: &std::path::Path) -> Evaluator {
    evaluator_in_temp_dir(temp_dir)
}

#[test]
fn test_here_with_macros() {
    let temp_dir = TempDir::new().expect("Failed to create temporary directory");
    let temp_dir_path = temp_dir.path();

    let test_file_path = temp_dir_path.join("test.txt");
    fs::write(
        &test_file_path,
        r#"
        %def(insert_content, greeting, %{
            Inserted content, %(greeting)!
        %})
        Before %here(insert_content, Hello)
        After
        "#,
    )
    .expect("Failed to write test file");

    let mut evaluator = create_evaluator_with_temp_dir(temp_dir_path);

    let result = process_string(
        &fs::read_to_string(&test_file_path).unwrap(),
        Some(&test_file_path),
        &mut evaluator,
    );

    let modified_content = fs::read_to_string(&test_file_path).unwrap();
    assert_eq!(
        modified_content.trim(),
        "%def(insert_content, greeting, %{\n            Inserted content, %(greeting)!\n        %})\n        Before %%here(insert_content, Hello)\n            Inserted content, Hello!\n                After"
    );

    assert!(result.is_ok());
}

#[test]
fn test_here_idempotency_already_patched_file() {
    // Running on a file that already has %%here (the neutralised form) must
    // be a no-op: the file must not be modified a second time.
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("patched.txt");

    // Write a file that already has %%here (neutralised) — simulating the state
    // after a previous successful run.
    let already_patched =
        "%def(msg, hello world)\n%%here(msg)\nhello world\nrest of file";
    fs::write(&test_file, already_patched).unwrap();

    let content = fs::read_to_string(&test_file).unwrap();
    let mut ev = create_evaluator_with_temp_dir(temp_dir.path());
    let result = process_string(&content, Some(&test_file), &mut ev);

    // Must succeed (%%here is literal text, not a macro call).
    assert!(result.is_ok(), "already-patched file should expand without error: {:?}", result);

    // File must be unchanged — no second patch.
    let after = fs::read_to_string(&test_file).unwrap();
    assert_eq!(after, already_patched, "idempotency violated: file was modified again");
}

#[test]
fn test_here_multiple_live_calls_is_error_and_noop() {
    // Multiple live %here calls in one file are rejected before any rewrite.
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("two_here.txt");

    let content =
        "%def(a, first)\n%def(b, second)\n%here(a)\n%here(b)";
    fs::write(&test_file, content).unwrap();

    let src = fs::read_to_string(&test_file).unwrap();
    let mut ev = create_evaluator_with_temp_dir(temp_dir.path());
    let err = process_string(&src, Some(&test_file), &mut ev).unwrap_err().to_string();

    let modified = fs::read_to_string(&test_file).unwrap();
    assert!(err.contains("multiple live %here calls"));
    assert_eq!(modified, content, "file should remain unchanged on error");
}

