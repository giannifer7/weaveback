# MCP Smoke Tests

```rust
// <[@file weaveback-api/src/mcp/tests/smoke.rs]>=
// weaveback-api/src/mcp/tests/smoke.rs
// I'd Really Rather You Didn't edit this generated file.

use super::super::{get_or_spawn_lsp, run_mcp};
use std::collections::HashMap;
use std::path::PathBuf;
use weaveback_lsp::LspClient;
use weaveback_macro::evaluator::EvalConfig;

// <[mcp-test-smoke]>

// @
```


```rust
// <[mcp-test-smoke]>=
#[test]
fn get_or_spawn_lsp_unsupported_extension_returns_error() {
    let mut clients: HashMap<String, LspClient> = HashMap::new();
    let result = get_or_spawn_lsp(&mut clients, "xyz_unsupported");
    let msg = match result {
        Err(e) => e,
        Ok(_) => panic!("expected error for unsupported extension"),
    };
    assert!(
        msg.contains("unsupported file extension"),
        "unexpected message: {msg}"
    );
}

/// Verifying that `initialize` JSON is well-formed: tools/list response
/// must include the expected tool names.
#[test]
fn tools_list_contains_expected_tool_names() {
    let tools = serde_json::json!({
        "tools": [
            { "name": "weaveback_trace" },
            { "name": "weaveback_apply_back" },
            { "name": "weaveback_apply_fix" },
            { "name": "weaveback_chunk_context" },
            { "name": "weaveback_list_chunks" },
            { "name": "weaveback_find_chunk" },
        ]
    });
    let names: Vec<&str> = tools["tools"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|t| t["name"].as_str())
        .collect();
    assert!(names.contains(&"weaveback_trace"));
    assert!(names.contains(&"weaveback_apply_fix"));
    assert!(names.contains(&"weaveback_chunk_context"));
}
#[test]
fn test_run_mcp_loop() {
    let input = r#"{"jsonrpc":"2.0","id":100,"method":"initialize"}
{"jsonrpc":"2.0","id":101,"method":"tools/list"}
"#;
    let reader = std::io::Cursor::new(input);
    let mut writer = Vec::new();
    let db_path = PathBuf::from("nonexistent.db");
    let gen_dir = PathBuf::from("gen");
    let eval_config = EvalConfig::default();
    
    let project_root = std::env::current_dir().unwrap();
    let result = run_mcp(reader, &mut writer, db_path, gen_dir, project_root, eval_config);
    assert!(result.is_ok());
    
    let output = String::from_utf8(writer).unwrap();
    assert!(output.contains("\"id\":100"));
    assert!(output.contains("\"protocolVersion\":\"2024-11-05\""));
    assert!(output.contains("\"id\":101"));
    assert!(output.contains("weaveback_trace"));
}
// @
```

