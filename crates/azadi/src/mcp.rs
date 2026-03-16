use crate::lookup;
use azadi_noweb::db::AzadiDb;
use serde_json::{json, Value};
use std::io::{self, BufRead};
use std::path::PathBuf;

pub fn run_mcp(db_path: PathBuf, gen_dir: PathBuf, eval_config: azadi_macros::evaluator::EvalConfig) -> Result<(), crate::Error> {
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
                    "capabilities": {
                        "tools": {}
                    },
                    "serverInfo": {
                        "name": "Azadi Trace Server",
                        "version": "0.1.0"
                    }
                }));
            }
            "tools/list" => {
                send_response(id, json!({
                    "tools": [
                        {
                            "name": "azadi_trace",
                            "description": "Trace an output file line back to its original literate source",
                            "inputSchema": {
                                "type": "object",
                                "properties": {
                                    "out_file": { "type": "string" },
                                    "out_line": { "type": "integer" }
                                },
                                "required": ["out_file", "out_line"]
                            }
                        }
                    ]
                }));
            }
            "tools/call" => {
                let params = req.get("params").and_then(|p| p.as_object());
                let name = params.and_then(|p| p.get("name")).and_then(|n| n.as_str());
                let input = params.and_then(|p| p.get("arguments")).and_then(|a| a.as_object());

                if name == Some("azadi_trace") {
                    if let Some(input) = input {
                        let out_file = input.get("out_file").and_then(|f| f.as_str()).unwrap_or("");
                        let out_line = input.get("out_line").and_then(|l| l.as_u64()).unwrap_or(0) as u32;

                        if let Some(ref db) = db {
                            match lookup::perform_trace(out_file, out_line, db, &gen_dir, eval_config.clone()) {
                                Ok(Some(res)) => {
                                    send_response(id, json!({
                                        "content": [
                                            {
                                                "type": "text",
                                                "text": serde_json::to_string(&res).unwrap()
                                            }
                                        ]
                                    }));
                                }
                                Ok(None) => {
                                    send_response(id, json!({
                                        "isError": true,
                                        "content": [{ "type": "text", "text": format!("No mapping found for {}:{}", out_file, out_line) }]
                                    }));
                                }
                                Err(e) => {
                                    send_response(id, json!({
                                        "isError": true,
                                        "content": [{ "type": "text", "text": format!("Lookup error: {:?}", e) }]
                                    }));
                                }
                            }
                        } else {
                            send_response(id, json!({
                                "isError": true,
                                "content": [{ "type": "text", "text": "Database not found. Run expansion with --trace first." }]
                            }));
                        }
                    } else {
                        send_response(id, json!({
                            "isError": true,
                            "content": [{ "type": "text", "text": "Missing arguments" }]
                        }));
                    }
                } else {
                    send_response(id, json!({
                        "isError": true,
                        "content": [{ "type": "text", "text": format!("Unknown tool: {:?}", name) }]
                    }));
                }
            }
            "notifications/initialized" => {} 
            _ => {
                // Ignore unknown methods or return error
            }
        }
    }
    Ok(())
}

fn send_response(id: Option<Value>, result: Value) {
    let mut resp = json!({
        "jsonrpc": "2.0"
    });
    if let Some(id) = id {
        resp.as_object_mut().unwrap().insert("id".to_string(), id);
        resp.as_object_mut().unwrap().insert("result".to_string(), result);
    }
    println!("{}", serde_json::to_string(&resp).unwrap());
}
