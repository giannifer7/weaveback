#[test]
fn test_wb_mcp_help() {
    // env!("CARGO_BIN_EXE_wb-mcp") is only available if wb-mcp is a bin member of the current crate.
    // Since this is a workspace, we might need to find the binary path manually or use a different approach.
    // Actually, integration tests in a crate CAN access its own binaries via CARGO_BIN_EXE_<name>.
    let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_wb-mcp"));
    cmd.arg("--help");
    let output = cmd.output().expect("failed to execute wb-mcp");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("wb-mcp"));
}

#[test]
fn test_wb_mcp_initialize() {
    use std::io::Write;
    let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_wb-mcp"));
    cmd.arg("--db").arg("smoke.db").arg("--gen").arg("gen");
    cmd.stdin(std::process::Stdio::piped());
    cmd.stdout(std::process::Stdio::piped());
    
    let mut child = cmd.spawn().expect("failed to spawn wb-mcp");
    let mut stdin = child.stdin.take().unwrap();
    
    writeln!(stdin, "{{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\"}}").unwrap();
    drop(stdin);
    
    let output = child.wait_with_output().expect("failed to wait for wb-mcp");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("protocolVersion"));
}
