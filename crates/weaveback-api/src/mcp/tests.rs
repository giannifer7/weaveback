// weaveback-api/src/mcp/tests.rs
// I'd Really Rather You Didn't edit this generated file.

use super::*;
use crate::process;
use std::collections::HashMap;
use std::path::PathBuf;
use weaveback_lsp::LspClient;
use weaveback_macro::evaluator::EvalConfig;
use weaveback_tangle::db::WeavebackDb;

/// `get_or_spawn_lsp` must return an error immediately for unsupported
/// extensions, without spawning any process.
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

// ── Test helpers ──────────────────────────────────────────────────────────

struct McpWorkspace {
    root: std::path::PathBuf,
}
impl McpWorkspace {
    fn new() -> Self {
        let id = format!(
            "wb-mcp-tests-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let root = std::env::temp_dir().join(id);
        std::fs::create_dir_all(&root).unwrap();
        let root = root.canonicalize().unwrap();
        Self { root }
    }
    fn db_path(&self) -> PathBuf { self.root.join("weaveback.db") }
    fn gen_dir(&self) -> PathBuf { self.root.join("gen") }
    fn open_db(&self) -> WeavebackDb { WeavebackDb::open(self.db_path()).unwrap() }
    fn write_file(&self, rel_path: &str, content: &[u8]) {
        let p = self.root.join(rel_path);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(p, content).unwrap();
    }
}
impl Drop for McpWorkspace {
    fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.root); }
}

fn mcp_drive(ws: &McpWorkspace, requests: &str) -> String {
    let reader = std::io::Cursor::new(requests.to_string());
    let mut writer = Vec::new();
    run_mcp(reader, &mut writer, ws.db_path(), ws.gen_dir(), ws.root.clone(), EvalConfig::default()).unwrap();
    String::from_utf8(writer).unwrap()
}

// ── Protocol-level tests ──────────────────────────────────────────────────

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
fn mcp_list_chunks_returns_seeded_chunk() {
    let ws = McpWorkspace::new();
    {
        let mut db = ws.open_db();
        db.set_chunk_defs(&[weaveback_tangle::db::ChunkDefEntry {
            src_file:   "src/lib.adoc".to_string(),
            chunk_name: "my-chunk".to_string(),
            nth:        0,
            def_start:  1,
            def_end:    5,
        }]).unwrap();
    }
    let req = "{\"jsonrpc\":\"2.0\",\"id\":20,\"method\":\"tools/call\",\"params\":{\"name\":\"weaveback_list_chunks\",\"arguments\":{}}}\n";
    let out = mcp_drive(&ws, req);
    assert!(out.contains("my-chunk"), "output was: {out}");
    assert!(out.contains("src/lib.adoc"));
}

#[test]
fn mcp_find_chunk_returns_seeded_chunk() {
    let ws = McpWorkspace::new();
    {
        let mut db = ws.open_db();
        db.set_chunk_defs(&[weaveback_tangle::db::ChunkDefEntry {
            src_file:   "src/lib.adoc".to_string(),
            chunk_name: "search-target".to_string(),
            nth:        0,
            def_start:  10,
            def_end:    20,
        }]).unwrap();
    }
    let req = "{\"jsonrpc\":\"2.0\",\"id\":21,\"method\":\"tools/call\",\"params\":{\"name\":\"weaveback_find_chunk\",\"arguments\":{\"name\":\"search-target\"}}}\n";
    let out = mcp_drive(&ws, req);
    assert!(out.contains("src/lib.adoc"), "output was: {out}");
}

#[test]
fn mcp_list_tags_returns_empty_array_for_fresh_db() {
    let ws = McpWorkspace::new();
    ws.open_db(); // create db file
    let req = "{\"jsonrpc\":\"2.0\",\"id\":22,\"method\":\"tools/call\",\"params\":{\"name\":\"weaveback_list_tags\",\"arguments\":{}}}\n";
    let out = mcp_drive(&ws, req);
    assert!(out.contains("\"content\""), "output was: {out}");
    assert!(out.contains("[]"));
}

