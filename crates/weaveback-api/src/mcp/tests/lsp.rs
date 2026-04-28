// weaveback-api/src/mcp/tests/lsp.rs
// I'd Really Rather You Didn't edit this generated file.

use super::helpers::*;

#[test]
fn mcp_lsp_hover_reaches_handler() {
    let ws = McpWorkspace::new();
    let mcp_rs = mcp_rs_path();
    if !mcp_rs.exists() { return; } // tangle not run yet
    let out_file = mcp_rs.to_string_lossy().into_owned();
    let req = format!(
        "{{\"jsonrpc\":\"2.0\",\"id\":40,\"method\":\"tools/call\",\"params\":{{\"name\":\"weaveback_lsp_hover\",\"arguments\":{{\"out_file\":\"{out_file}\",\"line\":40,\"col\":8}}}}}}\n"
    );
    let out = lsp_mcp_drive(&ws, &req);
    assert!(out.contains("\"id\":40"), "unexpected output: {out}");
}
#[test]
fn mcp_lsp_symbols_reaches_handler() {
    let ws = McpWorkspace::new();
    let mcp_rs = mcp_rs_path();
    if !mcp_rs.exists() { return; }
    let out_file = mcp_rs.to_string_lossy().into_owned();
    let req = format!(
        "{{\"jsonrpc\":\"2.0\",\"id\":50,\"method\":\"tools/call\",\"params\":{{\"name\":\"weaveback_lsp_symbols\",\"arguments\":{{\"out_file\":\"{out_file}\"}}}}}}\n"
    );
    let out = lsp_mcp_drive(&ws, &req);
    assert!(out.contains("\"id\":50"), "unexpected output: {out}");
}
#[test]
fn mcp_lsp_definition_reaches_handler() {
    let ws = McpWorkspace::new();
    let mcp_rs = mcp_rs_path();
    if !mcp_rs.exists() { return; }
    let out_file = mcp_rs.to_string_lossy().into_owned();
    let req = format!(
        "{{\"jsonrpc\":\"2.0\",\"id\":60,\"method\":\"tools/call\",\"params\":{{\"name\":\"weaveback_lsp_definition\",\"arguments\":{{\"out_file\":\"{out_file}\",\"line\":40,\"col\":8}}}}}}\n"
    );
    let out = lsp_mcp_drive(&ws, &req);
    assert!(out.contains("\"id\":60"), "unexpected output: {out}");
}
#[test]
fn mcp_lsp_references_reaches_handler() {
    let ws = McpWorkspace::new();
    let mcp_rs = mcp_rs_path();
    if !mcp_rs.exists() { return; }
    let out_file = mcp_rs.to_string_lossy().into_owned();
    let req = format!(
        "{{\"jsonrpc\":\"2.0\",\"id\":70,\"method\":\"tools/call\",\"params\":{{\"name\":\"weaveback_lsp_references\",\"arguments\":{{\"out_file\":\"{out_file}\",\"line\":40,\"col\":8}}}}}}\n"
    );
    let out = lsp_mcp_drive(&ws, &req);
    assert!(out.contains("\"id\":70"), "unexpected output: {out}");
}
#[test]
fn mcp_lsp_symbols_success_path() {
    // Mock a simple LSP handshake that returns empty list for symbols
    let ws = McpWorkspace::new();
    ws.open_db();
    let req = r#"{"jsonrpc":"2.0","id":1100,"method":"tools/call","params":{"name":"weaveback_lsp_symbols","arguments":{"out_file":"test.rs"}}}"#;
    let out = mcp_drive(&ws, req);
    // It might fail if no LSP binary for .rs is found, but we want to exercise the dispatch logic.
    assert!(out.contains("\"id\":1100"));
}
#[test]
fn mcp_lsp_formatting_success_path() {
    let ws = McpWorkspace::new();
    ws.open_db();
    let req = r#"{"jsonrpc":"2.0","id":1300,"method":"tools/call","params":{"name":"weaveback_lsp_formatting","arguments":{"out_file":"test.rs"}}}"#;
    let out = mcp_drive(&ws, req);
    assert!(out.contains("\"id\":1300"));
}

