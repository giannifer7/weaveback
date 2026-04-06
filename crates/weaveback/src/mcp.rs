use crate::apply_back::{self, ApplyBackOptions};
use crate::lookup;
use weaveback_agent_core::{
    ChangePlan, ChangeTarget, PlannedEdit, Workspace as AgentWorkspace,
    WorkspaceConfig as AgentWorkspaceConfig,
};
use weaveback_agent_core::change_plan::OutputAnchor;
use weaveback_macro::evaluator::EvalConfig;
use weaveback_tangle::db::WeavebackDb;
use weaveback_lsp::LspClient;
use weaveback_core::PathResolver;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{self, BufRead};
use std::path::PathBuf;

fn get_or_spawn_lsp<'a>(
    clients: &'a mut HashMap<String, LspClient>,
    ext: &str,
) -> Result<&'a mut LspClient, String> {
    let (lsp_cmd, lsp_lang) = weaveback_lsp::get_lsp_config(ext)
        .ok_or_else(|| format!("unsupported file extension: .{}", ext))?;

    let needs_spawn = match clients.get_mut(&lsp_lang) {
        Some(c) => !c.is_alive(),
        None => true,
    };

    if needs_spawn {
        let project_root = std::env::current_dir().map_err(|e| e.to_string())?;
        let mut c = LspClient::spawn(&lsp_cmd, &[], &project_root, lsp_lang.clone())
            .map_err(|e| format!("failed to spawn LSP '{}': {e}", lsp_cmd))?;
        c.initialize(&project_root)
            .map_err(|e| format!("failed to initialize LSP '{}': {e}", lsp_cmd))?;
        clients.insert(lsp_lang.clone(), c);
    }
    Ok(clients.get_mut(&lsp_lang).unwrap())
}

fn tools_list_result() -> Value {
    json!({
        "tools": [
            {
                "name": "weaveback_trace",
                "description": "Trace an output file line back to its original literate source. Returns src_file/src_line/src_col/kind. MacroArg spans include macro_name/param_name. MacroBody spans include macro_name and a def_locations array (all %def call sites). VarBinding spans include var_name and a set_locations array (all %set call sites). Use --col for sub-line token precision.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "out_file": { "type": "string", "description": "Path to the generated file" },
                        "out_line": { "type": "integer", "description": "1-indexed line number in the generated file" },
                        "out_col":  { "type": "integer", "description": "1-indexed character position within the output line (default 1). Use to pinpoint a specific token." }
                    },
                    "required": ["out_file", "out_line"]
                }
            },
            {
                "name": "weaveback_apply_back",
                "description": "Bulk baseline-reconciliation tool: propagate edits already made directly in gen/ files back to the literate source. Use this only when gen/ files have been edited by hand and you need to reconcile the baseline. For intentional fixes where you know what the source should look like, prefer weaveback_apply_fix (oracle-verified, surgical, no full rebuild needed). weaveback_apply_back diffs each modified gen/ file against its stored baseline, traces each changed line to its noweb+macro origin, and patches the literate source. Returns a report of what was patched, skipped, or needs manual attention.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "files":   { "type": "array", "items": { "type": "string" }, "description": "Relative paths within gen/ to process (default: all modified files)" },
                        "dry_run": { "type": "boolean", "description": "Show what would change without writing (default: false)" }
                    },
                    "required": []
                }
            },
            {
                "name": "weaveback_apply_fix",
                "description": "**Preferred tool for all literate-source edits.** Apply a source edit (single line or multi-line range) and oracle-verify it produces the expected output before writing. Workflow: (1) use weaveback_trace to find src_file/src_line, (2) read the source, (3) call this tool with the replacement and the expected output line. The macro expander re-runs as an oracle — the file is written only if the expected output is produced, making the edit safe to apply without a full rebuild. Use apply_back only when you have already edited gen/ files directly and need to reconcile the baseline.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "src_file":        { "type": "string",  "description": "Absolute path of the literate source file to edit" },
                        "src_line":        { "type": "integer", "description": "1-indexed first line to replace in src_file" },
                        "src_line_end":    { "type": "integer", "description": "1-indexed last line of the replacement range (inclusive, defaults to src_line for single-line edits)" },
                        "new_src_line":    { "type": "string",  "description": "Replacement text when replacing a single line (without trailing newline)" },
                        "new_src_lines":   { "type": "array", "items": { "type": "string" }, "description": "Replacement lines for multi-line edits (each element is one line without trailing newline); overrides new_src_line when present" },
                        "out_file":        { "type": "string",  "description": "Generated file path (used for oracle lookup)" },
                        "out_line":        { "type": "integer", "description": "1-indexed line in the generated file (oracle check point)" },
                        "expected_output": { "type": "string",  "description": "The exact content of out_line expected after the fix (indent-stripped); oracle rejects the edit if this does not match" }
                    },
                    "required": ["src_file", "src_line", "out_file", "out_line", "expected_output"]
                }
            },
            {
                "name": "weaveback_chunk_context",
                "description": "Return full context for a named noweb chunk: its body, the AsciiDoc section title breadcrumb, the full prose of the enclosing section (paragraphs, admonitions, design notes), bodies of all direct dependencies, reverse-dep names, output files, and recent git log entries. Use this before editing or reasoning about a chunk.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "file": { "type": "string", "description": "Source file path (relative to project root), e.g. 'crates/weaveback/src/serve.adoc'" },
                        "name": { "type": "string", "description": "Chunk name as it appears in the <<name>>= marker" },
                        "nth":  { "type": "integer", "description": "0-based index for chunks defined multiple times (default 0)" }
                    },
                    "required": ["file", "name"]
                }
            },
            {
                "name": "weaveback_list_chunks",
                "description": "List all chunk definitions in the project, optionally filtered to a single source file. Returns an array of { file, name, nth, def_start, def_end } objects.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "file": { "type": "string", "description": "Source file to filter to (optional; omit for all files)" }
                    },
                    "required": []
                }
            },
            {
                "name": "weaveback_find_chunk",
                "description": "Find which source file(s) define a given chunk name. Returns an array of { file, nth, def_start, def_end } objects.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "file": { "type": "string", "description": "Chunk name to look up" }
                    },
                    "required": ["name"]
                }
            },
            {
                "name": "weaveback_lsp_definition",
                "description": "Find the definition of a symbol at a given position in a generated file, and map it back to its original literate source. Requires rust-analyzer.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "out_file": { "type": "string", "description": "Path to the generated file" },
                        "line":     { "type": "integer", "description": "1-indexed line number" },
                        "col":      { "type": "integer", "description": "1-indexed character position" }
                    },
                    "required": ["out_file", "line", "col"]
                }
            },
            {
                "name": "weaveback_lsp_references",
                "description": "Find all references to a symbol at a given position in a generated file, and map them back to their original literate sources. Requires rust-analyzer.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "out_file": { "type": "string", "description": "Path to the generated file" },
                        "line":     { "type": "integer", "description": "1-indexed line number" },
                        "col":      { "type": "integer", "description": "1-indexed character position" }
                    },
                    "required": ["out_file", "line", "col"]
                }
            },
            {
                "name": "weaveback_lsp_hover",
                "description": "Get type information and documentation for a symbol at a given position in a generated file, mapped back to literate source. Requires rust-analyzer.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "out_file": { "type": "string", "description": "Path to the generated file" },
                        "line":     { "type": "integer", "description": "1-indexed line number" },
                        "col":      { "type": "integer", "description": "1-indexed character position" }
                    },
                    "required": ["out_file", "line", "col"]
                }
            },
            {
                "name": "weaveback_lsp_diagnostics",
                "description": "Get current compiler errors/warnings for a generated file, mapped back to original literate source lines.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "out_file": { "type": "string", "description": "Path to the generated file" }
                    },
                    "required": ["out_file"]
                }
            },
            {
                "name": "weaveback_lsp_symbols",
                "description": "List all semantic symbols (functions, structs, etc.) in a generated file, with their original literate source locations.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "out_file": { "type": "string", "description": "Path to the generated file" }
                    },
                    "required": ["out_file"]
                }
            },
            {
                "name": "weaveback_search",
                "description": "Hybrid search over the prose in all literate source files. FTS5 and tags are always used; if prose embeddings were generated during tangle, semantic reranking is also applied. Returns ranked excerpts with file path, line range, tags, score, and contributing channels. Use this to discover which chunks or sections are relevant to a concept before calling weaveback_chunk_context. Supports FTS5 query syntax: AND, OR, NOT, phrase \"...\", prefix foo*.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "Search terms (FTS5 syntax)" },
                        "limit": { "type": "integer", "description": "Maximum results to return (default 10)" }
                    },
                    "required": ["query"]
                }
            },
            {
                "name": "weaveback_list_tags",
                "description": "List all LLM-generated tags for prose blocks in the project. Returns each block's source file, line, block type, and comma-separated tags. Optionally filter to a single source file. Use this to explore the semantic landscape of the project or to find all blocks tagged with a given concept.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "file": { "type": "string", "description": "Optional: filter to this source file (plain relative path, e.g. crates/weaveback-tangle/src/db.adoc)" }
                    }
                }
            }
        ]
    })
}

