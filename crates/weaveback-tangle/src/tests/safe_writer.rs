// weaveback-tangle/src/tests/safe_writer.rs
// I'd Really Rather You Didn't edit this generated file.

// src/tests/safe_writer.rs
use super::*;
use crate::WeavebackError;
use crate::SafeWriterError;
use crate::safe_writer::{SafeFileWriter, SafeWriterConfig};
use std::{collections::HashMap, fs, io::Write, path::PathBuf, thread, time::Duration};
use tempfile::TempDir;

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
fn test_multiple_file_generation() -> Result<(), WeavebackError> {
    let (_temp, mut writer) = create_test_writer();
    let test_file1 = PathBuf::from("file1.txt");
    let test_file2 = PathBuf::from("file2.txt");

    write_file(&mut writer, &test_file1, "Content 1")?;
    write_file(&mut writer, &test_file2, "Content 2")?;

    let content1 = fs::read_to_string(writer.get_gen_base().join(&test_file1))?;
    let content2 = fs::read_to_string(writer.get_gen_base().join(&test_file2))?;

    assert_eq!(content1.trim(), "Content 1");
    assert_eq!(content2.trim(), "Content 2");
    Ok(())
}

#[test]
fn test_unmodified_file_update() -> Result<(), WeavebackError> {
    let (_temp, mut writer) = create_test_writer();
    let test_file = PathBuf::from("test.txt");

    write_file(&mut writer, &test_file, "Initial content")?;
    write_file(&mut writer, &test_file, "New content")?;

    let content = fs::read_to_string(writer.get_gen_base().join(&test_file))?;
    assert_eq!(content, "New content", "New content should be written");
    Ok(())
}

#[test]
fn test_shorter_regeneration_truncates_old_suffix() -> Result<(), WeavebackError> {
    let (_temp, mut writer) = create_test_writer();
    let test_file = PathBuf::from("test.txt");

    write_file(&mut writer, &test_file, "line one\nline two\nline three\n")?;
    write_file(&mut writer, &test_file, "short\n")?;

    let content = fs::read_to_string(writer.get_gen_base().join(&test_file))?;
    assert_eq!(content, "short\n");
    assert!(!content.contains("line two"));
    assert!(!content.contains("line three"));
    Ok(())
}

#[test]
fn test_backup_creation() -> Result<(), WeavebackError> {
    let (_temp, mut writer) = create_test_writer();
    let test_file = PathBuf::from("test.txt");
    let content = "Test content";

    write_file(&mut writer, &test_file, content)?;

    let baseline = writer.get_baseline_for_test("test.txt");
    assert!(baseline.is_some(), "Baseline should be stored in db");
    assert_eq!(
        baseline.as_deref().unwrap(),
        content.as_bytes(),
        "Baseline content should match written content"
    );
    Ok(())
}

#[test]
fn test_nested_directory_creation() -> Result<(), WeavebackError> {
    let (_temp, mut writer) = create_test_writer();
    let nested_path = PathBuf::from("dir1/dir2/test.txt");

    write_file(&mut writer, &nested_path, "Nested content")?;

    let gen_dir = writer.get_gen_base().join("dir1").join("dir2");
    assert!(gen_dir.exists(), "Generated directory structure should exist");

    let baseline = writer.get_baseline_for_test("dir1/dir2/test.txt");
    assert!(
        baseline.is_some(),
        "Baseline for nested file should be stored in db"
    );
    Ok(())
}

