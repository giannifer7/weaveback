// crates/azadi/tests/test_mcp.rs
//
// Integration tests for the MCP server (azadi mcp).
//
// The server reads newline-delimited JSON-RPC from stdin and writes responses
// to stdout.  Tests pipe messages in, close stdin, and parse the response stream.
//
// Exercises: initialize handshake, tools/list, azadi_trace, azadi_apply_back,
// azadi_apply_fix, and error paths.

use assert_cmd::cargo::cargo_bin;
use serde_json::{json, Value};
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use tempfile::TempDir;

// ── helpers ───────────────────────────────────────────────────────────────────

fn write_file(dir: &Path, rel: &str, content: &str) {
    let path = dir.join(rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

/// Run `azadi --gen . <driver>` in `dir` to populate azadi.db.
fn build(dir: &Path, driver: &str) {
    write_file(dir, "driver.md", driver);
    let status = Command::new(cargo_bin("azadi"))
        .args(["--gen", ".", "driver.md"])
        .current_dir(dir)
        .status()
        .unwrap();
    assert!(status.success(), "azadi build failed");
}

/// Send a slice of JSON-RPC messages to `azadi mcp` (one per line),
/// collect all response lines, and return them parsed.
///
/// Passes `--gen .` and `--db azadi.db` which are the defaults but spelled
/// out explicitly so the server finds the db written by `build()`.
fn mcp_exchange(dir: &Path, messages: &[Value]) -> Vec<Value> {
    let mut input = String::new();
    for msg in messages {
        input.push_str(&serde_json::to_string(msg).unwrap());
        input.push('\n');
    }

    let mut child = Command::new(cargo_bin("azadi"))
        .args(["--gen", ".", "--db", "azadi.db", "mcp"])
        .current_dir(dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();

    child.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();

    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "azadi mcp exited non-zero");

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("bad MCP JSON: {e}\nline: {l}")))
        .collect()
}

/// Standard opening handshake: initialize + initialized notification.
fn handshake() -> Vec<Value> {
    vec![
        json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": { "name": "test", "version": "0" }
            }
        }),
        json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
    ]
}

/// Return the `result` of the response whose `id` matches, panicking if absent.
fn result_for(responses: &[Value], id: u64) -> &Value {
    responses.iter()
        .find(|r| r.get("id").and_then(|v| v.as_u64()) == Some(id))
        .unwrap_or_else(|| panic!("no response with id={id} in {responses:?}"))
        .get("result")
        .unwrap_or_else(|| panic!("response for id={id} has no 'result' field"))
}

/// Return the text content of a tool-call response.
fn text_content(result: &Value) -> String {
    result["content"][0]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("no text content in {result}"))
        .to_string()
}

// ── fixture ───────────────────────────────────────────────────────────────────

const DRIVER: &str = r#"%def(cfg_int, field, toml_key, default_val, %{
# <[defaults]>=
result.%(field) = %(default_val)
# @
# <[toml reads]>=
cfg.%(field) = t.getInt("%(toml_key)", cfg.%(field))
# @
%})

%set(module_name, config)

# <[@file config.nim]>=
# <[defaults]>
# <[toml reads]>
# @

# <[@file header.nim]>=
// version: 1.0
# module: %(module_name)
# @

%cfg_int(field=batchSize, toml_key=db_batch_size, default_val=300)
%cfg_int(field=prefetch,  toml_key=prefetch_ahead, default_val=50)
"#;

// ── initialize / tools/list ───────────────────────────────────────────────────

#[test]
fn test_mcp_initialize() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    build(&root, DRIVER);

    let mut msgs = handshake();
    msgs.push(json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }));

    let responses = mcp_exchange(&root, &msgs);

    // initialize response
    let init = result_for(&responses, 1);
    assert_eq!(init["protocolVersion"], "2024-11-05");
    assert_eq!(init["serverInfo"]["name"], "Azadi Trace Server");

    // tools/list response
    let tools = result_for(&responses, 2)["tools"]
        .as_array()
        .expect("tools must be an array");
    let names: Vec<&str> = tools.iter()
        .filter_map(|t| t["name"].as_str())
        .collect();
    assert!(names.contains(&"azadi_trace"),       "missing azadi_trace: {names:?}");
    assert!(names.contains(&"azadi_apply_back"),  "missing azadi_apply_back: {names:?}");
    assert!(names.contains(&"azadi_apply_fix"),   "missing azadi_apply_fix: {names:?}");
}