fn build_apply_fix_plan(
    src_file: &str,
    src_line_1: usize,
    src_line_end_1: usize,
    new_lines: Vec<String>,
    out_file: &str,
    out_line_1: u32,
    expected: &str,
) -> ChangePlan {
    ChangePlan {
        plan_id: "mcp-apply-fix".to_string(),
        goal: "Apply a single oracle-verified fix".to_string(),
        constraints: Vec::new(),
        edits: vec![PlannedEdit {
            edit_id: "edit-1".to_string(),
            rationale: "MCP weaveback_apply_fix request".to_string(),
            target: ChangeTarget {
                src_file: src_file.to_string(),
                src_line: src_line_1,
                src_line_end: src_line_end_1,
            },
            new_src_lines: new_lines,
            anchor: OutputAnchor {
                out_file: out_file.to_string(),
                out_line: out_line_1,
                expected_output: expected.to_string(),
            },
        }],
    }
}

fn response_payload(id: Option<Value>, result: Value) -> Value {
    let mut resp = json!({ "jsonrpc": "2.0" });
    if let Some(id) = id {
        resp.as_object_mut().unwrap().insert("id".to_string(), id);
        resp.as_object_mut().unwrap().insert("result".to_string(), result);
    }
    resp
}

fn text_result(text: &str) -> Value {
    json!({
        "content": [{ "type": "text", "text": text }]
    })
}

fn error_result(msg: &str) -> Value {
    json!({
        "isError": true,
        "content": [{ "type": "text", "text": msg }]
    })
}

fn chunk_context_value(ctx: weaveback_agent_core::ChunkContext) -> Value {
    json!({
        "file": ctx.file,
        "name": ctx.name,
        "nth": ctx.nth,
        "body": ctx.body,
        "section_title_chain": ctx.section_breadcrumb,
        "section_prose": ctx.prose,
        "dependencies": ctx.direct_dependencies,
        "output_files": ctx.outputs,
    })
}

fn search_results_value(results: &[weaveback_agent_core::SearchHit]) -> Value {
    Value::Array(
        results
            .iter()
            .map(|r| {
                let mut obj = json!({
                    "src_file":   r.src_file,
                    "block_type": r.block_type,
                    "line_start": r.line_start,
                    "line_end":   r.line_end,
                    "snippet":    r.snippet,
                    "score":      r.score,
                    "channels":   r.channels,
                });
                if !r.tags.is_empty() {
                    obj["tags"] = json!(r.tags);
                }
                obj
            })
            .collect(),
    )
}

