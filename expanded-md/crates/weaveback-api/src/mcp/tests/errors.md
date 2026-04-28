# MCP Error-Path Tests

```rust
// <[@file weaveback-api/src/mcp/tests/errors.rs]>=
// weaveback-api/src/mcp/tests/errors.rs
// I'd Really Rather You Didn't edit this generated file.

use super::helpers::*;

// <[mcp-test-errors]>

// @
```


```rust
// <[mcp-test-errors]>=
#[test]
fn mcp_trace_missing_db_returns_error() {
    let ws = McpWorkspace::new();
    let req = "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/call\",\"params\":{\"name\":\"weaveback_trace\",\"arguments\":{\"out_file\":\"foo.rs\",\"out_line\":1}}}\n";
    let out = mcp_drive(&ws, req);
    assert!(out.contains("Database not found") || out.contains("isError"));
}
#[test]
fn mcp_trace_missing_arguments_returns_error() {
    let ws = McpWorkspace::new();
    let req = "{\"jsonrpc\":\"2.0\",\"id\":3,\"method\":\"tools/call\",\"params\":{\"name\":\"weaveback_trace\"}}\n";
    let out = mcp_drive(&ws, req);
    assert!(out.contains("isError") || out.contains("Missing"));
}
#[test]
fn mcp_apply_fix_zero_src_line_returns_error() {
    let ws = McpWorkspace::new();
    let req = "{\"jsonrpc\":\"2.0\",\"id\":4,\"method\":\"tools/call\",\"params\":{\"name\":\"weaveback_apply_fix\",\"arguments\":{\"src_file\":\"a.adoc\",\"src_line\":0,\"out_file\":\"a.rs\",\"out_line\":1,\"expected_output\":\"x\"}}}\n";
    let out = mcp_drive(&ws, req);
    assert!(out.contains("src_line must be"));
}
#[test]
fn mcp_apply_fix_inverted_range_returns_error() {
    let ws = McpWorkspace::new();
    let req = "{\"jsonrpc\":\"2.0\",\"id\":5,\"method\":\"tools/call\",\"params\":{\"name\":\"weaveback_apply_fix\",\"arguments\":{\"src_file\":\"a.adoc\",\"src_line\":10,\"src_line_end\":5,\"out_file\":\"a.rs\",\"out_line\":1,\"expected_output\":\"x\"}}}\n";
    let out = mcp_drive(&ws, req);
    assert!(out.contains("src_line_end must be"));
}
#[test]
fn mcp_chunk_context_empty_args_returns_error() {
    let ws = McpWorkspace::new();
    let req = "{\"jsonrpc\":\"2.0\",\"id\":6,\"method\":\"tools/call\",\"params\":{\"name\":\"weaveback_chunk_context\",\"arguments\":{\"file\":\"\",\"name\":\"\"}}}\n";
    let out = mcp_drive(&ws, req);
    assert!(out.contains("isError"));
}
#[test]
fn mcp_list_chunks_missing_db_returns_error() {
    let ws = McpWorkspace::new();
    let req = "{\"jsonrpc\":\"2.0\",\"id\":7,\"method\":\"tools/call\",\"params\":{\"name\":\"weaveback_list_chunks\",\"arguments\":{}}}\n";
    let out = mcp_drive(&ws, req);
    assert!(out.contains("Database not found"));
}
#[test]
fn mcp_find_chunk_empty_name_returns_error() {
    let ws = McpWorkspace::new();
    let req = "{\"jsonrpc\":\"2.0\",\"id\":8,\"method\":\"tools/call\",\"params\":{\"name\":\"weaveback_find_chunk\",\"arguments\":{\"name\":\"\"}}}\n";
    let out = mcp_drive(&ws, req);
    assert!(out.contains("isError") || out.contains("name is required"));
}
#[test]
fn mcp_search_empty_query_returns_error() {
    let ws = McpWorkspace::new();
    let req = "{\"jsonrpc\":\"2.0\",\"id\":9,\"method\":\"tools/call\",\"params\":{\"name\":\"weaveback_search\",\"arguments\":{\"query\":\"\"}}}\n";
    let out = mcp_drive(&ws, req);
    assert!(out.contains("isError") || out.contains("query is required"));
}
#[test]
fn mcp_list_tags_missing_db_returns_error() {
    let ws = McpWorkspace::new();
    let req = "{\"jsonrpc\":\"2.0\",\"id\":10,\"method\":\"tools/call\",\"params\":{\"name\":\"weaveback_list_tags\",\"arguments\":{}}}\n";
    let out = mcp_drive(&ws, req);
    assert!(out.contains("Database not found"));
}
#[test]
fn mcp_coverage_missing_lcov_returns_error() {
    let ws = McpWorkspace::new();
    let req = "{\"jsonrpc\":\"2.0\",\"id\":11,\"method\":\"tools/call\",\"params\":{\"name\":\"weaveback_coverage\",\"arguments\":{\"lcov_path\":\"/nonexistent/lcov.info\"}}}\n";
    let out = mcp_drive(&ws, req);
    assert!(out.contains("lcov file not found") || out.contains("isError"));
}
#[test]
fn mcp_apply_back_missing_db_sends_response() {
    let ws = McpWorkspace::new();
    let req = "{\"jsonrpc\":\"2.0\",\"id\":12,\"method\":\"tools/call\",\"params\":{\"name\":\"weaveback_apply_back\",\"arguments\":{}}}\n";
    let out = mcp_drive(&ws, req);
    assert!(!out.is_empty());
    assert!(out.contains("\"id\":12"));
}

// ── tools/call – success paths (real DB) ─────────────────────────────────
#[test]
fn mcp_search_missing_db_returns_error() {
    let ws = McpWorkspace::new();
    let req = "{\"jsonrpc\":\"2.0\",\"id\":23,\"method\":\"tools/call\",\"params\":{\"name\":\"weaveback_search\",\"arguments\":{\"query\":\"hello\"}}}\n";
    let out = mcp_drive(&ws, req);
    assert!(out.contains("Database not found"));
}
#[test]
fn mcp_lsp_tools_missing_db_error() {
    let ws = McpWorkspace::new();
    // Database not found path for LSP tools
    let reqs = [
        r#"{"jsonrpc":"2.0","id":400,"method":"tools/call","params":{"name":"weaveback_lsp_definition","arguments":{"out_file":"test.rs","line":1,"col":1}}}"#,
    ].join("\n") + "\n";
    let out = mcp_drive(&ws, &reqs);
    // Depending on the environment, this may fail before any DB-backed
    // operation with an LSP initialization error.
    assert!(
        out.contains("Database not found")
            || out.contains("LSP call failed")
            || out.contains("failed to initialize LSP"),
        "output: {out}"
    );
}
#[test]
fn mcp_lsp_unsupported_extension_error() {
    let ws = McpWorkspace::new();
    ws.open_db();
    let reqs = [
        r#"{"jsonrpc":"2.0","id":500,"method":"tools/call","params":{"name":"weaveback_lsp_definition","arguments":{"out_file":"test.unknown","line":1,"col":1}}}"#,
    ].join("\n") + "\n";
    let out = mcp_drive(&ws, &reqs);
    assert!(out.contains("LSP error") || out.contains("not supported"), "output: {out}");
}
#[test]
fn mcp_apply_fix_invalid_args() {
    let ws = McpWorkspace::new();
    let reqs = [
        r#"{"jsonrpc":"2.0","id":600,"method":"tools/call","params":{"name":"weaveback_apply_fix","arguments":{"src_file":"a.adoc"}}}"#, // missing most args
    ].join("\n") + "\n";
    let out = mcp_drive(&ws, &reqs);
    assert!(out.contains("isError") || out.contains("Missing arguments"));
}
#[test]
fn mcp_chunk_context_not_found() {
    let ws = McpWorkspace::new();
    ws.open_db();
    let req = r#"{"jsonrpc":"2.0","id":700,"method":"tools/call","params":{"name":"weaveback_chunk_context","arguments":{"file":"missing.adoc","name":"none"}}}"#;
    let out = mcp_drive(&ws, req);
    assert!(out.contains("Chunk not found"));
}
#[test]
fn mcp_find_chunk_empty_name_error() {
    let ws = McpWorkspace::new();
    ws.open_db();
    let req = r#"{"jsonrpc":"2.0","id":800,"method":"tools/call","params":{"name":"weaveback_find_chunk","arguments":{"name":""}}}"#;
    let out = mcp_drive(&ws, req);
    assert!(out.contains("name is required"));
}
// @
```