// ── azadi_trace via MCP ───────────────────────────────────────────────────────

#[test]
fn test_mcp_trace_literal() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    build(&root, DRIVER);

    let mut msgs = handshake();
    msgs.push(json!({
        "jsonrpc": "2.0", "id": 2, "method": "tools/call",
        "params": {
            "name": "azadi_trace",
            "arguments": { "out_file": "header.nim", "out_line": 1 }
        }
    }));

    let responses = mcp_exchange(&root, &msgs);
    let text = text_content(result_for(&responses, 2));
    let j: Value = serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("trace response is not JSON: {e}\n{text}"));

    assert_eq!(j["kind"], "Literal", "header.nim:1 is a literal line: {j}");
    assert!(j["src_file"].as_str().unwrap().contains("driver.md"));
}

#[test]
fn test_mcp_trace_macro_arg_with_col() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    build(&root, DRIVER);

    // config.nim line 1 = "result.batchSize = 300"
    // col 8 = 'b' of batchSize (1-indexed char position) → MacroArg(field)
    let mut msgs = handshake();
    msgs.push(json!({
        "jsonrpc": "2.0", "id": 2, "method": "tools/call",
        "params": {
            "name": "azadi_trace",
            "arguments": { "out_file": "config.nim", "out_line": 1, "out_col": 8 }
        }
    }));

    let responses = mcp_exchange(&root, &msgs);
    let text = text_content(result_for(&responses, 2));
    let j: Value = serde_json::from_str(&text).unwrap();

    assert_eq!(j["kind"], "MacroArg");
    assert_eq!(j["macro_name"], "cfg_int");
    assert_eq!(j["param_name"], "field");
}

#[test]
fn test_mcp_trace_no_db() {
    // No azadi run — database does not exist.
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().canonicalize().unwrap();

    let mut msgs = handshake();
    msgs.push(json!({
        "jsonrpc": "2.0", "id": 2, "method": "tools/call",
        "params": {
            "name": "azadi_trace",
            "arguments": { "out_file": "any.txt", "out_line": 1 }
        }
    }));

    let responses = mcp_exchange(&root, &msgs);
    let result = result_for(&responses, 2);
    assert_eq!(result["isError"], true, "should report error when db absent: {result}");
    let msg = result["content"][0]["text"].as_str().unwrap_or("");
    assert!(msg.contains("Database not found") || msg.contains("not found"),
        "error message should mention missing db: {msg}");
}

// ── azadi_apply_back via MCP ──────────────────────────────────────────────────

#[test]
fn test_mcp_apply_back_no_changes() {
    // gen/ files untouched → "No modified gen/ files found."
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    build(&root, DRIVER);

    let mut msgs = handshake();
    msgs.push(json!({
        "jsonrpc": "2.0", "id": 2, "method": "tools/call",
        "params": { "name": "azadi_apply_back", "arguments": {} }
    }));

    let responses = mcp_exchange(&root, &msgs);
    let text = text_content(result_for(&responses, 2));
    assert!(text.contains("No modified"), "expected 'No modified' in: {text}");
}

#[test]
fn test_mcp_apply_back_dry_run() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    build(&root, DRIVER);

    // Edit a gen/ file
    let hdr = root.join("header.nim");
    let original = fs::read_to_string(&hdr).unwrap();
    fs::write(&hdr, original.replace("// version: 1.0", "// version: 9.9")).unwrap();

    let mut msgs = handshake();
    msgs.push(json!({
        "jsonrpc": "2.0", "id": 2, "method": "tools/call",
        "params": {
            "name": "azadi_apply_back",
            "arguments": { "dry_run": true }
        }
    }));

    let responses = mcp_exchange(&root, &msgs);
    let text = text_content(result_for(&responses, 2));

    // dry-run must mention the file being processed and dry-run markers
    assert!(text.contains("header.nim") || text.contains("Processing"),
        "expected file name in dry-run report: {text}");
    assert!(text.contains("dry-run"),
        "expected '[dry-run]' markers in: {text}");

    // Source must NOT have been modified
    let after_src = fs::read_to_string(root.join("driver.md")).unwrap();
    assert!(after_src.contains("// version: 1.0"),
        "dry-run must not write to driver.md: {after_src}");
}