fn handle_apply_fix(
    input: &serde_json::Map<String, Value>,
    agent_session: &weaveback_agent_core::Session,
) -> Result<String, String> {
    let src_file = input.get("src_file").and_then(|v| v.as_str()).unwrap_or("");
    let src_line_1 = input.get("src_line").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
    let src_line_end_1 = input
        .get("src_line_end")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize)
        .unwrap_or(src_line_1);
    let new_lines: Vec<String> = if let Some(arr) = input.get("new_src_lines").and_then(|v| v.as_array()) {
        arr.iter().filter_map(|v| v.as_str().map(str::to_string)).collect()
    } else {
        let s = input.get("new_src_line").and_then(|v| v.as_str()).unwrap_or("");
        vec![s.to_string()]
    };
    let out_file = input.get("out_file").and_then(|v| v.as_str()).unwrap_or("");
    let out_line_1 = input.get("out_line").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
    let expected = input
        .get("expected_output")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if src_line_1 == 0 {
        return Err("src_line must be >= 1".to_string());
    }
    if src_line_end_1 < src_line_1 {
        return Err("src_line_end must be >= src_line".to_string());
    }

    let plan = build_apply_fix_plan(
        src_file,
        src_line_1,
        src_line_end_1,
        new_lines,
        out_file,
        out_line_1,
        expected,
    );
    match agent_session.apply_change_plan(&plan) {
        Ok(result) if result.applied => Ok(format!(
            "Applied ChangePlan {} with edits: {}",
            result.plan_id,
            result.applied_edit_ids.join(", ")
        )),
        Ok(result) => Err(format!(
            "Failed ChangePlan {}. Failed edits: {}",
            result.plan_id,
            result.failed_edit_ids.join(", ")
        )),
        Err(e) => Err(e),
    }
}

fn handle_chunk_context(
    input: &serde_json::Map<String, Value>,
    agent_session: &weaveback_agent_core::Session,
) -> Result<String, String> {
    let file = input.get("file").and_then(|v| v.as_str()).unwrap_or("");
    let name = input.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let nth = input.get("nth").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
    if file.is_empty() || name.is_empty() {
        return Err("file and name are required".to_string());
    }
    let ctx = agent_session
        .chunk_context(file, name, nth)
        .map_err(|_| format!("Chunk not found: {}#{}[{}]", file, name, nth))?;
    serde_json::to_string_pretty(&chunk_context_value(ctx)).map_err(|e| e.to_string())
}