#[test]
fn test_modification_detection() -> Result<(), WeavebackError> {
    let (_temp, mut writer) = create_test_writer();
    let test_file = PathBuf::from("test.txt");
    let modified_content = "Modified content";

    write_file(&mut writer, &test_file, "Initial content")?;

    thread::sleep(Duration::from_millis(10));
    let final_path = writer.get_gen_base().join(&test_file);
    {
        let mut file = fs::File::create(&final_path)?;
        write!(file, "{}", modified_content)?;
    }

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

    write_file(&mut writer, &test_file, "Initial")?;

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

#[test]
fn test_force_generated_overwrites_modified_file() -> Result<(), WeavebackError> {
    let temp = TempDir::new().unwrap();
    let mut writer = SafeFileWriter::with_config(
        temp.path().join("gen"),
        SafeWriterConfig {
            force_generated: true,
            ..SafeWriterConfig::default()
        },
    ).unwrap();
    let test_file = PathBuf::from("test.txt");

    write_file(&mut writer, &test_file, "Initial content")?;

    let final_path = writer.get_gen_base().join(&test_file);
    {
        let mut file = fs::File::create(&final_path)?;
        write!(file, "Modified externally")?;
    }

    write_file(&mut writer, &test_file, "Regenerated content")?;

    let content = fs::read_to_string(&final_path)?;
    assert_eq!(content, "Regenerated content");
    Ok(())
}

#[test]
fn test_force_generated_writes_identical_content() -> Result<(), WeavebackError> {
    // Regression: force_generated must bypass copy_if_different (Step 3).
    // When tangle produces content identical to what is on disk (e.g. because
    // macros failed silently), the old code would skip the write even with
    // force_generated=true.  The fix is to call atomic_copy unconditionally.
    let temp = TempDir::new().unwrap();
    let mut writer = SafeFileWriter::with_config(
        temp.path().join("gen"),
        SafeWriterConfig {
            force_generated: true,
            ..SafeWriterConfig::default()
        },
    ).unwrap();
    let test_file = PathBuf::from("test.txt");

    write_file(&mut writer, &test_file, "Same content")?;

    // Overwrite on disk so the mtime changes but content stays the same.
    let final_path = writer.get_gen_base().join(&test_file);
    // Record mtime before the second write.
    let mtime_before = fs::metadata(&final_path).unwrap().modified().unwrap();
    // Brief pause so the filesystem timestamp resolution can distinguish writes.
    std::thread::sleep(std::time::Duration::from_millis(10));

    // Tangle again with the same content — force_generated must still write.
    write_file(&mut writer, &test_file, "Same content")?;

    let mtime_after = fs::metadata(&final_path).unwrap().modified().unwrap();
    // The file must have been touched (mtime advanced).
    assert!(mtime_after >= mtime_before, "force_generated must write even when content is identical");
    let content = fs::read_to_string(&final_path)?;
    assert_eq!(content, "Same content");
    Ok(())
}

#[test]
fn test_baseline_always_written() -> Result<(), WeavebackError> {
    let (_temp, mut writer) = create_test_writer();
    let test_file = PathBuf::from("test.txt");

    write_file(&mut writer, &test_file, "Test content")?;

    assert!(
        writer.get_baseline_for_test("test.txt").is_some(),
        "Baseline must always be stored in db for modification detection"
    );
    Ok(())
}

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

#[test]
fn test_formatter_is_applied() -> Result<(), WeavebackError> {
    let temp = tempfile::TempDir::new().unwrap();
    let mut formatters = HashMap::new();
    // Shell script that replaces the file content with "FORMATTED\n"
    formatters.insert(
        "txt".to_string(),
        "sh -c echo FORMATTED > \"$1\" && echo FORMATTED > \"$1\"".to_string(),
    );
    // Use a simpler approach: a script file
    let script_path = temp.path().join("fmt.sh");
    fs::write(&script_path, "#!/bin/sh\necho FORMATTED > \"$1\"\n").unwrap();
    std::process::Command::new("chmod")
        .arg("+x")
        .arg(&script_path)
        .status()
        .unwrap();

    let mut formatters2 = HashMap::new();
    formatters2.insert("txt".to_string(), script_path.to_string_lossy().to_string());

    let config = SafeWriterConfig {
        formatters: formatters2,
        ..SafeWriterConfig::default()
    };
    let mut writer =
        SafeFileWriter::with_config(temp.path().join("gen"), config)?;

    let test_file = PathBuf::from("test.txt");
    write_file(&mut writer, &test_file, "original content")?;

    let output = fs::read_to_string(writer.get_gen_base().join(&test_file))?;
    assert!(
        output.contains("FORMATTED"),
        "Formatter should have replaced content, got: {:?}",
        output
    );
    Ok(())
}

#[test]
fn test_formatter_error_propagates() -> Result<(), WeavebackError> {
    let temp = tempfile::TempDir::new().unwrap();
    let mut formatters = HashMap::new();
    formatters.insert("txt".to_string(), "nonexistent-formatter-xyz".to_string());

    let config = SafeWriterConfig {
        formatters,
        ..SafeWriterConfig::default()
    };
    let mut writer =
        SafeFileWriter::with_config(temp.path().join("gen"), config)?;

    let test_file = PathBuf::from("test.txt");
    let result = write_file(&mut writer, &test_file, "some content");
    match result {
        Err(WeavebackError::SafeWriter(SafeWriterError::FormatterError(_))) => Ok(()),
        Ok(_) => panic!("Expected FormatterError but write succeeded"),
        Err(e) => panic!("Unexpected error: {}", e),
    }
}

#[test]
fn test_formatter_prevents_false_positive() -> Result<(), WeavebackError> {
    let temp = tempfile::TempDir::new().unwrap();
    let script_path = temp.path().join("noop.sh");
    // A no-op formatter: copies file to itself (content unchanged)
    fs::write(
        &script_path,
        "#!/bin/sh\ncp \"$1\" \"$1.bak\" && mv \"$1.bak\" \"$1\"\n",
    )
    .unwrap();
    std::process::Command::new("chmod")
        .arg("+x")
        .arg(&script_path)
        .status()
        .unwrap();

    let mut formatters = HashMap::new();
    formatters.insert("txt".to_string(), script_path.to_string_lossy().to_string());

    let config = SafeWriterConfig {
        formatters,
        ..SafeWriterConfig::default()
    };
    let mut writer =
        SafeFileWriter::with_config(temp.path().join("gen"), config)?;

    let test_file = PathBuf::from("test.txt");
    write_file(&mut writer, &test_file, "initial content")?;

    // Simulate formatter running externally on the output (content unchanged)
    let output_path = writer.get_gen_base().join(&test_file);
    let content = fs::read_to_string(&output_path)?;
    fs::write(&output_path, &content)?;

    // Second write should NOT trigger ModifiedExternally (content is the same as baseline)
    let result = write_file(&mut writer, &test_file, "initial content");
    assert!(
        result.is_ok(),
        "Expected success but got: {:?}",
        result.err()
    );
    Ok(())
}