#[test]
fn test_mcp_apply_back_literal_patch() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    build(&root, DRIVER);

    // Simulate an IDE editing the literal version comment
    let hdr = root.join("header.nim");
    let original = fs::read_to_string(&hdr).unwrap();
    fs::write(&hdr, original.replace("// version: 1.0", "// version: 3.0")).unwrap();

    let mut msgs = handshake();
    msgs.push(json!({
        "jsonrpc": "2.0", "id": 2, "method": "tools/call",
        "params": { "name": "azadi_apply_back", "arguments": {} }
    }));

    let responses = mcp_exchange(&root, &msgs);
    let text = text_content(result_for(&responses, 2));

    // Report should mention a patch was applied
    assert!(text.contains("patched") || text.contains("Processing"),
        "expected patch report in: {text}");

    // Literate source must be updated
    let src = fs::read_to_string(root.join("driver.md")).unwrap();
    assert!(src.contains("// version: 3.0"),
        "driver.md should contain '// version: 3.0' after apply-back: {src}");
    assert!(!src.contains("// version: 1.0"),
        "old version should be gone from driver.md: {src}");
}

#[test]
fn test_mcp_apply_back_specific_file() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    build(&root, DRIVER);

    // Edit both gen/ files
    let hdr = root.join("header.nim");
    let hdr_orig = fs::read_to_string(&hdr).unwrap();
    fs::write(&hdr, hdr_orig.replace("// version: 1.0", "// version: 5.0")).unwrap();

    let cfg = root.join("config.nim");
    let cfg_orig = fs::read_to_string(&cfg).unwrap();
    fs::write(&cfg, cfg_orig.replace("result.batchSize = 300", "result.batchSize = 999")).unwrap();

    // Only process header.nim
    let mut msgs = handshake();
    msgs.push(json!({
        "jsonrpc": "2.0", "id": 2, "method": "tools/call",
        "params": {
            "name": "azadi_apply_back",
            "arguments": { "files": ["header.nim"] }
        }
    }));

    let responses = mcp_exchange(&root, &msgs);
    let text = text_content(result_for(&responses, 2));

    // header.nim patched
    let src = fs::read_to_string(root.join("driver.md")).unwrap();
    assert!(src.contains("// version: 5.0"),
        "version update should be in driver.md: {src}");

    // config.nim edit not mentioned / not patched (we only asked for header.nim)
    assert!(!text.contains("config.nim"),
        "report should not mention config.nim when only header.nim requested: {text}");
}

// ── azadi_apply_fix via MCP ───────────────────────────────────────────────────

/// Minimal fixture: single literal output line.
const SIMPLE_DRIVER: &str = "# <[@file out.txt]>=\nhello world\n# @\n";

#[test]
fn test_mcp_apply_fix_oracle_verified() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    build(&root, SIMPLE_DRIVER);

    // out.txt line 1 = "hello world" — literal from the @file chunk at driver.md line 2.
    // Change it to "hello earth". The intermediate line is also "hello world"
    // (no macro expansion), so expected_output = "hello earth".
    let mut msgs = handshake();
    msgs.push(json!({
        "jsonrpc": "2.0", "id": 2, "method": "tools/call",
        "params": {
            "name": "azadi_apply_fix",
            "arguments": {
                "src_file":        root.join("driver.md").to_str().unwrap(),
                "src_line":        2,
                "new_src_line":    "hello earth",
                "out_file":        "out.txt",
                "out_line":        1,
                "expected_output": "hello earth"
            }
        }
    }));

    let responses = mcp_exchange(&root, &msgs);
    let result = result_for(&responses, 2);
    assert_ne!(result.get("isError"), Some(&json!(true)),
        "apply_fix should succeed: {result}");

    let text = text_content(result);
    assert!(text.contains("Applied") || text.contains("Oracle"),
        "unexpected apply_fix response: {text}");

    let src = fs::read_to_string(root.join("driver.md")).unwrap();
    assert!(src.contains("hello earth"), "driver.md not updated: {src}");
    assert!(!src.contains("hello world"), "old value should be gone: {src}");
}