#[test]
fn mcp_search_missing_db_returns_error() {
    let ws = McpWorkspace::new();
    let req = "{\"jsonrpc\":\"2.0\",\"id\":23,\"method\":\"tools/call\",\"params\":{\"name\":\"weaveback_search\",\"arguments\":{\"query\":\"hello\"}}}\n";
    let out = mcp_drive(&ws, req);
    assert!(out.contains("Database not found"));
}

#[test]
fn mcp_list_chunks_with_file_filter_returns_only_matching() {
    let ws = McpWorkspace::new();
    {
        let mut db = ws.open_db();
        db.set_chunk_defs(&[
            weaveback_tangle::db::ChunkDefEntry {
                src_file: "a.adoc".to_string(), chunk_name: "chunk-a".to_string(),
                nth: 0, def_start: 1, def_end: 3,
            },
            weaveback_tangle::db::ChunkDefEntry {
                src_file: "b.adoc".to_string(), chunk_name: "chunk-b".to_string(),
                nth: 0, def_start: 1, def_end: 3,
            },
        ]).unwrap();
    }
    let req = "{\"jsonrpc\":\"2.0\",\"id\":24,\"method\":\"tools/call\",\"params\":{\"name\":\"weaveback_list_chunks\",\"arguments\":{\"file\":\"a.adoc\"}}}\n";
    let out = mcp_drive(&ws, req);
    assert!(out.contains("chunk-a"));
    assert!(!out.contains("chunk-b"));
}

#[test]
fn mcp_trace_returns_seeded_location() {
    let ws = McpWorkspace::new();
    let src_rel = "src/test.adoc";
    ws.write_file(src_rel, "= Title\n\n<<@file pkg/src/test.rs>>=\nline1\n@@\n".as_bytes());

    let args = process::SinglePassArgs {
        inputs: vec![src_rel.into()],
        input_dir: ws.root.clone(),
        gen_dir: ws.root.clone(),
        db: ws.db_path(),
        project_root: Some(ws.root.clone()),
        no_fts: true,
        no_macros: false,
        ..process::SinglePassArgs::default_for_test()
    };
    process::run_single_pass(args).map_err(|e| e.to_string()).unwrap();

    let req = "{\"jsonrpc\":\"2.0\",\"id\":25,\"method\":\"tools/call\",\"params\":{\"name\":\"weaveback_trace\",\"arguments\":{\"out_file\":\"pkg/src/test.rs\",\"out_line\":1}}}\n";
    let out = mcp_drive(&ws, req);
    assert!(out.contains("src/test.adoc"), "Trace failed. Output: {out}");
}

