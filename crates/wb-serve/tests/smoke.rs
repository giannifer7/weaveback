#[test]
fn test_wb_serve_help() {
    let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_wb-serve"));
    cmd.arg("--help");
    let output = cmd.output().expect("failed to execute wb-serve");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("wb-serve"));
}

#[test]
fn test_wb_serve_invalid_port() {
    let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_wb-serve"));
    cmd.arg("--port").arg("99999"); // Invalid port
    let output = cmd.output().expect("failed to execute wb-serve");
    assert!(!output.status.success());
}
