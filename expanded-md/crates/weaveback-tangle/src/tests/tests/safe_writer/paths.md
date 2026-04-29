# Path Validation





```rust
// <[@file weaveback-tangle/src/tests/safe_writer/paths.rs]>=
// weaveback-tangle/src/tests/safe_writer/paths.rs
// I'd Really Rather You Didn't edit this generated file.

use super::*;

#[test]
fn test_validate_filename_relative_path() -> Result<(), WeavebackError> {
    let (_temp, mut writer) = create_test_writer();
    let test_file = PathBuf::from("simple.txt");
    write_file(&mut writer, &test_file, "Allowed")?;
    let final_path = writer.get_gen_base().join(&test_file);
    let content = fs::read_to_string(&final_path)?;
    assert_eq!(content, "Allowed");
    Ok(())
}


#[test]
fn test_path_safety() {
    let (_temp, mut writer) = create_test_writer();

    let test_cases = [
        (
            PathBuf::from("../outside.txt"),
            "Path traversal detected (..)",
        ),
        (
            PathBuf::from("/absolute/path.txt"),
            "Absolute paths are not allowed",
        ),
        (
            PathBuf::from("C:/windows/path.txt"),
            "Windows-style absolute paths are not allowed",
        ),
        (
            PathBuf::from("C:test.txt"),
            "Windows-style absolute paths are not allowed",
        ),
    ];

    for (path, expected_msg) in test_cases {
        let result = write_file(&mut writer, &path, "Should fail");
        match result {
            Err(WeavebackError::SafeWriter(SafeWriterError::SecurityViolation(msg))) => {
                assert!(
                    msg.contains(expected_msg),
                    "Expected message '{}' for path {}",
                    expected_msg,
                    path.display()
                );
            }
            _ => panic!("Expected SecurityViolation for path: {}", path.display()),
        }
    }
}

// @@
```

