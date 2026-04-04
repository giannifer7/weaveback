use crate::apply_back::{self, ApplyBackOptions};
use crate::lookup;
use crate::serve::build_chunk_context;
use weaveback_macro::evaluator::{EvalConfig, Evaluator};
use weaveback_macro::macro_api::process_string;
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

pub fn run_mcp(db_path: PathBuf, gen_dir: PathBuf, eval_config: EvalConfig) -> Result<(), crate::Error> {
    let stdin = io::stdin();
    let mut lsp_clients: HashMap<String, LspClient> = HashMap::new();
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
                send_response(id, json!({
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
                                        "description": "BM25 full-text search over the prose in all literate source files. Returns ranked excerpts with file path and line range. Use this to discover which chunks or sections are relevant to a concept before calling weaveback_chunk_context. Supports FTS5 query syntax: AND, OR, NOT, phrase \"...\", prefix foo*.",
                                        "inputSchema": {
                                            "type": "object",
                                            "properties": {
                                                "query": { "type": "string", "description": "Search terms (FTS5 syntax)" },
                                                "limit": { "type": "integer", "description": "Maximum results to return (default 10)" }
                                            },
                                            "required": ["query"]
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
                        match WeavebackDb::open_read_only(&db_path) {
                            Err(e) => send_error(id, &format!("Database error: {e:?}")),
                            Ok(db) => match lookup::perform_trace(out_file, out_line, out_col, &db, &resolver, eval_config.clone()) {
                                Ok(Some(res)) => send_text(id, &serde_json::to_string(&res).unwrap()),
                                Ok(None)      => send_error(id, &format!("No mapping found for {}:{}", out_file, out_line)),
                                Err(e)        => send_error(id, &format!("Lookup error: {:?}", e)),
                            },
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
                            send_error(id, "src_line must be >= 1");
                            continue;
                        }
                        if src_line_end_1 < src_line_1 {
                            send_error(id, "src_line_end must be >= src_line");
                            continue;
                        }

                        let db = if db_path.exists() { WeavebackDb::open_read_only(&db_path).ok() } else { None };
                        match apply_fix(src_file, src_line_1, src_line_end_1, &new_lines, out_file, out_line_1, expected, &db, &resolver, &eval_config) {
                            Ok(msg) => send_text(id, &msg),
                            Err(e)  => send_error(id, &e),
                        }
                    }

                    Some("weaveback_chunk_context") => {
                        let Some(input) = input else {
                            send_error(id, "Missing arguments");
                            continue;
                        };
                        let file = input.get("file").and_then(|v| v.as_str()).unwrap_or("");
                        let name = input.get("name").and_then(|v| v.as_str()).unwrap_or("");
                        let nth  = input.get("nth").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                        if file.is_empty() || name.is_empty() {
                            send_error(id, "file and name are required");
                            continue;
                        }
                        let project_root = db_path.parent().unwrap_or(std::path::Path::new("."));
                        let ctx = build_chunk_context(project_root, file, name, nth);
                        if ctx.is_null() {
                            send_error(id, &format!("Chunk not found: {}#{}[{}]", file, name, nth));
                        } else {
                            send_text(id, &serde_json::to_string_pretty(&ctx).unwrap());
                        }
                    }

                    Some("weaveback_list_chunks") => {
                        let file_filter = input
                            .and_then(|i| i.get("file"))
                            .and_then(|v| v.as_str());
                        if !db_path.exists() {
                            send_error(id, "Database not found. Run weaveback on your source files first.");
                            continue;
                        }
                        match WeavebackDb::open_read_only(&db_path) {
                            Err(e) => send_error(id, &format!("Database error: {e:?}")),
                            Ok(db) => match db.list_chunk_defs(file_filter) {
                                Err(e) => send_error(id, &format!("Query error: {e:?}")),
                                Ok(defs) => {
                                    let arr: Vec<Value> = defs.iter().map(|d| json!({
                                        "file":      d.src_file,
                                        "name":      d.chunk_name,
                                        "nth":       d.nth,
                                        "def_start": d.def_start,
                                        "def_end":   d.def_end,
                                    })).collect();
                                    send_text(id, &serde_json::to_string_pretty(&arr).unwrap());
                                }
                            },
                        }
                    }

                    Some("weaveback_find_chunk") => {
                        let Some(input) = input else {
                            send_error(id, "Missing arguments");
                            continue;
                        };
                        let name = input.get("name").and_then(|v| v.as_str()).unwrap_or("");
                        if name.is_empty() {
                            send_error(id, "name is required");
                            continue;
                        }
                        if !db_path.exists() {
                            send_error(id, "Database not found. Run weaveback on your source files first.");
                            continue;
                        }
                        match WeavebackDb::open_read_only(&db_path) {
                            Err(e) => send_error(id, &format!("Database error: {e:?}")),
                            Ok(db) => match db.find_chunk_defs_by_name(name) {
                                Err(e) => send_error(id, &format!("Query error: {e:?}")),
                                Ok(defs) => {
                                    let arr: Vec<Value> = defs.iter().map(|d| json!({
                                        "file":      d.src_file,
                                        "nth":       d.nth,
                                        "def_start": d.def_start,
                                        "def_end":   d.def_end,
                                    })).collect();
                                    send_text(id, &serde_json::to_string_pretty(&arr).unwrap());
                                }
                            },
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
                        let query = input.as_ref()
                            .and_then(|v| v.get("query"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        if query.is_empty() {
                            send_error(id, "query is required");
                            continue;
                        }
                        let limit = input.as_ref()
                            .and_then(|v| v.get("limit"))
                            .and_then(|v| v.as_u64())
                            .unwrap_or(10) as usize;
                        if !db_path.exists() {
                            send_error(id, "Database not found. Run weaveback on your source files first.");
                            continue;
                        }
                        match WeavebackDb::open_read_only(&db_path) {
                            Err(e) => send_error(id, &format!("Database error: {e:?}")),
                            Ok(db) => match db.search_prose(query, limit) {
                                Err(e) => send_error(id, &format!("Search error: {e:?}")),
                                Ok(results) => {
                                    let arr: Vec<Value> = results.iter().map(|r| json!({
                                        "src_file":   r.src_file,
                                        "block_type": r.block_type,
                                        "line_start": r.line_start,
                                        "line_end":   r.line_end,
                                        "snippet":    r.snippet,
                                    })).collect();
                                    send_text(id, &serde_json::to_string_pretty(&arr).unwrap());
                                }
                            },
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

/// Read `src_file`, replace lines `src_line_1..=src_line_end_1` (1-indexed) with
/// `new_lines`, re-evaluate with the macro expander, and check that line
/// `out_line_1` (1-indexed) of the *expanded intermediate* equals `expected`.
///
/// Returns `Ok(message)` on success (file written), `Err(reason)` on failure.
#[allow(clippy::too_many_arguments)]
fn apply_fix(
    src_file: &str,
    src_line_1: usize,
    src_line_end_1: usize,
    new_lines: &[String],
    out_file: &str,
    out_line_1: u32,
    expected: &str,
    db: &Option<WeavebackDb>,
    resolver: &PathResolver,
    eval_config: &EvalConfig,
) -> Result<String, String> {
    let db = db.as_ref().ok_or("Database not found")?;

    // Resolve the noweb-level intermediate line for oracle check.
    let db_path = resolver.normalize(out_file);
    let nw_entry = db
        .get_noweb_entry(&db_path, out_line_1 - 1)
        .map_err(|e| format!("db error: {e}"))?
        .ok_or_else(|| format!("No noweb map entry for {}:{}", out_file, out_line_1))?;

    let expanded_line_1 = nw_entry.src_line as usize + 1; // keep 1-indexed for reporting

    // Read the source file.
    let content = std::fs::read_to_string(src_file)
        .map_err(|e| format!("Cannot read {src_file}: {e}"))?;
    let orig_lines: Vec<&str> = content.lines().collect();
    let file_len = orig_lines.len();

    if src_line_1 > file_len {
        return Err(format!("src_line {src_line_1} out of range (file has {file_len} lines)"));
    }
    if src_line_end_1 > file_len {
        return Err(format!("src_line_end {src_line_end_1} out of range (file has {file_len} lines)"));
    }

    let lo = src_line_1 - 1;
    let hi = src_line_end_1 - 1;
    let removed: Vec<String> = orig_lines[lo..=hi].iter().map(|s| s.to_string()).collect();

    let patched_lines: Vec<&str> = orig_lines[..lo]
        .iter().copied()
        .chain(new_lines.iter().map(|s| s.as_str()))
        .chain(orig_lines[hi + 1..].iter().copied())
        .collect();

    let had_trailing_newline = content.ends_with('\n');
    let mut patched = patched_lines.join("\n");
    if had_trailing_newline { patched.push('\n'); }

    // Oracle: re-evaluate and check expanded line.
    let oracle_path = std::path::Path::new(src_file).with_file_name("<oracle>");

    // Retrieve configuration used when this file was tangled.
    let mut oracle_config = eval_config.clone();
    if let Ok(Some(cfg)) = weaveback_tangle::lookup::find_best_source_config(db, src_file) {
        oracle_config.special_char = cfg.special_char;
    }

    let mut evaluator = Evaluator::new(oracle_config);
    let expanded_bytes = process_string(&patched, Some(&oracle_path), &mut evaluator)
        .map_err(|e| format!("Evaluation error: {e:?}"))?;
    let expanded = String::from_utf8_lossy(&expanded_bytes);

    let actual_line = expanded.lines().nth(expanded_line_1 - 1)
        .ok_or_else(|| format!("Expanded output has fewer than {expanded_line_1} lines"))?;

    if actual_line != expected {
        return Err(format!(
            "Oracle check failed — patched source produces:\n  {:?}\nexpected:\n  {:?}\nNo changes written.",
            actual_line, expected,
        ));
    }

    // Verified — write the patched file.
    std::fs::write(src_file, &patched)
        .map_err(|e| format!("Cannot write {src_file}: {e}"))?;

    let range_desc = if src_line_1 == src_line_end_1 {
        format!("{}:{}", src_file, src_line_1)
    } else {
        format!("{}:{}-{}", src_file, src_line_1, src_line_end_1)
    };
    Ok(format!(
        "Applied: {} — replaced {} line(s) with {} line(s)\n  old: {}\n  new: {}\nOracle verified: expanded line {} = {:?}",
        range_desc,
        removed.len(),
        new_lines.len(),
        removed.iter().map(|s| format!("{:?}", s)).collect::<Vec<_>>().join(", "),
        new_lines.iter().map(|s| format!("{:?}", s)).collect::<Vec<_>>().join(", "),
        expanded_line_1,
        expected,
    ))
}

fn send_response(id: Option<Value>, result: Value) {
    let mut resp = json!({ "jsonrpc": "2.0" });
    if let Some(id) = id {
        resp.as_object_mut().unwrap().insert("id".to_string(), id);
        resp.as_object_mut().unwrap().insert("result".to_string(), result);
    }
    println!("{}", serde_json::to_string(&resp).unwrap());
}

fn send_text(id: Option<Value>, text: &str) {
    send_response(id, json!({
        "content": [{ "type": "text", "text": text }]
    }));
}

fn send_error(id: Option<Value>, msg: &str) {
    send_response(id, json!({
        "isError": true,
        "content": [{ "type": "text", "text": msg }]
    }));
}