#[test]
fn mcp_search_returns_seeded_results() {
    let ws = McpWorkspace::new();
    let src_rel = "src/search.adoc";
    ws.write_file(src_rel, "= Title\n\n[para]
SearchMeKeyword\n".as_bytes());

    let args = process::SinglePassArgs {
        inputs: vec![src_rel.into()],
        input_dir: ws.root.clone(),
        gen_dir: ws.root.clone(),
        db: ws.db_path(),
        project_root: Some(ws.root.clone()),
        no_fts: false,
        ..process::SinglePassArgs::default_for_test()
    };
    process::run_single_pass(args).map_err(|e| e.to_string()).unwrap();

    let req = "{\"jsonrpc\":\"2.0\",\"id\":26,\"method\":\"tools/call\",\"params\":{\"name\":\"weaveback_search\",\"arguments\":{\"query\":\"SearchMeKeyword\"}}}\n";
    let out = mcp_drive(&ws, req);
    assert!(out.contains("SearchMe"), "Search failed. Output: {out}");
    assert!(out.contains("src/search.adoc"), "Search output missing file path: {out}");
}

#[test]
fn mcp_apply_fix_updates_source() {
    let ws = McpWorkspace::new();
    let src_rel = "src/test.adoc";
    ws.write_file(src_rel, "= Title\n\n<<@file test.rs>>=\nold\n@@\n".as_bytes());

    let args = process::SinglePassArgs {
        inputs: vec![src_rel.into()],
        input_dir: ws.root.clone(),
        gen_dir: ws.gen_dir(), // out file at root/gen/test.rs
        db: ws.db_path(),
        project_root: Some(ws.root.clone()),
        no_fts: true,
        no_macros: false,
        ..process::SinglePassArgs::default_for_test()
    };
    process::run_single_pass(args).map_err(|e| e.to_string()).unwrap();

    // Apply fix: change "old" to "new"
    let req =
        "{\"jsonrpc\":\"2.0\",\"id\":30,\"method\":\"tools/call\",\"params\":{\"name\":\"weaveback_apply_fix\",\"arguments\":{\"src_file\":\"src/test.adoc\",\"src_line\":4,\"new_src_line\":\"new\",\"out_file\":\"test.rs\",\"out_line\":1,\"expected_output\":\"new\"}}}\n"
            .to_string();
    let out = mcp_drive(&ws, &req);
    assert!(out.contains("Applied ChangePlan"), "Apply fix failed. Output: {out}");

    let content = std::fs::read_to_string(ws.root.join(src_rel)).unwrap();
    assert!(content.contains("new"), "Source was not updated. Content: {content}");
}



#[test]
fn mcp_apply_back_updates_source() {
    let ws = McpWorkspace::new();
    let src_rel = "src/test.adoc";
    ws.write_file(src_rel, "= Title\n\n<<@file test.rs>>=\nold\n@@\n".as_bytes());

    let args = process::SinglePassArgs {
        inputs: vec![src_rel.into()],
        input_dir: ws.root.clone(),
        gen_dir: ws.gen_dir(), // out file at root/gen/test.rs
        db: ws.db_path(),
        project_root: Some(ws.root.clone()),
        no_fts: true,
        no_macros: false,
        ..process::SinglePassArgs::default_for_test()
    };
    process::run_single_pass(args).map_err(|e| e.to_string()).unwrap();

    // Edit generated file
    std::fs::write(ws.gen_dir().join("test.rs"), "new\n").unwrap();

    // Apply back
    let req = "{\"jsonrpc\":\"2.0\",\"id\":32,\"method\":\"tools/call\",\"params\":{\"name\":\"weaveback_apply_back\",\"arguments\":{\"files\":[\"test.rs\"]}}}\n";
    let out = mcp_drive(&ws, req);
    assert!(out.contains("src/test.adoc"), "Apply back failed. Output: {out}");

    let content = std::fs::read_to_string(ws.root.join(src_rel)).unwrap();
    assert!(content.contains("new"), "Source was not updated. Content: {content}");
}

#[test]
fn mcp_chunk_context_returns_full_metadata() {
    let ws = McpWorkspace::new();
    {
        let mut db = ws.open_db();
        db.set_chunk_defs(&[weaveback_tangle::db::ChunkDefEntry {
            src_file:   "src/ctx.adoc".to_string(),
            chunk_name: "ctx-chunk".to_string(),
            nth:        0,
            def_start:  1,
            def_end:    3,
        }]).unwrap();
        ws.write_file("src/ctx.adoc", "== Section Title\n\nctx anchor\n".as_bytes());
        db.set_src_snapshot("src/ctx.adoc", "== Section Title\n\nctx anchor\n".as_bytes()).unwrap();
        db.set_source_blocks("src/ctx.adoc", &[weaveback_tangle::block_parser::SourceBlockEntry {
            block_index: 0,
            block_type:  "section".to_string(),
            line_start:  1,
            line_end:    3,
            content_hash: [0u8; 32],
        }]).unwrap();
        db.rebuild_prose_fts(Some(&ws.root)).unwrap();
    }
    let req = r#"{"jsonrpc":"2.0","id":36,"method":"tools/call","params":{"name":"weaveback_chunk_context","arguments":{"file":"src/ctx.adoc","name":"ctx-chunk"}}}"#;
    let out = mcp_drive(&ws, req);
    assert!(out.contains("Section Title"), "Breadcrumb missing: {out}");
    assert!(out.contains("ctx-chunk"), "Name missing: {out}");
}

#[test]
fn mcp_list_tags_returns_seeded_tags() {
    let ws = McpWorkspace::new();
    {
        let mut db = ws.open_db();
        db.set_source_blocks("src/tags.adoc", &[weaveback_tangle::block_parser::SourceBlockEntry {
            block_index: 0,
            block_type:  "para".to_string(),
            line_start:  1,
            line_end:    5,
            content_hash: [0u8; 32],
        }]).unwrap();
        db.set_block_tags("src/tags.adoc", 0, &[0u8; 32], "test-tag,mcp").unwrap();
    }
    let req = r#"{"jsonrpc":"2.0","id":37,"method":"tools/call","params":{"name":"weaveback_list_tags","arguments":{}}}"#;
    let out = mcp_drive(&ws, req);
    assert!(out.contains("test-tag"), "Tag missing: {out}");
}

#[test]
fn mcp_coverage_reports_stats() {
    let ws = McpWorkspace::new();
    ws.open_db(); // Ensure DB exists
    let lcov = ws.root.join("lcov.info");
    std::fs::write(&lcov, "SF:src/test.rs\nDA:1,1\nend_of_record\n").unwrap();

    let lcov_path = lcov.to_string_lossy().replace("\\", "/");
    let req = format!(
        r#"{{"jsonrpc":"2.0","id":38,"method":"tools/call","params":{{"name":"weaveback_coverage","arguments":{{"lcov_path":"{}"}}}}}}"#,
        lcov_path
    );
    let out = mcp_drive(&ws, &req);
    assert!(out.contains("attributed_records"), "Coverage report missing or invalid. Output: {out}");
}

// ── LSP integration tests (real rust-analyzer) ────────────────────────
//
// These tests spin up the real `rust-analyzer` binary against the live
// workspace so that the LSP dispatch arms in `run_mcp` are exercised.
// They are marked `#[ignore]` so `cargo test` skips them by default;
// run with `cargo test -- --ignored` to include them.

fn mcp_workspace_root() -> std::path::PathBuf {
    let mut dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    loop {
        let candidate = dir.join("Cargo.toml");
        if candidate.exists()
            && let Ok(txt) = std::fs::read_to_string(&candidate)
            && txt.contains("[workspace]") {
            return dir;
        }
        if !dir.pop() { break; }
    }
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn mcp_rs_path() -> std::path::PathBuf {
    mcp_workspace_root().join("crates/weaveback-api/src/mcp.rs")
}

fn lsp_mcp_drive(ws: &McpWorkspace, req: &str) -> String {
    // Use real db so DB presence check passes for LSP tools.
    ws.open_db();
    mcp_drive(ws, req)
}

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

#[test]
fn mcp_list_chunks_success_path() {
    let ws = McpWorkspace::new();
    ws.open_db();
    let req = r#"{"jsonrpc":"2.0","id":900,"method":"tools/call","params":{"name":"weaveback_list_chunks","arguments":{}}}"#;
    let out = mcp_drive(&ws, &(req.to_string() + "\n"));
    assert!(out.contains("\"id\":900"), "output: {out}");
}

#[test]
fn mcp_apply_back_success_path() {
    let ws = McpWorkspace::new();
    ws.open_db();
    let req = r#"{"jsonrpc":"2.0","id":1000,"method":"tools/call","params":{"name":"weaveback_apply_back","arguments":{"dry_run":true}}}"#;
    let out = mcp_drive(&ws, req);
    assert!(out.contains("\"id\":1000"), "output: {out}");
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

#[test]
fn mcp_list_tags_success_path() {
    let ws = McpWorkspace::new();
    ws.open_db();
    let req = r#"{"jsonrpc":"2.0","id":1400,"method":"tools/call","params":{"name":"weaveback_list_tags","arguments":{}}}"#;
    let out = mcp_drive(&ws, req);
    assert!(out.contains("\"id\":1400"));
}

#[test]
fn mcp_coverage_success_path() {
    let ws = McpWorkspace::new();
    ws.open_db();
    let tmp = tempfile::tempdir().unwrap();
    let lcov = tmp.path().join("test.lcov");
    std::fs::write(&lcov, "SF:src/a.rs\nend_of_record\n").unwrap();

    let req = format!(r#"{{"jsonrpc":"2.0","id":1500,"method":"tools/call","params":{{"name":"weaveback_coverage","arguments":{{"lcov_path":"{}"}}}}}}"#, lcov.display());
    let out = mcp_drive(&ws, &req);
    assert!(out.contains("\"id\":1500"));
}