fn handle_search(
    input: Option<&serde_json::Map<String, Value>>,
    db_path: &std::path::Path,
    agent_session: &weaveback_agent_core::Session,
) -> Result<String, String> {
    let query = input
        .and_then(|v| v.get("query"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if query.is_empty() {
        return Err("query is required".to_string());
    }
    let limit = input
        .and_then(|v| v.get("limit"))
        .and_then(|v| v.as_u64())
        .unwrap_or(10) as usize;
    if !db_path.exists() {
        return Err("Database not found. Run weaveback on your source files first.".to_string());
    }
    let results = agent_session
        .search(query, limit)
        .map_err(|e| format!("Search error: {e}"))?;
    serde_json::to_string_pretty(&search_results_value(&results)).map_err(|e| e.to_string())
}

fn handle_list_chunks(
    file_filter: Option<&str>,
    db_path: &std::path::Path,
) -> Result<String, String> {
    if !db_path.exists() {
        return Err("Database not found. Run weaveback on your source files first.".to_string());
    }
    let db = WeavebackDb::open_read_only(db_path)
        .map_err(|e| format!("Database error: {e:?}"))?;
    let defs = db
        .list_chunk_defs(file_filter)
        .map_err(|e| format!("Query error: {e:?}"))?;
    let arr: Vec<Value> = defs
        .iter()
        .map(|d| {
            json!({
                "file":      d.src_file,
                "name":      d.chunk_name,
                "nth":       d.nth,
                "def_start": d.def_start,
                "def_end":   d.def_end,
            })
        })
        .collect();
    serde_json::to_string_pretty(&arr).map_err(|e| e.to_string())
}

fn handle_find_chunk(
    input: &serde_json::Map<String, Value>,
    db_path: &std::path::Path,
) -> Result<String, String> {
    let name = input.get("name").and_then(|v| v.as_str()).unwrap_or("");
    if name.is_empty() {
        return Err("name is required".to_string());
    }
    if !db_path.exists() {
        return Err("Database not found. Run weaveback on your source files first.".to_string());
    }
    let db = WeavebackDb::open_read_only(db_path)
        .map_err(|e| format!("Database error: {e:?}"))?;
    let defs = db
        .find_chunk_defs_by_name(name)
        .map_err(|e| format!("Query error: {e:?}"))?;
    let arr: Vec<Value> = defs
        .iter()
        .map(|d| {
            json!({
                "file":      d.src_file,
                "nth":       d.nth,
                "def_start": d.def_start,
                "def_end":   d.def_end,
            })
        })
        .collect();
    serde_json::to_string_pretty(&arr).map_err(|e| e.to_string())
}

fn handle_list_tags(
    file_filter: Option<&str>,
    db_path: &std::path::Path,
) -> Result<String, String> {
    if !db_path.exists() {
        return Err("Database not found. Run weaveback on your source files first.".to_string());
    }
    let db = WeavebackDb::open_read_only(db_path)
        .map_err(|e| format!("Database error: {e:?}"))?;
    let blocks = db
        .list_block_tags(file_filter)
        .map_err(|e| format!("Tag list error: {e:?}"))?;
    let arr: Vec<Value> = blocks
        .iter()
        .map(|b| {
            json!({
                "src_file":    b.src_file,
                "block_index": b.block_index,
                "block_type":  b.block_type,
                "line_start":  b.line_start,
                "tags":        b.tags,
            })
        })
        .collect();
    serde_json::to_string_pretty(&arr).map_err(|e| e.to_string())
}

pub fn run_mcp(db_path: PathBuf, gen_dir: PathBuf, eval_config: EvalConfig) -> Result<(), crate::Error> {
    let stdin = io::stdin();
    let mut lsp_clients: HashMap<String, LspClient> = HashMap::new();
    let agent_workspace = AgentWorkspace::open(AgentWorkspaceConfig {
        project_root: std::env::current_dir().unwrap_or_default(),
        db_path: db_path.clone(),
        gen_dir: gen_dir.clone(),
    });
    let agent_session = agent_workspace.session();
    let project_root = std::env::current_dir().unwrap_or_default();
    let resolver = PathResolver::new(project_root, gen_dir.clone());

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        if line.trim().is_empty() { continue; }

        let req: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let id = req.get("id").cloned();
        let method = req.get("method").and_then(|m| m.as_str()).unwrap_or("");

        match method {
            "initialize" => {
                send_response(id, json!({
                    "protocolVersion": "2024-11-05",
                    "capabilities": { "tools": {} },
                    "serverInfo": { "name": "Weaveback Trace Server", "version": "0.1.0" }
                }));
            }

            "tools/list" => {
                send_response(id, tools_list_result());
            }

            "tools/call" => {
                let params = req.get("params").and_then(|p| p.as_object());
                let tool_name = params.and_then(|p| p.get("name")).and_then(|n| n.as_str());
                let input = params.and_then(|p| p.get("arguments")).and_then(|a| a.as_object());

                match tool_name {
                    Some("weaveback_trace") => {
                        let Some(input) = input else {
                            send_error(id, "Missing arguments");
                            continue;
                        };
                        let out_file = input.get("out_file").and_then(|f| f.as_str()).unwrap_or("");
                        let out_line = input.get("out_line").and_then(|l| l.as_u64()).unwrap_or(0) as u32;
                        let out_col  = input.get("out_col") .and_then(|c| c.as_u64()).unwrap_or(0) as u32;

                        if !db_path.exists() {
                            send_error(id, "Database not found. Run weaveback on your source files first.");
                            continue;
                        }
                        match agent_session.trace(out_file, out_line, out_col) {
                            Ok(Some(res)) => {
                                let mut obj = serde_json::Map::new();
                                obj.insert("out_file".into(), json!(res.out_file));
                                obj.insert("out_line".into(), json!(res.out_line));
                                if let Some(v) = res.src_file { obj.insert("src_file".into(), json!(v)); }
                                if let Some(v) = res.src_line { obj.insert("src_line".into(), json!(v)); }
                                if let Some(v) = res.src_col { obj.insert("src_col".into(), json!(v)); }
                                if let Some(v) = res.kind { obj.insert("kind".into(), json!(v)); }
                                if let Some(v) = res.macro_name { obj.insert("macro_name".into(), json!(v)); }
                                if let Some(v) = res.param_name { obj.insert("param_name".into(), json!(v)); }
                                send_text(id, &serde_json::to_string(&Value::Object(obj)).unwrap())
                            }
                            Ok(None) => send_error(id, &format!("No mapping found for {}:{}", out_file, out_line)),
                            Err(e) => send_error(id, &format!("Lookup error: {e}")),
                        }
                    }

                    Some("weaveback_apply_back") => {
                        let input = input.cloned().unwrap_or_default();
                        let files: Vec<String> = input.get("files")
                            .and_then(|v| v.as_array())
                            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                            .unwrap_or_default();
                        let dry_run = input.get("dry_run").and_then(|v| v.as_bool()).unwrap_or(false);

                        let opts = ApplyBackOptions {
                            db_path: db_path.clone(),
                            gen_dir: gen_dir.clone(),
                            dry_run,
                            files,
                            eval_config: Some(eval_config.clone()),
                        };
                        let mut buf: Vec<u8> = Vec::new();
                        match apply_back::run_apply_back(opts, &mut buf) {
                            Ok(()) => send_text(id, &String::from_utf8_lossy(&buf)),
                            Err(e) => send_error(id, &format!("{:?}", e)),
                        }
                    }

                    Some("weaveback_apply_fix") => {
                        let Some(input) = input else {
                            send_error(id, "Missing arguments");
                            continue;
                        };
                        match handle_apply_fix(input, &agent_session) {
                            Ok(text) => send_text(id, &text),
                            Err(e) => send_error(id, &e),
                        }
                    }

                    Some("weaveback_chunk_context") => {
                        let Some(input) = input else {
                            send_error(id, "Missing arguments");
                            continue;
                        };
                        match handle_chunk_context(input, &agent_session) {
                            Ok(text) => send_text(id, &text),
                            Err(e) => send_error(id, &e),
                        }
                    }

                    Some("weaveback_list_chunks") => {
                        let file_filter = input
                            .and_then(|i| i.get("file"))
                            .and_then(|v| v.as_str());
                        match handle_list_chunks(file_filter, &db_path) {
                            Ok(text) => send_text(id, &text),
                            Err(e) => send_error(id, &e),
                        }
                    }

                    Some("weaveback_find_chunk") => {
                        let Some(input) = input else {
                            send_error(id, "Missing arguments");
                            continue;
                        };
                        match handle_find_chunk(input, &db_path) {
                            Ok(text) => send_text(id, &text),
                            Err(e) => send_error(id, &e),
                        }
                    }

                    Some("weaveback_lsp_definition") => {
                        let Some(input) = input else {
                            send_error(id, "Missing arguments");
                            continue;
                        };
                        let out_file = input.get("out_file").and_then(|v| v.as_str()).unwrap_or("");
                        let line     = input.get("line").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                        let col      = input.get("col").and_then(|v| v.as_u64()).unwrap_or(0) as u32;

                        if out_file.is_empty() || line == 0 || col == 0 {
                            send_error(id, "out_file, line, and col are required and must be > 0");
                            continue;
                        }

                        let ext = std::path::Path::new(out_file).extension().and_then(|e| e.to_str()).unwrap_or("");
                        let client = match get_or_spawn_lsp(&mut lsp_clients, ext) {
                            Ok(c) => c,
                            Err(e) => { send_error(id, &format!("LSP error: {e}")); continue; }
                        };

                        match client.goto_definition(std::path::Path::new(out_file), line - 1, col - 1) {
                            Ok(Some(loc)) => {
                                if let Ok(target_path) = loc.uri.to_file_path() {
                                    let db = if db_path.exists() { WeavebackDb::open_read_only(&db_path).ok() } else { None };
                                    let db = match db { Some(d) => d, None => { send_error(id, "Database not found"); continue; } };

                                    match lookup::perform_trace(
                                        target_path.to_string_lossy().as_ref(),
                                        loc.range.start.line + 1,
                                        loc.range.start.character + 1,
                                        &db,
                                        &resolver,
                                        eval_config.clone(),
                                    ) {
                                        Ok(Some(res)) => send_text(id, &serde_json::to_string_pretty(&res).unwrap()),
                                        Ok(None) => send_text(id, &serde_json::to_string_pretty(&json!({
                                            "out_file": target_path.to_string_lossy(),
                                            "out_line": loc.range.start.line + 1,
                                            "out_col":  loc.range.start.character + 1,
                                            "note": "LSP result could not be mapped to source"
                                        })).unwrap()),
                                        Err(e) => send_error(id, &format!("Mapping error: {e:?}")),
                                    }
                                } else {
                                    send_error(id, "LSP returned non-file URI");
                                }
                            }
                            Ok(None) => send_text(id, "No definition found."),
                            Err(e) => send_error(id, &format!("LSP call failed: {e}")),
                        }
                    }

                    Some("weaveback_lsp_references") => {
                        let Some(input) = input else {
                            send_error(id, "Missing arguments");
                            continue;
                        };
                        let out_file = input.get("out_file").and_then(|v| v.as_str()).unwrap_or("");
                        let line     = input.get("line").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                        let col      = input.get("col").and_then(|v| v.as_u64()).unwrap_or(0) as u32;

                        if out_file.is_empty() || line == 0 || col == 0 {
                            send_error(id, "out_file, line, and col are required and must be > 0");
                            continue;
                        }

                        let ext = std::path::Path::new(out_file).extension().and_then(|e| e.to_str()).unwrap_or("");
                        let client = match get_or_spawn_lsp(&mut lsp_clients, ext) {
                            Ok(c) => c,
                            Err(e) => { send_error(id, &format!("LSP error: {e}")); continue; }
                        };

                        match client.find_references(std::path::Path::new(out_file), line - 1, col - 1) {
                            Ok(locs) => {
                                let mut results = Vec::new();
                                let db = if db_path.exists() { WeavebackDb::open_read_only(&db_path).ok() } else { None };
                                let db = match db { Some(d) => d, None => { send_error(id, "Database not found"); continue; } };

                                for loc in locs {
                                    if let Ok(target_path) = loc.uri.to_file_path() {
                                        match lookup::perform_trace(
                                            target_path.to_string_lossy().as_ref(),
                                            loc.range.start.line + 1,
                                            loc.range.start.character + 1,
                                            &db,
                                            &resolver,
                                            eval_config.clone(),
                                        ) {
                                            Ok(Some(res)) => results.push(res),
                                            _ => results.push(json!({
                                                "out_file": target_path.to_string_lossy(),
                                                "out_line": loc.range.start.line + 1,
                                                "out_col":  loc.range.start.character + 1,
                                                "note": "LSP result could not be mapped to source"
                                            })),
                                        }
                                    }
                                }
                                send_text(id, &serde_json::to_string_pretty(&results).unwrap());
                            }
                            Err(e) => send_error(id, &format!("LSP call failed: {e}")),
                        }
                    }

                    Some("weaveback_lsp_hover") => {
                        let Some(input) = input else {
                            send_error(id, "Missing arguments");
                            continue;
                        };
                        let out_file = input.get("out_file").and_then(|v| v.as_str()).unwrap_or("");
                        let line     = input.get("line").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                        let col      = input.get("col").and_then(|v| v.as_u64()).unwrap_or(0) as u32;

                        if out_file.is_empty() || line == 0 || col == 0 {
                            send_error(id, "out_file, line, and col are required and must be > 0");
                            continue;
                        }

                        let ext = std::path::Path::new(out_file).extension().and_then(|e| e.to_str()).unwrap_or("");
                        let client = match get_or_spawn_lsp(&mut lsp_clients, ext) {
                            Ok(c) => c,
                            Err(e) => { send_error(id, &format!("LSP error: {e}")); continue; }
                        };

                        match client.hover(std::path::Path::new(out_file), line - 1, col - 1) {
                            Ok(Some(hover)) => {
                                let db = if db_path.exists() { WeavebackDb::open_read_only(&db_path).ok() } else { None };
                                let db = match db { Some(d) => d, None => { send_error(id, "Database not found"); continue; } };

                                // Also trace the current point to show which chunk we are in
                                let trace = lookup::perform_trace(out_file, line, col, &db, &resolver, eval_config.clone()).ok().flatten();

                                let mut res = json!({
                                    "hover": hover,
                                });
                                if let Some(t) = trace {
                                    res.as_object_mut().unwrap().insert("source".into(), t);
                                }
                                send_text(id, &serde_json::to_string_pretty(&res).unwrap());
                            }
                            Ok(None) => send_text(id, "No hover info found."),
                            Err(e) => send_error(id, &format!("LSP call failed: {e}")),
                        }
                    }

                    Some("weaveback_lsp_diagnostics") => {
                        let Some(input) = input else {
                            send_error(id, "Missing arguments");
                            continue;
                        };
                        let out_file = input.get("out_file").and_then(|v| v.as_str()).unwrap_or("");
                        if out_file.is_empty() {
                            send_error(id, "out_file is required");
                            continue;
                        }

                        let ext = std::path::Path::new(out_file).extension().and_then(|e| e.to_str()).unwrap_or("");
                        let client = match get_or_spawn_lsp(&mut lsp_clients, ext) {
                            Ok(c) => c,
                            Err(e) => { send_error(id, &format!("LSP error: {e}")); continue; }
                        };

                        let diags = client.get_diagnostics(std::path::Path::new(out_file));
                        let db = if db_path.exists() { WeavebackDb::open_read_only(&db_path).ok() } else { None };
                        let db = match db { Some(d) => d, None => { send_error(id, "Database not found"); continue; } };

                        let mut mapped = Vec::new();
                        for d in diags {
                            let line = d.range.start.line + 1;
                            let col = d.range.start.character + 1;
                            let trace = lookup::perform_trace(out_file, line, col, &db, &resolver, eval_config.clone()).ok().flatten();
                            mapped.push(json!({
                                "diagnostic": d,
                                "source": trace,
                            }));
                        }
                        send_text(id, &serde_json::to_string_pretty(&mapped).unwrap());
                    }

                    Some("weaveback_lsp_symbols") => {
                        let Some(input) = input else {
                            send_error(id, "Missing arguments");
                            continue;
                        };
                        let out_file = input.get("out_file").and_then(|v| v.as_str()).unwrap_or("");
                        if out_file.is_empty() {
                            send_error(id, "out_file is required");
                            continue;
                        }

                        let ext = std::path::Path::new(out_file).extension().and_then(|e| e.to_str()).unwrap_or("");
                        let client = match get_or_spawn_lsp(&mut lsp_clients, ext) {
                            Ok(c) => c,
                            Err(e) => { send_error(id, &format!("LSP error: {e}")); continue; }
                        };

                        match client.document_symbols(std::path::Path::new(out_file)) {
                            Ok(symbols) => {
                                send_text(id, &serde_json::to_string_pretty(&symbols).unwrap());
                            }
                            Err(e) => send_error(id, &format!("LSP call failed: {e}")),
                        }
                    }

                    Some("weaveback_search") => {
                        match handle_search(input, &db_path, &agent_session) {
                            Ok(text) => send_text(id, &text),
                            Err(e) => send_error(id, &e),
                        }
                    }

                    Some("weaveback_list_tags") => {
                        let file_filter = input.as_ref()
                            .and_then(|v| v.get("file"))
                            .and_then(|v| v.as_str());
                        match handle_list_tags(file_filter, &db_path) {
                            Ok(text) => send_text(id, &text),
                            Err(e) => send_error(id, &e),
                        }
                    }

                    other => send_error(id, &format!("Unknown tool: {:?}", other)),
                }
            }

            "notifications/initialized" => {}
            _ => {}
        }
    }
    Ok(())
}

fn send_response(id: Option<Value>, result: Value) {
    println!("{}", serde_json::to_string(&response_payload(id, result)).unwrap());
}

fn send_text(id: Option<Value>, text: &str) {
    send_response(id, text_result(text));
}

fn send_error(id: Option<Value>, msg: &str) {
    send_response(id, error_result(msg));
}

#[cfg(test)]
mod tests {
    use super::{
        build_apply_fix_plan, error_result, handle_apply_fix, handle_chunk_context,
        handle_find_chunk, handle_list_chunks, handle_list_tags, handle_search,
        response_payload, text_result, tools_list_result,
    };
    use serde_json::{json, Map, Value};
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};
    use weaveback_agent_core::{Workspace as AgentWorkspace, WorkspaceConfig as AgentWorkspaceConfig};
    use weaveback_tangle::block_parser::SourceBlockEntry;
    use weaveback_tangle::db::{ChunkDefEntry, Confidence, NowebMapEntry, TangleConfig, WeavebackDb};

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    struct TestWorkspace {
        root: PathBuf,
        db_path: PathBuf,
        gen_dir: PathBuf,
    }

    impl TestWorkspace {
        fn new() -> Self {
            let unique = format!(
                "wb-mcp-tests-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .expect("clock drifted backwards")
                    .as_nanos()
                    + u128::from(TEST_COUNTER.fetch_add(1, Ordering::Relaxed))
            );
            let root = std::env::temp_dir().join(unique);
            let gen_dir = root.join("gen");
            let db_path = root.join("weaveback.db");
            fs::create_dir_all(&gen_dir).expect("create temp workspace");
            Self { root, db_path, gen_dir }
        }

        fn agent_session(&self) -> weaveback_agent_core::Session {
            AgentWorkspace::open(AgentWorkspaceConfig {
                project_root: self.root.clone(),
                db_path: self.db_path.clone(),
                gen_dir: self.gen_dir.clone(),
            })
            .session()
        }

        fn write_source(&self, rel: &str, content: &str) -> PathBuf {
            let path = self.root.join(rel);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).expect("create source parent");
            }
            fs::write(&path, content).expect("write source");
            path
        }

        fn read_source(&self, rel: &str) -> String {
            fs::read_to_string(self.root.join(rel)).expect("read source")
        }

        fn open_db(&self) -> WeavebackDb {
            WeavebackDb::open(&self.db_path).expect("open sqlite db")
        }
    }

    impl Drop for TestWorkspace {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn block(index: u32, block_type: &str, line_start: u32, line_end: u32) -> SourceBlockEntry {
        SourceBlockEntry {
            block_index: index,
            block_type: block_type.to_string(),
            line_start,
            line_end,
            content_hash: [0u8; 32],
        }
    }

    #[test]
    fn tools_list_contains_expected_core_tools() {
        let result = tools_list_result();
        let tools = result
            .get("tools")
            .and_then(|value| value.as_array())
            .expect("tools array");
        assert!(tools.len() >= 10);
        assert!(tools.iter().any(|tool| tool.get("name") == Some(&json!("weaveback_trace"))));
        assert!(tools.iter().any(|tool| tool.get("name") == Some(&json!("weaveback_apply_fix"))));
        assert!(tools.iter().any(|tool| tool.get("name") == Some(&json!("weaveback_search"))));
    }

    #[test]
    fn apply_fix_plan_builder_preserves_request_fields() {
        let plan = build_apply_fix_plan(
            "/tmp/source.adoc",
            4,
            6,
            vec!["one".to_string(), "two".to_string()],
            "gen/out.rs",
            8,
            "expected line",
        );
        assert_eq!(plan.plan_id, "mcp-apply-fix");
        assert_eq!(plan.edits.len(), 1);
        let edit = &plan.edits[0];
        assert_eq!(edit.edit_id, "edit-1");
        assert_eq!(edit.target.src_file, "/tmp/source.adoc");
        assert_eq!(edit.target.src_line, 4);
        assert_eq!(edit.target.src_line_end, 6);
        assert_eq!(edit.new_src_lines, vec!["one", "two"]);
        assert_eq!(edit.anchor.out_file, "gen/out.rs");
        assert_eq!(edit.anchor.out_line, 8);
        assert_eq!(edit.anchor.expected_output, "expected line");
    }

    #[test]
    fn response_helpers_wrap_jsonrpc_text_and_error_shapes() {
        let text = response_payload(Some(json!(7)), text_result("hello"));
        assert_eq!(text["jsonrpc"], json!("2.0"));
        assert_eq!(text["id"], json!(7));
        assert_eq!(text["result"]["content"][0]["type"], json!("text"));
        assert_eq!(text["result"]["content"][0]["text"], json!("hello"));

        let err = response_payload(Some(json!("req-1")), error_result("boom"));
        assert_eq!(err["result"]["isError"], json!(true));
        assert_eq!(err["result"]["content"][0]["text"], json!("boom"));
    }

    #[test]
    fn handle_search_returns_pretty_json_hits() {
        let workspace = TestWorkspace::new();
        let source = "= Intro\n\nLiterate search text.\n";
        workspace.write_source("docs/search.adoc", source);

        let mut db = workspace.open_db();
        db.set_src_snapshot("docs/search.adoc", source.as_bytes()).unwrap();
        db.set_source_blocks(
            "docs/search.adoc",
            &[block(0, "section", 1, 1), block(1, "para", 3, 3)],
        )
        .unwrap();
        db.set_block_tags("docs/search.adoc", 1, &[1u8; 32], "search,docs")
            .unwrap();
        db.rebuild_prose_fts().unwrap();
        drop(db);

        let mut input = Map::new();
        input.insert("query".to_string(), json!("literate"));
        let text = handle_search(Some(&input), &workspace.db_path, &workspace.agent_session()).unwrap();
        let value: Value = serde_json::from_str(&text).unwrap();
        let hits = value.as_array().expect("search results array");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0]["src_file"], "docs/search.adoc");
        assert_eq!(hits[0]["block_type"], "para");
        assert_eq!(hits[0]["tags"], json!(["search", "docs"]));
    }

    #[test]
    fn handle_chunk_context_returns_serialized_context() {
        let workspace = TestWorkspace::new();
        let source = [
            "= Root",
            "",
            "== MCP",
            "Context prose.",
            "",
            "// <<alpha>>=",
            "alpha line",
            "<<beta>>",
            "// @",
            "",
            "// <<beta>>=",
            "beta line",
            "// @",
        ]
        .join("\n");
        workspace.write_source("docs/mcp.adoc", &source);

        let mut db = workspace.open_db();
        db.set_chunk_defs(&[
            ChunkDefEntry {
                src_file: "docs/mcp.adoc".to_string(),
                chunk_name: "alpha".to_string(),
                nth: 0,
                def_start: 6,
                def_end: 9,
            },
            ChunkDefEntry {
                src_file: "docs/mcp.adoc".to_string(),
                chunk_name: "beta".to_string(),
                nth: 0,
                def_start: 11,
                def_end: 13,
            },
        ])
        .unwrap();
        db.set_chunk_deps(&[(
            "alpha".to_string(),
            "beta".to_string(),
            "docs/mcp.adoc".to_string(),
        )])
        .unwrap();
        db.set_noweb_entries(
            "gen/out.rs",
            &[(
                0,
                NowebMapEntry {
                    src_file: "docs/mcp.adoc".to_string(),
                    chunk_name: "alpha".to_string(),
                    src_line: 5,
                    indent: String::new(),
                    confidence: Confidence::Exact,
                },
            )],
        )
        .unwrap();
        drop(db);

        let mut input = Map::new();
        input.insert("file".to_string(), json!("docs/mcp.adoc"));
        input.insert("name".to_string(), json!("alpha"));
        let text = handle_chunk_context(&input, &workspace.agent_session()).unwrap();
        let value: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(value["file"], "docs/mcp.adoc");
        assert_eq!(value["name"], "alpha");
        assert_eq!(value["body"], "alpha line\n<<beta>>");
        assert_eq!(value["section_title_chain"], json!(["Root", "MCP"]));
        assert_eq!(value["section_prose"], "== MCP\nContext prose.");
        assert_eq!(value["dependencies"], json!(["beta"]));
        assert_eq!(value["output_files"], json!(["gen/out.rs"]));
    }

    #[test]
    fn handle_apply_fix_applies_or_reports_validation_errors() {
        let workspace = TestWorkspace::new();
        let src_path = workspace.write_source("docs/fix.adoc", "before\n");

        let mut db = workspace.open_db();
        db.set_src_snapshot("docs/fix.adoc", b"before\n").unwrap();
        db.set_source_config(
            "docs/fix.adoc",
            &TangleConfig {
                sigil: '%',
                open_delim: "<<".to_string(),
                close_delim: ">>".to_string(),
                chunk_end: "@".to_string(),
                comment_markers: vec!["//".to_string()],
            },
        )
        .unwrap();
        db.set_noweb_entries(
            "gen/out.txt",
            &[(
                0,
                NowebMapEntry {
                    src_file: "docs/fix.adoc".to_string(),
                    chunk_name: "literal".to_string(),
                    src_line: 0,
                    indent: String::new(),
                    confidence: Confidence::Exact,
                },
            )],
        )
        .unwrap();
        drop(db);

        let mut ok_input = Map::new();
        ok_input.insert("src_file".to_string(), json!(src_path.to_string_lossy()));
        ok_input.insert("src_line".to_string(), json!(1));
        ok_input.insert("new_src_line".to_string(), json!("after"));
        ok_input.insert("out_file".to_string(), json!("gen/out.txt"));
        ok_input.insert("out_line".to_string(), json!(1));
        ok_input.insert("expected_output".to_string(), json!("after"));
        let text = handle_apply_fix(&ok_input, &workspace.agent_session()).unwrap();
        assert!(text.contains("Applied ChangePlan mcp-apply-fix"));
        assert_eq!(workspace.read_source("docs/fix.adoc"), "after\n");

        let mut bad_input = Map::new();
        bad_input.insert("src_file".to_string(), json!(src_path.to_string_lossy()));
        bad_input.insert("src_line".to_string(), json!(0));
        bad_input.insert("out_file".to_string(), json!("gen/out.txt"));
        bad_input.insert("out_line".to_string(), json!(1));
        bad_input.insert("expected_output".to_string(), json!("after"));
        let err = handle_apply_fix(&bad_input, &workspace.agent_session()).unwrap_err();
        assert_eq!(err, "src_line must be >= 1");
    }

    #[test]
    fn handle_search_reports_missing_query_and_missing_db() {
        let workspace = TestWorkspace::new();
        let empty = Map::new();
        let err = handle_search(Some(&empty), &workspace.db_path, &workspace.agent_session()).unwrap_err();
        assert_eq!(err, "query is required");

        let mut input = Map::new();
        input.insert("query".to_string(), json!("anything"));
        let err = handle_search(Some(&input), &workspace.db_path, &workspace.agent_session()).unwrap_err();
        assert_eq!(err, "Database not found. Run weaveback on your source files first.");
    }

    #[test]
    fn handle_chunk_context_reports_missing_or_unknown_chunk() {
        let workspace = TestWorkspace::new();
        let mut input = Map::new();
        let err = handle_chunk_context(&input, &workspace.agent_session()).unwrap_err();
        assert_eq!(err, "file and name are required");

        workspace.write_source("docs/empty.adoc", "= Empty\n");
        let db = workspace.open_db();
        db.set_src_snapshot("docs/empty.adoc", b"= Empty\n").unwrap();
        drop(db);

        input.insert("file".to_string(), json!("docs/empty.adoc"));
        input.insert("name".to_string(), json!("missing"));
        let err = handle_chunk_context(&input, &workspace.agent_session()).unwrap_err();
        assert_eq!(err, "Chunk not found: docs/empty.adoc#missing[0]");
    }

    #[test]
    fn handle_apply_fix_reports_bad_range_order() {
        let workspace = TestWorkspace::new();
        let mut input = Map::new();
        input.insert("src_file".to_string(), json!("/tmp/source.adoc"));
        input.insert("src_line".to_string(), json!(3));
        input.insert("src_line_end".to_string(), json!(2));
        input.insert("out_file".to_string(), json!("gen/out.txt"));
        input.insert("out_line".to_string(), json!(1));
        input.insert("expected_output".to_string(), json!("x"));

        let err = handle_apply_fix(&input, &workspace.agent_session()).unwrap_err();
        assert_eq!(err, "src_line_end must be >= src_line");
    }

    #[test]
    fn handle_list_chunks_and_find_chunk_return_serialized_results() {
        let workspace = TestWorkspace::new();
        workspace.write_source(
            "docs/list.adoc",
            "= List\n\n// <<alpha>>=\nbody\n// @\n\n// <<beta>>=\nmore\n// @\n",
        );
        let mut db = workspace.open_db();
        db.set_chunk_defs(&[
            weaveback_tangle::db::ChunkDefEntry {
                src_file: "docs/list.adoc".to_string(),
                chunk_name: "alpha".to_string(),
                nth: 0,
                def_start: 3,
                def_end: 5,
            },
            weaveback_tangle::db::ChunkDefEntry {
                src_file: "docs/list.adoc".to_string(),
                chunk_name: "beta".to_string(),
                nth: 0,
                def_start: 7,
                def_end: 9,
            },
        ])
        .unwrap();
        drop(db);

        let listed = handle_list_chunks(Some("docs/list.adoc"), &workspace.db_path).unwrap();
        let listed: Value = serde_json::from_str(&listed).unwrap();
        assert_eq!(listed.as_array().unwrap().len(), 2);
        assert_eq!(listed[0]["name"], "alpha");

        let mut input = Map::new();
        input.insert("name".to_string(), json!("beta"));
        let found = handle_find_chunk(&input, &workspace.db_path).unwrap();
        let found: Value = serde_json::from_str(&found).unwrap();
        assert_eq!(found.as_array().unwrap().len(), 1);
        assert_eq!(found[0]["file"], "docs/list.adoc");
    }

    #[test]
    fn handle_list_chunks_find_chunk_and_tags_report_missing_data() {
        let workspace = TestWorkspace::new();

        let err = handle_list_chunks(None, &workspace.db_path).unwrap_err();
        assert_eq!(err, "Database not found. Run weaveback on your source files first.");

        let empty = Map::new();
        let err = handle_find_chunk(&empty, &workspace.db_path).unwrap_err();
        assert_eq!(err, "name is required");

        let err = handle_list_tags(None, &workspace.db_path).unwrap_err();
        assert_eq!(err, "Database not found. Run weaveback on your source files first.");
    }

    #[test]
    fn handle_list_tags_returns_serialized_tag_rows() {
        let workspace = TestWorkspace::new();
        let mut db = workspace.open_db();
        db.set_source_blocks(
            "docs/tags.adoc",
            &[block(0, "para", 1, 2)],
        )
        .unwrap();
        db.set_block_tags("docs/tags.adoc", 0, &[1, 2, 3], "sqlite,fts")
            .unwrap();
        drop(db);

        let text = handle_list_tags(Some("docs/tags.adoc"), &workspace.db_path).unwrap();
        let value: Value = serde_json::from_str(&text).unwrap();
        let arr = value.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["src_file"], "docs/tags.adoc");
        assert_eq!(arr[0]["tags"], "sqlite,fts");
    }
}