#[test]
fn test_mcp_apply_fix_trace_then_fix() {
    // Realistic workflow: first trace a generated line, then use apply_fix
    // with the src_file/src_line returned by trace.
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    build(&root, DRIVER);

    // Step 1: trace header.nim:1 to find its literate source location.
    let mut msgs = handshake();
    msgs.push(json!({
        "jsonrpc": "2.0", "id": 2, "method": "tools/call",
        "params": {
            "name": "azadi_trace",
            "arguments": { "out_file": "header.nim", "out_line": 1 }
        }
    }));
    let responses = mcp_exchange(&root, &msgs);
    let trace_text = text_content(result_for(&responses, 2));
    let trace: Value = serde_json::from_str(&trace_text).unwrap();

    assert_eq!(trace["kind"], "Literal");
    let src_file = trace["src_file"].as_str().unwrap().to_string();
    let src_line = trace["src_line"].as_u64().unwrap();

    // Step 2: apply_fix using the trace-derived location.
    let mut msgs2 = handshake();
    msgs2.push(json!({
        "jsonrpc": "2.0", "id": 2, "method": "tools/call",
        "params": {
            "name": "azadi_apply_fix",
            "arguments": {
                "src_file":        src_file,
                "src_line":        src_line,
                "new_src_line":    "// version: 4.0",
                "out_file":        "header.nim",
                "out_line":        1,
                "expected_output": "// version: 4.0"
            }
        }
    }));

    let responses2 = mcp_exchange(&root, &msgs2);
    let result = result_for(&responses2, 2);
    assert_ne!(result.get("isError"), Some(&json!(true)),
        "apply_fix should succeed: {result}");

    let src = fs::read_to_string(root.join("driver.md")).unwrap();
    assert!(src.contains("// version: 4.0"), "driver.md not updated: {src}");
    assert!(!src.contains("// version: 1.0"), "old value should be gone: {src}");
}

#[test]
fn test_mcp_apply_fix_oracle_rejects_wrong_expected() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    build(&root, DRIVER);

    // Wrong expected_output — oracle should reject and leave source unchanged.
    let mut msgs = handshake();
    msgs.push(json!({
        "jsonrpc": "2.0", "id": 2, "method": "tools/call",
        "params": {
            "name": "azadi_apply_fix",
            "arguments": {
                "src_file":        root.join("driver.md").to_str().unwrap(),
                "src_line":        18,
                "new_src_line":    "// version: 99.0",
                "out_file":        "header.nim",
                "out_line":        1,
                "expected_output": "// version: WRONG"
            }
        }
    }));

    let responses = mcp_exchange(&root, &msgs);
    let result = result_for(&responses, 2);
    assert_eq!(result["isError"], true,
        "oracle should reject wrong expected_output: {result}");

    let src = fs::read_to_string(root.join("driver.md")).unwrap();
    assert!(src.contains("// version: 1.0"),
        "source must be untouched after oracle rejection: {src}");
}

// ── unknown tool ──────────────────────────────────────────────────────────────

#[test]
fn test_mcp_unknown_tool() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    build(&root, DRIVER);

    let mut msgs = handshake();
    msgs.push(json!({
        "jsonrpc": "2.0", "id": 2, "method": "tools/call",
        "params": { "name": "no_such_tool", "arguments": {} }
    }));

    let responses = mcp_exchange(&root, &msgs);
    let result = result_for(&responses, 2);
    assert_eq!(result["isError"], true, "unknown tool should return isError: {result}");
}
