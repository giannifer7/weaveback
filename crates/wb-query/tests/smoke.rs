#[test]
fn test_wb_query_help() {
    let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_wb-query"));
    cmd.arg("--help");
    let output = cmd.output().expect("failed to execute wb-query");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("wb-query"));
}

#[test]
fn test_wb_query_where_missing_db() {
    let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_wb-query"));
    cmd.arg("--db").arg("nonexistent.db")
       .arg("where").arg("--out-file").arg("test.rs").arg("--line").arg("1");
    let output = cmd.output().expect("failed to execute wb-query");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("wb-query")); 
}
