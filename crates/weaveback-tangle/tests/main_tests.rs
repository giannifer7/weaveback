// weaveback-tangle/tests/main_tests.rs
// I'd Really Rather You Didn't edit this generated file.

use assert_cmd::prelude::*;
use predicates::prelude::*;
use std::fs;
use std::io::Write;
use std::process::Command;
use tempfile::tempdir;

#[test]
fn test_no_arguments_fails() -> Result<(), Box<dyn std::error::Error>> {
    // Running the binary with no arguments should fail and print usage or an error.
    let mut cmd = Command::cargo_bin("weaveback-tangle")?;
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("required"));

    Ok(())
}

#[test]
fn test_basic_chunk_extraction() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let input_file = dir.path().join("input.nw");
    let mut file = fs::File::create(&input_file)?;
    writeln!(file, "<[@file test.txt]>=")?;
    writeln!(file, "Hello, world!")?;
    writeln!(file, "@")?;

    let gen_dir = dir.path().join("gen");
    fs::create_dir_all(&gen_dir)?;

    let mut cmd = Command::cargo_bin("weaveback-tangle")?;
    cmd.arg("--gen")
        .arg(&gen_dir)
        .arg(&input_file)
        .current_dir(dir.path());

    cmd.assert().success();

    let output_path = gen_dir.join("test.txt");
    let output_content = fs::read_to_string(output_path)?;
    assert_eq!(output_content, "Hello, world!\n");

    Ok(())
}

#[test]
fn test_extract_specific_chunk_to_stdout() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let input_file = dir.path().join("input.nw");
    let mut file = fs::File::create(&input_file)?;
    writeln!(file, "<[chunk1]>=")?;
    writeln!(file, "Chunk 1 content")?;
    writeln!(file, "@")?;
    writeln!(file, "<[chunk2]>=")?;
    writeln!(file, "Chunk 2 content")?;
    writeln!(file, "@")?;

    let gen_dir = dir.path().join("gen");
    fs::create_dir_all(&gen_dir)?;

    let mut cmd = Command::cargo_bin("weaveback-tangle")?;
    cmd.arg("--gen")
        .arg(&gen_dir)
        .arg("--chunks")
        .arg("chunk2")
        .arg(&input_file)
        .current_dir(dir.path());

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Chunk 2 content"));

    Ok(())
}

#[test]
fn test_extract_chunk_to_file() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let input_file = dir.path().join("input.nw");
    {
        let mut file = fs::File::create(&input_file)?;
        writeln!(file, "<[chunk3]>=")?;
        writeln!(file, "This is chunk 3.")?;
        writeln!(file, "@")?;
    }

    let output_file = dir.path().join("chunk3_output.txt");
    let gen_dir = dir.path().join("gen");
    fs::create_dir_all(&gen_dir)?;

    let mut cmd = Command::cargo_bin("weaveback-tangle")?;
    cmd.arg("--gen")
        .arg(&gen_dir)
        .arg("--chunks")
        .arg("chunk3")
        .arg("--output")
        .arg(&output_file)
        .arg(&input_file)
        .current_dir(dir.path());

    cmd.assert().success();

    let content = fs::read_to_string(&output_file)?;
    assert!(content.contains("This is chunk 3."));

    Ok(())
}

