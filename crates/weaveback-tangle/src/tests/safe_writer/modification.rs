// weaveback-tangle/src/tests/safe_writer/modification.rs
// I'd Really Rather You Didn't edit this generated file.

use super::*;

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

