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
    // Create a temporary directory for the test
    let temp_dir = TempDir::new().expect("Failed to create temporary directory");
    let temp_dir_path = temp_dir.path();

    // Create a test file `test.txt` in the temporary directory
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

    // Create an Evaluator with the temporary directory as the include path
    let mut evaluator = create_evaluator_with_temp_dir(temp_dir_path);

    // Process the file with the %here macro
    let result = process_string(
        &fs::read_to_string(&test_file_path).unwrap(),
        Some(&test_file_path),
        &mut evaluator,
    );

    // Verify that the file was modified correctly
    let modified_content = fs::read_to_string(&test_file_path).unwrap();
    assert_eq!(
        modified_content.trim(),
        "%def(insert_content, greeting, %{\n            Inserted content, %(greeting)!\n        %})\n        Before %%here(insert_content, Hello)\n            Inserted content, Hello!\n                After"
    );

    // Verify that processing succeeded (early exit is a clean stop, not an error)
    assert!(result.is_ok());
}
