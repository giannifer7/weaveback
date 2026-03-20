use super::*;
use crate::WeavebackError;
use crate::SafeWriterError;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;
use tempfile::TempDir;

// Helper functions to reduce code duplication
fn create_test_writer() -> (TempDir, SafeFileWriter) {
    let temp = TempDir::new().unwrap();
    let writer = SafeFileWriter::new(temp.path().join("gen"), temp.path().join("private"));
    (temp, writer)
}

fn write_file(
    writer: &mut SafeFileWriter,
    path: &PathBuf,
    content: &str,
) -> Result<(), WeavebackError> {
    let private_path = writer.before_write(path)?;
    {
        let mut file = fs::File::create(&private_path)?;
        write!(file, "{}", content)?;
    }
    Ok(writer.after_write(path)?)
}

// Basic functionality tests
#[test]
fn test_basic_file_writing() -> Result<(), WeavebackError> {
    let (_temp, mut writer) = create_test_writer();
    let test_file = PathBuf::from("test.txt");
    let test_content = "Hello, World!";

    write_file(&mut writer, &test_file, test_content)?;

    let final_path = writer.get_gen_base().join(&test_file);
    let content = fs::read_to_string(&final_path)?;
    assert_eq!(content, test_content);

    Ok(())
}

#[test]
fn test_unmodified_file_update() -> Result<(), WeavebackError> {
    let (_temp, mut writer) = create_test_writer();
    let test_file = PathBuf::from("test.txt");

    write_file(&mut writer, &test_file, "Initial content")?;
    write_file(&mut writer, &test_file, "New content")?;

    let content = fs::read_to_string(writer.get_gen_base().join(&test_file))?;
    assert_eq!(
        content.as_str(),
        "New content",
        "New content should be written"
    );

    Ok(())
}

// Backup handling tests
#[test]
fn test_backup_creation() -> Result<(), WeavebackError> {
    let (_temp, mut writer) = create_test_writer();
    let test_file = PathBuf::from("test.txt");
    let content = "Test content";

    write_file(&mut writer, &test_file, content)?;

    let backup_path = writer.get_old_dir().join(&test_file);
    assert!(backup_path.exists(), "Backup file should exist");

    let backup_content = fs::read_to_string(backup_path)?;
    assert_eq!(
        backup_content, content,
        "Backup content should match original"
    );

    Ok(())
}

// Directory structure tests
#[test]
fn test_nested_directory_creation() -> Result<(), WeavebackError> {
    let (_temp, mut writer) = create_test_writer();
    let nested_path = PathBuf::from("dir1/dir2/test.txt");

    write_file(&mut writer, &nested_path, "Nested content")?;

    let gen_dir = writer.get_gen_base().join("dir1").join("dir2");
    let old_dir = writer.get_old_dir().join("dir1").join("dir2");
    let private_dir = writer.get_private_dir().join("dir1").join("dir2");

    assert!(
        gen_dir.exists(),
        "Generated directory structure should exist"
    );
    assert!(old_dir.exists(), "Backup directory structure should exist");
    assert!(
        private_dir.exists(),
        "Private directory structure should exist"
    );

    Ok(())
}

// Modification detection tests
#[test]
fn test_modification_detection() -> Result<(), WeavebackError> {
    let (_temp, mut writer) = create_test_writer();
    let test_file = PathBuf::from("test.txt");
    let modified_content = "Modified content";

    // Initial write
    write_file(&mut writer, &test_file, "Initial content")?;

    // External modification
    thread::sleep(Duration::from_millis(10));
    let final_path = writer.get_gen_base().join(&test_file);
    {
        let mut file = fs::File::create(&final_path)?;
        write!(file, "{}", modified_content)?;
    }

    // Attempt to write should fail with ModifiedExternally
    let result = write_file(&mut writer, &test_file, "New content");
    match result {
        Err(WeavebackError::SafeWriter(SafeWriterError::ModifiedExternally(_))) => {
            let content = fs::read_to_string(&final_path)?;
            assert_eq!(
                content, modified_content,
                "Modified content should be preserved"
            );
            Ok(())
        }
        Ok(_) => panic!("Expected ModifiedExternally error"),
        Err(e) => panic!("Unexpected error: {}", e),
    }
}

