// weaveback-api/src/mcp/run.rs
// I'd Really Rather You Didn't edit this generated file.

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
use std::io::{BufRead, Write};
use std::path::PathBuf;

use super::helpers::{send_error, send_response, send_text};

pub(crate) fn get_or_spawn_lsp<'a>(
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

pub fn run_mcp<R: BufRead, W: Write>(
    reader: R,
    mut writer: W,
    db_path: PathBuf,
    gen_dir: PathBuf,
    project_root: PathBuf,
    eval_config: EvalConfig,
) -> Result<(), std::io::Error> {
    let mut lsp_clients: HashMap<String, LspClient> = HashMap::new();
    let agent_workspace = AgentWorkspace::open(AgentWorkspaceConfig {
        project_root: project_root.clone(),
        db_path: db_path.clone(),
        gen_dir: gen_dir.clone(),
    });
    let agent_session = agent_workspace.session();
    let resolver = PathResolver::new(project_root, gen_dir.clone());

    for line in reader.lines() {
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
                send_response(&mut writer, id, json!({
                    "protocolVersion": "2024-11-05",
                    "capabilities": { "tools": {} },
                    "serverInfo": { "name": "Weaveback Trace Server", "version": "0.1.0" }
                }));
            }

            "tools/list" => {
                send_response(&mut writer, id, json!({
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
                                    "name": { "type": "string", "description": "Chunk name to look up" }
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
                                    },
                                    {
                                        "name": "weaveback_coverage",
                                        "description": "Get test coverage summary grouped by literate source chunks and sections, sorted by missed lines. Use this to prioritize what to test. Requires a valid lcov.info file. Note: if no lcov_path is provided, defaults to 'lcov.info'.",
                                        "inputSchema": {
                                            "type": "object",
                                            "properties": {
                                                "lcov_path": { "type": "string", "description": "Path to the lcov.info file (defaults to lcov.info in the root directory)" }
                                            }
                                        }
                                    }
                                ]
                            }));
                        }

            "tools/call" => {
                let params = req.get("params").and_then(|p| p.as_object());
                let tool_name = params.and_then(|p| p.get("name")).and_then(|n| n.as_str());
                let input = params.and_then(|p| p.get("arguments")).and_then(|a| a.as_object());

                match tool_name {
                    Some("weaveback_trace") => {
                        let Some(input) = input else {
                            send_error(&mut writer, id, "Missing arguments");
                            continue;
                        };
                        let out_file = input.get("out_file").and_then(|f| f.as_str()).unwrap_or("");
                        let out_line = input.get("out_line").and_then(|l| l.as_u64()).unwrap_or(0) as u32;
                        let out_col  = input.get("out_col") .and_then(|c| c.as_u64()).unwrap_or(0) as u32;

                        if !db_path.exists() {
                            send_error(&mut writer, id, "Database not found. Run weaveback on your source files first.");
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
                                send_text(&mut writer, id, &serde_json::to_string(&Value::Object(obj)).unwrap())
                            }
                            Ok(None) => send_error(&mut writer, id, &format!("No mapping found for {}:{}", out_file, out_line)),
                            Err(e) => send_error(&mut writer, id, &format!("Lookup error: {e}")),
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
                            Ok(()) => send_text(&mut writer, id, &String::from_utf8_lossy(&buf)),
                            Err(e) => send_error(&mut writer, id, &format!("{:?}", e)),
                        }
                    }

                    Some("weaveback_apply_fix") => {
                        let Some(input) = input else {
                            send_error(&mut writer, id, "Missing arguments");
                            continue;
                        };
                        let src_file   = input.get("src_file")       .and_then(|v| v.as_str()).unwrap_or("");
                        let src_line_1 = input.get("src_line")        .and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                        let src_line_end_1 = input.get("src_line_end").and_then(|v| v.as_u64())
                            .map(|v| v as usize).unwrap_or(src_line_1);
                        let new_lines: Vec<String> = if let Some(arr) = input.get("new_src_lines").and_then(|v| v.as_array()) {
                            arr.iter().filter_map(|v| v.as_str().map(str::to_string)).collect()
                        } else {
                            let s = input.get("new_src_line").and_then(|v| v.as_str()).unwrap_or("");
                            vec![s.to_string()]
                        };
                        let out_file   = input.get("out_file")        .and_then(|v| v.as_str()).unwrap_or("");
                        let out_line_1 = input.get("out_line")        .and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                        let expected   = input.get("expected_output") .and_then(|v| v.as_str()).unwrap_or("");

                        if src_line_1 == 0 {
                            send_error(&mut writer, id, "src_line must be >= 1");
                            continue;
                        }
                        if src_line_end_1 < src_line_1 {
                            send_error(&mut writer, id, "src_line_end must be >= src_line");
                            continue;
                        }

                        let plan = ChangePlan {
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
                        };
                        match agent_session.apply_change_plan(&plan) {
                            Ok(result) if result.applied => send_text(&mut writer,
                                id,
                                &format!(
                                    "Applied ChangePlan {} with edits: {}",
                                    result.plan_id,
                                    result.applied_edit_ids.join(", ")
                                ),
                            ),
                            Ok(result) => send_error(&mut writer,
                                id,
                                &format!(
                                    "Failed ChangePlan {}. Failed edits: {}",
                                    result.plan_id,
                                    result.failed_edit_ids.join(", ")
                                ),
                            ),
                            Err(e)  => send_error(&mut writer, id, &e),
                        }
                    }

                    Some("weaveback_chunk_context") => {
                        let Some(input) = input else {
                            send_error(&mut writer, id, "Missing arguments");
                            continue;
                        };
                        let file = input.get("file").and_then(|v| v.as_str()).unwrap_or("");
                        let name = input.get("name").and_then(|v| v.as_str()).unwrap_or("");
                        let nth  = input.get("nth").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                        if file.is_empty() || name.is_empty() {
                            send_error(&mut writer, id, "file and name are required");
                            continue;
                        }
                        match agent_session.chunk_context(file, name, nth) {
                            Ok(ctx) => {
                                let obj = json!({
                                    "file": ctx.file,
                                    "name": ctx.name,
                                    "nth": ctx.nth,
                                    "body": ctx.body,
                                    "section_title_chain": ctx.section_breadcrumb,
                                    "section_prose": ctx.prose,
                                    "dependencies": ctx.direct_dependencies,
                                    "output_files": ctx.outputs,
                                });
                                send_text(&mut writer, id, &serde_json::to_string_pretty(&obj).unwrap());
                            }
                            Err(_) => send_error(&mut writer, id, &format!("Chunk not found: {}#{}[{}]", file, name, nth)),
                        }
                    }

                    Some("weaveback_list_chunks") => {
                        let file_filter = input
                            .and_then(|i| i.get("file"))
                            .and_then(|v| v.as_str());
                        if !db_path.exists() {
                            send_error(&mut writer, id, "Database not found. Run weaveback on your source files first.");
                            continue;
                        }
                        match WeavebackDb::open_read_only(&db_path) {
                            Err(e) => send_error(&mut writer, id, &format!("Database error: {e:?}")),
                            Ok(db) => match db.list_chunk_defs(file_filter) {
                                Err(e) => send_error(&mut writer, id, &format!("Query error: {e:?}")),
                                Ok(defs) => {
                                    let arr: Vec<Value> = defs.iter().map(|d| json!({
                                        "file":      d.src_file,
                                        "name":      d.chunk_name,
                                        "nth":       d.nth,
                                        "def_start": d.def_start,
                                        "def_end":   d.def_end,
                                    })).collect();
                                    send_text(&mut writer, id, &serde_json::to_string_pretty(&arr).unwrap());
                                }
                            },
                        }
                    }

                    Some("weaveback_find_chunk") => {
                        let Some(input) = input else {
                            send_error(&mut writer, id, "Missing arguments");
                            continue;
                        };
                        let name = input.get("name").and_then(|v| v.as_str()).unwrap_or("");
                        if name.is_empty() {
                            send_error(&mut writer, id, "name is required");
                            continue;
                        }
                        if !db_path.exists() {
                            send_error(&mut writer, id, "Database not found. Run weaveback on your source files first.");
                            continue;
                        }
                        match WeavebackDb::open_read_only(&db_path) {
                            Err(e) => send_error(&mut writer, id, &format!("Database error: {e:?}")),
                            Ok(db) => match db.find_chunk_defs_by_name(name) {
                                Err(e) => send_error(&mut writer, id, &format!("Query error: {e:?}")),
                                Ok(defs) => {
                                    let arr: Vec<Value> = defs.iter().map(|d| json!({
                                        "file":      d.src_file,
                                        "nth":       d.nth,
                                        "def_start": d.def_start,
                                        "def_end":   d.def_end,
                                    })).collect();
                                    send_text(&mut writer, id, &serde_json::to_string_pretty(&arr).unwrap());
                                }
                            },
                        }
                    }

                    Some("weaveback_lsp_definition") => {
                        let Some(input) = input else {
                            send_error(&mut writer, id, "Missing arguments");
                            continue;
                        };
                        let out_file = input.get("out_file").and_then(|v| v.as_str()).unwrap_or("");
                        let line     = input.get("line").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                        let col      = input.get("col").and_then(|v| v.as_u64()).unwrap_or(0) as u32;

                        if out_file.is_empty() || line == 0 || col == 0 {
                            send_error(&mut writer, id, "out_file, line, and col are required and must be > 0");
                            continue;
                        }

                        let ext = std::path::Path::new(out_file).extension().and_then(|e| e.to_str()).unwrap_or("");
                        let client = match get_or_spawn_lsp(&mut lsp_clients, ext) {
                            Ok(c) => c,
                            Err(e) => { send_error(&mut writer, id, &format!("LSP error: {e}")); continue; }
                        };

                        match client.goto_definition(std::path::Path::new(out_file), line - 1, col - 1) {
                            Ok(Some(loc)) => {
                                if let Ok(target_path) = loc.uri.to_file_path() {
                                    let db = if db_path.exists() { WeavebackDb::open_read_only(&db_path).ok() } else { None };
                                    let db = match db { Some(d) => d, None => { send_error(&mut writer, id, "Database not found"); continue; } };

                                    match lookup::perform_trace(
                                        target_path.to_string_lossy().as_ref(),
                                        loc.range.start.line + 1,
                                        loc.range.start.character + 1,
                                        &db,
                                        &resolver,
                                        eval_config.clone(),
                                    ) {
                                        Ok(Some(res)) => send_text(&mut writer, id, &serde_json::to_string_pretty(&res).unwrap()),
                                        Ok(None) => send_text(&mut writer, id, &serde_json::to_string_pretty(&json!({
                                            "out_file": target_path.to_string_lossy(),
                                            "out_line": loc.range.start.line + 1,
                                            "out_col":  loc.range.start.character + 1,
                                            "note": "LSP result could not be mapped to source"
                                        })).unwrap()),
                                        Err(e) => send_error(&mut writer, id, &format!("Mapping error: {e:?}")),
                                    }
                                } else {
                                    send_error(&mut writer, id, "LSP returned non-file URI");
                                }
                            }
                            Ok(None) => send_text(&mut writer, id, "No definition found."),
                            Err(e) => send_error(&mut writer, id, &format!("LSP call failed: {e}")),
                        }
                    }

                    Some("weaveback_lsp_references") => {
                        let Some(input) = input else {
                            send_error(&mut writer, id, "Missing arguments");
                            continue;
                        };
                        let out_file = input.get("out_file").and_then(|v| v.as_str()).unwrap_or("");
                        let line     = input.get("line").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                        let col      = input.get("col").and_then(|v| v.as_u64()).unwrap_or(0) as u32;

                        if out_file.is_empty() || line == 0 || col == 0 {
                            send_error(&mut writer, id, "out_file, line, and col are required and must be > 0");
                            continue;
                        }

                        let ext = std::path::Path::new(out_file).extension().and_then(|e| e.to_str()).unwrap_or("");
                        let client = match get_or_spawn_lsp(&mut lsp_clients, ext) {
                            Ok(c) => c,
                            Err(e) => { send_error(&mut writer, id, &format!("LSP error: {e}")); continue; }
                        };

                        match client.find_references(std::path::Path::new(out_file), line - 1, col - 1) {
                            Ok(locs) => {
                                let mut results = Vec::new();
                                let db = if db_path.exists() { WeavebackDb::open_read_only(&db_path).ok() } else { None };
                                let db = match db { Some(d) => d, None => { send_error(&mut writer, id, "Database not found"); continue; } };

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
                                send_text(&mut writer, id, &serde_json::to_string_pretty(&results).unwrap());
                            }
                            Err(e) => send_error(&mut writer, id, &format!("LSP call failed: {e}")),
                        }
                    }

                    Some("weaveback_lsp_hover") => {
                        let Some(input) = input else {
                            send_error(&mut writer, id, "Missing arguments");
                            continue;
                        };
                        let out_file = input.get("out_file").and_then(|v| v.as_str()).unwrap_or("");
                        let line     = input.get("line").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                        let col      = input.get("col").and_then(|v| v.as_u64()).unwrap_or(0) as u32;

                        if out_file.is_empty() || line == 0 || col == 0 {
                            send_error(&mut writer, id, "out_file, line, and col are required and must be > 0");
                            continue;
                        }

                        let ext = std::path::Path::new(out_file).extension().and_then(|e| e.to_str()).unwrap_or("");
                        let client = match get_or_spawn_lsp(&mut lsp_clients, ext) {
                            Ok(c) => c,
                            Err(e) => { send_error(&mut writer, id, &format!("LSP error: {e}")); continue; }
                        };

                        match client.hover(std::path::Path::new(out_file), line - 1, col - 1) {
                            Ok(Some(hover)) => {
                                let db = if db_path.exists() { WeavebackDb::open_read_only(&db_path).ok() } else { None };
                                let db = match db { Some(d) => d, None => { send_error(&mut writer, id, "Database not found"); continue; } };

                                // Also trace the current point to show which chunk we are in
                                let trace = lookup::perform_trace(out_file, line, col, &db, &resolver, eval_config.clone()).ok().flatten();

                                let mut res = json!({
                                    "hover": hover,
                                });
                                if let Some(t) = trace {
                                    res.as_object_mut().unwrap().insert("source".into(), t);
                                }
                                send_text(&mut writer, id, &serde_json::to_string_pretty(&res).unwrap());
                            }
                            Ok(None) => send_text(&mut writer, id, "No hover info found."),
                            Err(e) => send_error(&mut writer, id, &format!("LSP call failed: {e}")),
                        }
                    }

                    Some("weaveback_lsp_diagnostics") => {
                        let Some(input) = input else {
                            send_error(&mut writer, id, "Missing arguments");
                            continue;
                        };
                        let out_file = input.get("out_file").and_then(|v| v.as_str()).unwrap_or("");
                        if out_file.is_empty() {
                            send_error(&mut writer, id, "out_file is required");
                            continue;
                        }

                        let ext = std::path::Path::new(out_file).extension().and_then(|e| e.to_str()).unwrap_or("");
                        let client = match get_or_spawn_lsp(&mut lsp_clients, ext) {
                            Ok(c) => c,
                            Err(e) => { send_error(&mut writer, id, &format!("LSP error: {e}")); continue; }
                        };

                        let diags = client.get_diagnostics(std::path::Path::new(out_file));
                        let db = if db_path.exists() { WeavebackDb::open_read_only(&db_path).ok() } else { None };
                        let db = match db { Some(d) => d, None => { send_error(&mut writer, id, "Database not found"); continue; } };

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
                        send_text(&mut writer, id, &serde_json::to_string_pretty(&mapped).unwrap());
                    }

                    Some("weaveback_lsp_symbols") => {
                        let Some(input) = input else {
                            send_error(&mut writer, id, "Missing arguments");
                            continue;
                        };
                        let out_file = input.get("out_file").and_then(|v| v.as_str()).unwrap_or("");
                        if out_file.is_empty() {
                            send_error(&mut writer, id, "out_file is required");
                            continue;
                        }

                        let ext = std::path::Path::new(out_file).extension().and_then(|e| e.to_str()).unwrap_or("");
                        let client = match get_or_spawn_lsp(&mut lsp_clients, ext) {
                            Ok(c) => c,
                            Err(e) => { send_error(&mut writer, id, &format!("LSP error: {e}")); continue; }
                        };

                        match client.document_symbols(std::path::Path::new(out_file)) {
                            Ok(symbols) => {
                                send_text(&mut writer, id, &serde_json::to_string_pretty(&symbols).unwrap());
                            }
                            Err(e) => send_error(&mut writer, id, &format!("LSP call failed: {e}")),
                        }
                    }

                    Some("weaveback_search") => {
                        let query = input.as_ref()
                            .and_then(|v| v.get("query"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        if query.is_empty() {
                            send_error(&mut writer, id, "query is required");
                            continue;
                        }
                        let limit = input.as_ref()
                            .and_then(|v| v.get("limit"))
                            .and_then(|v| v.as_u64())
                            .unwrap_or(10) as usize;
                        if !db_path.exists() {
                            send_error(&mut writer, id, "Database not found. Run weaveback on your source files first.");
                            continue;
                        }
                        match agent_session.search(query, limit) {
                            Err(e) => send_error(&mut writer, id, &format!("Search error: {e}")),
                            Ok(results) => {
                                let arr: Vec<Value> = results.iter().map(|r| {
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
                                }).collect();
                                send_text(&mut writer, id, &serde_json::to_string_pretty(&arr).unwrap());
                            }
                        }
                    }

                    Some("weaveback_list_tags") => {
                        let file_filter = input.as_ref()
                            .and_then(|v| v.get("file"))
                            .and_then(|v| v.as_str());
                        if !db_path.exists() {
                            send_error(&mut writer, id, "Database not found. Run weaveback on your source files first.");
                            continue;
                        }
                        match WeavebackDb::open_read_only(&db_path) {
                            Err(e) => send_error(&mut writer, id, &format!("Database error: {e:?}")),
                            Ok(db) => match db.list_block_tags(file_filter) {
                                Err(e) => send_error(&mut writer, id, &format!("Tag list error: {e:?}")),
                                Ok(blocks) => {
                                    let arr: Vec<Value> = blocks.iter().map(|b| json!({
                                        "src_file":    b.src_file,
                                        "block_index": b.block_index,
                                        "block_type":  b.block_type,
                                        "line_start":  b.line_start,
                                        "tags":        b.tags,
                                    })).collect();
                                    send_text(&mut writer, id, &serde_json::to_string_pretty(&arr).unwrap());
                                }
                            },
                        }
                    }

                    Some("weaveback_coverage") => {
                        let lcov_path = input.as_ref()
                            .and_then(|v| v.get("lcov_path"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("lcov.info");
                        let path = std::path::Path::new(lcov_path);
                        if !path.exists() {
                            send_error(&mut writer, id, &format!("lcov file not found at {}", path.display()));
                            continue;
                        }
                        if !db_path.exists() {
                            send_error(&mut writer, id, "Database not found. Run weaveback on your source files first.");
                            continue;
                        }
                        match (std::fs::read_to_string(path), WeavebackDb::open_read_only(&db_path)) {
                            (Ok(lcov_text), Ok(db)) => {
                                let records = crate::coverage::parse_lcov_records(&lcov_text);
                                let prj_root = std::env::current_dir().unwrap_or_default();
                                let summary = crate::coverage::build_coverage_summary(&records, &db, &prj_root, &resolver);
                                send_text(&mut writer, id, &serde_json::to_string_pretty(&summary).unwrap());
                            }
                            (Err(e), _) => send_error(&mut writer, id, &format!("Error reading {lcov_path}: {e}")),
                            (_, Err(e)) => send_error(&mut writer, id, &format!("Database error: {e:?}")),
                        }
                    }

                    other => send_error(&mut writer, id, &format!("Unknown tool: {:?}", other)),
                }
            }

            "resources/list" => {
                send_response(&mut writer, id, json!({ "resources": [] }));
            }
            "prompts/list" => {
                send_response(&mut writer, id, json!({ "prompts": [] }));
            }
            "notifications/initialized" => {}
            _ => {}
        }
    }
    Ok(())
}

