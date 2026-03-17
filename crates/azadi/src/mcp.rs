use crate::apply_back::{self, ApplyBackOptions};
use crate::lookup;
use azadi_macros::evaluator::{EvalConfig, Evaluator};
use azadi_macros::macro_api::process_string;
use azadi_noweb::db::AzadiDb;
use serde_json::{json, Value};
use std::io::{self, BufRead};
use std::path::PathBuf;

pub fn run_mcp(db_path: PathBuf, gen_dir: PathBuf, eval_config: EvalConfig) -> Result<(), crate::Error> {
    let stdin = io::stdin();

    let db = if db_path.exists() {
        Some(AzadiDb::open(&db_path)?)
    } else {
        None
    };

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
                    "serverInfo": { "name": "Azadi Trace Server", "version": "0.1.0" }
                }));
            }

            "tools/list" => {
                send_response(id, json!({
                    "tools": [
                        {
                            "name": "azadi_trace",
                            "description": "Trace an output file line back to its original literate source. Returns src_file/src_line/src_col/kind. MacroArg spans include macro_name/param_name. MacroBody spans include macro_name and a def_locations array (all %def call sites). VarBinding spans include var_name and a set_locations array (all %set call sites). Use --col for sub-line token precision.",
                            "inputSchema": {
                                "type": "object",
                                "properties": {
                                    "out_file": { "type": "string", "description": "Path to the generated file" },
                                    "out_line": { "type": "integer", "description": "1-indexed line number in the generated file" },
                                    "out_col":  { "type": "integer", "description": "Byte column within the output line (0-indexed, default 0). Use to pinpoint a specific token." }
                                },
                                "required": ["out_file", "out_line"]
                            }
                        },
                        {
                            "name": "azadi_apply_back",
                            "description": "Propagate edits made in gen/ files back to the literate source. Diffs each modified gen/ file against its stored baseline, traces each changed line to its origin (noweb + macro level), and patches the literate source with oracle verification. Returns a report of what was patched, skipped, or needs manual attention.",
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
                            "name": "azadi_apply_fix",
                            "description": "Apply a source edit and verify it produces the desired output line. Use this after reading the literate source, determining a fix, and wanting to apply it safely. The tool re-evaluates the macro expander as an oracle — the edit is written only if the expected output line is produced.",
                            "inputSchema": {
                                "type": "object",
                                "properties": {
                                    "src_file":        { "type": "string",  "description": "Absolute path of the literate source file to edit" },
                                    "src_line":        { "type": "integer", "description": "1-indexed line to replace in src_file" },
                                    "new_src_line":    { "type": "string",  "description": "Replacement text for that line (without trailing newline)" },
                                    "out_file":        { "type": "string",  "description": "Generated file path (used for oracle lookup)" },
                                    "out_line":        { "type": "integer", "description": "1-indexed line in the generated file (oracle check point)" },
                                    "expected_output": { "type": "string",  "description": "The exact output line content expected after the fix (indent-stripped)" }
                                },
                                "required": ["src_file", "src_line", "new_src_line", "out_file", "out_line", "expected_output"]
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
                    Some("azadi_trace") => {
                        let Some(input) = input else {
                            send_error(id, "Missing arguments");
                            continue;
                        };
                        let out_file = input.get("out_file").and_then(|f| f.as_str()).unwrap_or("");
                        let out_line = input.get("out_line").and_then(|l| l.as_u64()).unwrap_or(0) as u32;
                        let out_col  = input.get("out_col") .and_then(|c| c.as_u64()).unwrap_or(0) as u32;

                        match &db {
                            Some(db) => match lookup::perform_trace(out_file, out_line, out_col, db, &gen_dir, eval_config.clone()) {
                                Ok(Some(res)) => send_text(id, &serde_json::to_string(&res).unwrap()),
                                Ok(None)      => send_error(id, &format!("No mapping found for {}:{}", out_file, out_line)),
                                Err(e)        => send_error(id, &format!("Lookup error: {:?}", e)),
                            },
                            None => send_error(id, "Database not found. Run azadi on your source files first."),
                        }
                    }

                    Some("azadi_apply_back") => {
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

                    Some("azadi_apply_fix") => {
                        let Some(input) = input else {
                            send_error(id, "Missing arguments");
                            continue;
                        };
                        let src_file     = input.get("src_file")       .and_then(|v| v.as_str()).unwrap_or("");
                        let src_line_1   = input.get("src_line")        .and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                        let new_src_line = input.get("new_src_line")    .and_then(|v| v.as_str()).unwrap_or("");
                        let out_file     = input.get("out_file")        .and_then(|v| v.as_str()).unwrap_or("");
                        let out_line_1   = input.get("out_line")        .and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                        let expected     = input.get("expected_output") .and_then(|v| v.as_str()).unwrap_or("");

                        if src_line_1 == 0 {
                            send_error(id, "src_line must be >= 1");
                            continue;
                        }

                        match apply_fix(src_file, src_line_1 - 1, new_src_line, out_file, out_line_1, expected, &db, &gen_dir, &eval_config) {
                            Ok(msg) => send_text(id, &msg),
                            Err(e)  => send_error(id, &e),
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

// ── azadi_apply_fix implementation ───────────────────────────────────────────

/// Read `src_file`, replace line at `src_line_0` with `new_src_line`,
/// re-evaluate with the macro expander, and check that line `out_line_1`
/// (1-indexed) of the *expanded intermediate* equals `expected`.
///
/// Uses `noweb_map` to find the intermediate line index corresponding to
/// `out_file:out_line_1`, then verifies the expanded output.
///
/// Returns `Ok(message)` on success (file written), `Err(reason)` on failure.
#[allow(clippy::too_many_arguments)]
fn apply_fix(
    src_file: &str,
    src_line_0: usize,
    new_src_line: &str,
    out_file: &str,
    out_line_1: u32,
    expected: &str,
    db: &Option<AzadiDb>,
    gen_dir: &std::path::Path,
    eval_config: &EvalConfig,
) -> Result<String, String> {
    let db = db.as_ref().ok_or("Database not found")?;

    // Resolve the noweb-level intermediate line for oracle check.
    let db_path = crate::lookup::normalize_path_pub(out_file, gen_dir);
    let nw_entry = db
        .get_noweb_entry(&db_path, out_line_1 - 1)
        .map_err(|e| format!("db error: {e}"))?
        .ok_or_else(|| format!("No noweb map entry for {}:{}", out_file, out_line_1))?;

    let expanded_line_0 = nw_entry.src_line as usize;

    // Read the source file.
    let content = std::fs::read_to_string(src_file)
        .map_err(|e| format!("Cannot read {src_file}: {e}"))?;
    let mut lines: Vec<&str> = content.lines().collect();
    if src_line_0 >= lines.len() {
        return Err(format!("src_line {} out of range (file has {} lines)", src_line_0 + 1, lines.len()));
    }

    let old_line = lines[src_line_0].to_string();
    lines[src_line_0] = new_src_line;
    let had_trailing_newline = content.ends_with('\n');
    let mut patched = lines.join("\n");
    if had_trailing_newline { patched.push('\n'); }

    // Oracle: re-evaluate and check expanded line.
    let src_path = std::path::Path::new(src_file);
    let mut evaluator = Evaluator::new(eval_config.clone());
    let expanded_bytes = process_string(&patched, Some(src_path), &mut evaluator)
        .map_err(|e| format!("Evaluation error: {e:?}"))?;
    let expanded = String::from_utf8_lossy(&expanded_bytes);

    let actual_line = expanded.lines().nth(expanded_line_0)
        .ok_or_else(|| format!("Expanded output has fewer than {} lines", expanded_line_0 + 1))?;

    if actual_line != expected {
        return Err(format!(
            "Oracle check failed — patched source produces:\n  {:?}\nexpected:\n  {:?}\nNo changes written.",
            actual_line, expected,
        ));
    }

    // Verified — write the patched file.
    std::fs::write(src_file, &patched)
        .map_err(|e| format!("Cannot write {src_file}: {e}"))?;

    Ok(format!(
        "Applied: {}:{} {:?} → {:?}\nOracle verified: expanded line {} = {:?}",
        src_file, src_line_0 + 1, old_line, new_src_line, expanded_line_0 + 1, expected,
    ))
}

// ── helpers ───────────────────────────────────────────────────────────────────

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