#[test]
fn test_concurrent_modifications() -> Result<(), WeavebackError> {
    let (_temp, mut writer) = create_test_writer();
    let test_file = PathBuf::from("test.txt");
    let modified_content_2 = "Modified 2";

    // Initial write
    write_file(&mut writer, &test_file, "Initial")?;

    // External modifications
    let final_path = writer.get_gen_base().join(&test_file);
    thread::sleep(Duration::from_millis(10));
    {
        let mut file = fs::File::create(&final_path)?;
        write!(file, "Modified 1")?;
    }
    thread::sleep(Duration::from_millis(10));
    {
        let mut file = fs::File::create(&final_path)?;
        write!(file, "{}", modified_content_2)?;
    }

    // Attempt to write should fail with ModifiedExternally
    let result = write_file(&mut writer, &test_file, "New content");
    match result {
        Err(WeavebackError::SafeWriter(SafeWriterError::ModifiedExternally(_))) => {
            let content = fs::read_to_string(&final_path)?;
            assert_eq!(
                content, modified_content_2,
                "Latest modification should be preserved"
            );
            Ok(())
        }
        Ok(_) => panic!("Expected ModifiedExternally error"),
        Err(e) => panic!("Unexpected error: {}", e),
    }
}

// Content comparison tests
#[test]
fn test_copy_if_different_with_same_content() -> Result<(), WeavebackError> {
    let (_temp, mut writer) = create_test_writer();
    let test_file = PathBuf::from("test.txt");
    let content = "Same content";

    write_file(&mut writer, &test_file, content)?;

    let final_path = writer.get_gen_base().join(&test_file);
    let initial_mtime = fs::metadata(&final_path)?.modified()?;

    thread::sleep(Duration::from_millis(10));
    write_file(&mut writer, &test_file, content)?;

    let new_mtime = fs::metadata(&final_path)?.modified()?;
    assert_eq!(
        initial_mtime, new_mtime,
        "File should not be modified if content is the same"
    );

    Ok(())
}

// Updated test to reflect new behavior on invalid (absolute) path
#[test]
fn test_invalid_path() -> Result<(), WeavebackError> {
    let (_temp, mut writer) = create_test_writer();
    // Previously tested directory creation error on absolute path, but now it triggers SecurityViolation.
    let invalid_path = PathBuf::from("/nonexistent/path/test.txt");

    match write_file(&mut writer, &invalid_path, "content") {
        Err(WeavebackError::SafeWriter(SafeWriterError::SecurityViolation(msg))) => {
            assert!(
                msg.contains("Absolute paths are not allowed"),
                "Expected SecurityViolation for absolute path"
            );
            Ok(())
        }
        Ok(_) => panic!("Expected SecurityViolation error"),
        Err(e) => panic!("Unexpected error: {}", e),
    }
}

#[test]
fn test_modified_externally_is_detected() -> Result<(), WeavebackError> {
    let (_temp, mut writer) = create_test_writer();
    let test_file = PathBuf::from("test.txt");

    // Write initial content to establish a baseline in __old__/
    write_file(&mut writer, &test_file, "Initial content")?;

    // Simulate an external edit to the gen/ file
    thread::sleep(Duration::from_millis(10));
    let final_path = writer.get_gen_base().join(&test_file);
    {
        let mut file = fs::File::create(&final_path)?;
        write!(file, "Modified externally")?;
    }

    // Next run must refuse to overwrite and report ModifiedExternally
    let result = write_file(&mut writer, &test_file, "New content");
    assert!(
        matches!(
            result,
            Err(WeavebackError::SafeWriter(SafeWriterError::ModifiedExternally(_)))
        ),
        "Expected ModifiedExternally, got: {:?}",
        result
    );

    // The externally modified content must be preserved
    let content = fs::read_to_string(&final_path)?;
    assert_eq!(content, "Modified externally");

    Ok(())
}

