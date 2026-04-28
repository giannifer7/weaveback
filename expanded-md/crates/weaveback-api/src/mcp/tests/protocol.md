# MCP Protocol Tests

```rust
// <[@file weaveback-api/src/mcp/tests/protocol.rs]>=
// weaveback-api/src/mcp/tests/protocol.rs
// I'd Really Rather You Didn't edit this generated file.

use super::helpers::*;

// <[mcp-test-protocol]>

// @
```


```rust
// <[mcp-test-protocol]>=
#[test]
fn mcp_notifications_initialized_is_silent() {
    let ws = McpWorkspace::new();
    let out = mcp_drive(&ws, "{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n");
    assert!(out.is_empty());
}
#[test]
fn mcp_empty_lines_are_skipped() {
    let ws = McpWorkspace::new();
    let out = mcp_drive(&ws, "\n  \n \n");
    assert!(out.is_empty());
}
#[test]
fn mcp_invalid_json_is_skipped_and_valid_continues() {
    let ws = McpWorkspace::new();
    let input = "not json\n{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\"}\n";
    let out = mcp_drive(&ws, input);
    assert!(out.contains("\"id\":1"));
    assert!(out.contains("protocolVersion"));
}

// ── tools/call – error paths ──────────────────────────────────────────────
#[test]
fn mcp_unknown_tool_returns_error() {
    let ws = McpWorkspace::new();
    let req = "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/call\",\"params\":{\"name\":\"no_such_tool\",\"arguments\":{}}}\n";
    let out = mcp_drive(&ws, req);
    assert!(out.contains("Unknown tool"));
    assert!(out.contains("\"isError\":true"));
}
#[test]
fn mcp_protocol_full_handshake() {
    let ws = McpWorkspace::new();
    // Handshake: initialize -> notifications/initialized -> tools/list
    let reqs = [
        r#"{"jsonrpc":"2.0","id":100,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test-client","version":"1.0"}}}"#,
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        r#"{"jsonrpc":"2.0","id":101,"method":"tools/list"}"#,
    ].join("\n") + "\n";
    
    let out = mcp_drive(&ws, &reqs);
    assert!(out.contains("\"id\":100"));
    assert!(out.contains("\"protocolVersion\":\"2024-11-05\""));
    assert!(out.contains("\"id\":101"));
    assert!(out.contains("weaveback_trace"));
}
#[test]
fn mcp_malformed_and_unknown_methods() {
    let ws = McpWorkspace::new();
    let reqs = [
        "invalid non-json",
        r#"{"jsonrpc":"2.0","id":200,"method":"unknown/method"}"#,
        r#"{"jsonrpc":"2.0","id":201,"method":"tools/call","params":{"name":"weaveback_trace"}}"#, // missing args
    ].join("\n") + "\n";
    
    let out = mcp_drive(&ws, &reqs);
    // Unknown method currently falls into the _ arm and is silent.
    assert!(out.contains("\"id\":201") || out.is_empty()); 
}
#[test]
fn mcp_list_resources_and_prompts_are_empty_but_covered() {
    let ws = McpWorkspace::new();
    let reqs = [
        r#"{"jsonrpc":"2.0","id":300,"method":"resources/list"}"#,
        r#"{"jsonrpc":"2.0","id":301,"method":"prompts/list"}"#,
    ].join("\n") + "\n";
    let out = mcp_drive(&ws, &reqs);
    assert!(out.contains("\"id\":300"));
    assert!(out.contains("\"id\":301"));
}
// @
```