#[test]
fn test_backup_disabled() -> Result<(), WeavebackError> {
    let (_temp, mut writer) = create_test_writer();
    let test_file = PathBuf::from("test.txt");

    // Disable backups
    let mut config = writer.get_config().clone();
    config.backup_enabled = false;
    writer.set_config(config);

    write_file(&mut writer, &test_file, "Test content")?;

    let backup_path = writer.get_old_dir().join(&test_file);
    assert!(
        !backup_path.exists(),
        "Backup file should not exist when backups are disabled"
    );

    Ok(())
}

// ===== New tests for validate_filename =====

#[test]
fn test_validate_filename_relative_path() -> Result<(), WeavebackError> {
    let (_temp, mut writer) = create_test_writer();
    // A simple relative path should be allowed
    let test_file = PathBuf::from("simple.txt");
    write_file(&mut writer, &test_file, "Allowed")?;
    let final_path = writer.get_gen_base().join(&test_file);
    let content = fs::read_to_string(&final_path)?;
    assert_eq!(content, "Allowed");
    Ok(())
}

#[test]
fn test_validate_filename_absolute_unix() {
    let (_temp, mut writer) = create_test_writer();
    let test_file = PathBuf::from("/absolute/unix/path.txt");

    let result = write_file(&mut writer, &test_file, "Should fail");
    match result {
        Err(WeavebackError::SafeWriter(SafeWriterError::SecurityViolation(msg))) => {
            assert!(
                msg.contains("Absolute paths are not allowed"),
                "Expected absolute path error message"
            );
        }
        _ => panic!("Expected SecurityViolation for absolute Unix path"),
    }
}

#[test]
fn test_validate_filename_absolute_windows() {
    let (_temp, mut writer) = create_test_writer();
    let test_file = PathBuf::from("C:/windows/path.txt");

    let result = write_file(&mut writer, &test_file, "Should fail");
    match result {
        Err(WeavebackError::SafeWriter(SafeWriterError::SecurityViolation(msg))) => {
            assert!(
                msg.contains("Windows-style absolute paths are not allowed"),
                "Expected windows-style absolute path error message"
            );
        }
        _ => panic!("Expected SecurityViolation for Windows absolute path"),
    }
}

#[test]
fn test_validate_filename_drive_letter_without_slash() {
    let (_temp, mut writer) = create_test_writer();
    let test_file = PathBuf::from("C:test.txt");

    let result = write_file(&mut writer, &test_file, "Should fail");
    match result {
        Err(WeavebackError::SafeWriter(SafeWriterError::SecurityViolation(msg))) => {
            assert!(
                msg.contains("Windows-style absolute paths are not allowed"),
                "Expected windows-style absolute path error message"
            );
        }
        _ => panic!("Expected SecurityViolation for Windows-style drive path"),
    }
}

#[test]
fn test_validate_filename_parent_traversal() {
    let (_temp, mut writer) = create_test_writer();
    let test_file = PathBuf::from("../outside.txt");

    let result = write_file(&mut writer, &test_file, "Should fail");
    match result {
        Err(WeavebackError::SafeWriter(SafeWriterError::SecurityViolation(msg))) => {
            assert!(
                msg.contains("Path traversal detected (..)"),
                "Expected path traversal error message"
            );
        }
        _ => panic!("Expected SecurityViolation for path traversal"),
    }
}

#[test]
fn test_validate_filename_nested_parent_traversal() {
    let (_temp, mut writer) = create_test_writer();
    let test_file = PathBuf::from("dir1/../dir2/test.txt");

    let result = write_file(&mut writer, &test_file, "Should fail");
    match result {
        Err(WeavebackError::SafeWriter(SafeWriterError::SecurityViolation(msg))) => {
            assert!(
                msg.contains("Path traversal detected (..)"),
                "Expected path traversal error message"
            );
        }
        _ => panic!("Expected SecurityViolation for nested path traversal"),
    }
}
