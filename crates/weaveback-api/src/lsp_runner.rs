/// LSP operation to perform.
pub enum LspCmd {
    /// Go to definition of the symbol at the given location.
    Definition { out_file: String, line: u32, col: u32 },
    /// Find all references to the symbol at the given location.
    References  { out_file: String, line: u32, col: u32 },
}
use weaveback_lsp::LspClient;
use weaveback_core::PathResolver;
use weaveback_macro::evaluator::EvalConfig;
use serde_json::json;
use std::path::{Path, PathBuf};
use crate::lookup;
use crate::coverage::open_db;

pub fn run_lsp(
    cmd: LspCmd,
    db_path: PathBuf,
    gen_dir: PathBuf,
    eval_config: EvalConfig,
    override_cmd: Option<String>,
    override_lang: Option<String>,
) -> Result<(), std::io::Error> {
    let project_root = std::env::current_dir()?;
    let db = open_db(&db_path).map_err(|e| std::io::Error::other(e.to_string()))?;
    let resolver = PathResolver::new(project_root.clone(), gen_dir);

    let sample_file = match &cmd {
        LspCmd::Definition { out_file, .. } => out_file,
        LspCmd::References  { out_file, .. } => out_file,
    };
    let ext = Path::new(sample_file).extension().and_then(|e| e.to_str()).unwrap_or("");

    let (lsp_cmd, lsp_lang) = match (override_cmd, override_lang) {
        (Some(c), Some(l)) => (c, l),
        (c, l) => {
            let (def_cmd, def_lang) = weaveback_lsp::get_lsp_config(ext)
                .ok_or_else(|| std::io::Error::other(format!("unsupported file extension: .{}", ext)))?;
            (c.unwrap_or(def_cmd), l.unwrap_or(def_lang))
        }
    };

    let mut client = LspClient::spawn(&lsp_cmd, &[], &project_root, lsp_lang)
        .map_err(|e| std::io::Error::other(format!("failed to start LSP '{}': {e}", lsp_cmd)))?;

    client.initialize(&project_root)
        .map_err(|e| std::io::Error::other(format!("LSP initialization failed: {e}")))?;

    match cmd {
        LspCmd::Definition { out_file, line, col } => {
            let path = Path::new(&out_file).canonicalize()
                .map_err(|e| std::io::Error::other(format!("invalid file path '{}': {e}", out_file)))?;

            client.did_open(&path)
                .map_err(|e| std::io::Error::other(format!("LSP didOpen failed: {e}")))?;

            let loc = client.goto_definition(&path, line - 1, col - 1)
                .map_err(|e| std::io::Error::other(format!("LSP definition call failed: {e}")))?;

            if let Some(loc) = loc {
                let target_path = loc.uri.to_file_path()
                    .map_err(|_| std::io::Error::other("LSP returned non-file URI"))?;
                let target_line = loc.range.start.line + 1;
                let target_col  = loc.range.start.character + 1;

                let trace = lookup::perform_trace(
                    &target_path.to_string_lossy(),
                    target_line, target_col,
                    &db, &resolver, eval_config,
                ).map_err(|e| std::io::Error::other(format!("Mapping failed: {e:?}")))?;

                if let Some(res) = trace {
                    println!("{}", serde_json::to_string_pretty(&res).unwrap());
                } else {
                    println!("{}", json!({
                        "out_file": target_path.to_string_lossy(),
                        "out_line": target_line,
                        "out_col":  target_col,
                        "note": "LSP result could not be mapped to source"
                    }));
                }
            } else {
                println!("No definition found.");
            }
        }

        LspCmd::References { out_file, line, col } => {
            let path = Path::new(&out_file).canonicalize()
                .map_err(|e| std::io::Error::other(format!("invalid file path '{}': {e}", out_file)))?;

            client.did_open(&path)
                .map_err(|e| std::io::Error::other(format!("LSP didOpen failed: {e}")))?;

            let locs = client.find_references(&path, line - 1, col - 1)
                .map_err(|e| std::io::Error::other(format!("LSP references call failed: {e}")))?;

            let mut results = Vec::new();
            for loc in locs {
                let target_path = loc.uri.to_file_path()
                    .map_err(|_| std::io::Error::other("LSP returned non-file URI"))?;
                let target_line = loc.range.start.line + 1;
                let target_col  = loc.range.start.character + 1;

                let trace = lookup::perform_trace(
                    &target_path.to_string_lossy(),
                    target_line, target_col,
                    &db, &resolver, eval_config.clone(),
                ).map_err(|e| std::io::Error::other(format!("Mapping failed: {e:?}")))?;

                if let Some(res) = trace {
                    results.push(res);
                } else {
                    results.push(json!({
                        "out_file": target_path.to_string_lossy(),
                        "out_line": target_line,
                        "out_col":  target_col,
                        "note": "LSP result could not be mapped to source"
                    }));
                }
            }
            println!("{}", serde_json::to_string_pretty(&results).unwrap());
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lsp_cmd_definition_fields_accessible() {
        let cmd = LspCmd::Definition {
            out_file: "foo.rs".to_string(),
            line: 10,
            col: 5,
        };
        match cmd {
            LspCmd::Definition { out_file, line, col } => {
                assert_eq!(out_file, "foo.rs");
                assert_eq!(line, 10);
                assert_eq!(col, 5);
            }
            _ => panic!("unexpected variant"),
        }
    }

    #[test]
    fn lsp_cmd_references_fields_accessible() {
        let cmd = LspCmd::References {
            out_file: "bar.rs".to_string(),
            line: 3,
            col: 1,
        };
        match cmd {
            LspCmd::References { out_file, line, col } => {
                assert_eq!(out_file, "bar.rs");
                assert_eq!(line, 3);
                assert_eq!(col, 1);
            }
            _ => panic!("unexpected variant"),
        }
    }

    #[test]
    fn run_lsp_errors_when_db_not_found() {
        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("nonexistent.db");
        let cmd = LspCmd::Definition {
            out_file: "foo.rs".to_string(),
            line: 1,
            col: 1,
        };
        let err = run_lsp(
            cmd,
            db_path,
            tmp.path().to_path_buf(),
            weaveback_macro::evaluator::EvalConfig::default(),
            None,
            None,
        )
        .unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("not found") || msg.contains("Database"),
            "unexpected error: {msg}"
        );
    }
    #[test]
    fn test_run_lsp_with_mock() {
        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("wb.db");
        let gen_dir = tmp.path().join("gen");
        std::fs::create_dir_all(&gen_dir).unwrap();

        // Seed some files and DB
        let out_file = gen_dir.join("test.rs");
        std::fs::write(&out_file, "fn main() {}").unwrap();

        let mut ws_db = weaveback_tangle::WeavebackDb::open(&db_path).unwrap();
        // Seed a mapping so perform_trace has something to find
        ws_db.set_noweb_entries(
            out_file.to_str().unwrap(),
            &[(1, weaveback_tangle::NowebMapEntry {
                src_file: "test.adoc".to_string(),
                chunk_name: "test".to_string(),
                src_line: 1,
                indent: "".to_string(),
                confidence: weaveback_tangle::db::Confidence::Exact,
            })]
        ).unwrap();

        let script = r#"
import sys, json

def read_msg():
    line = sys.stdin.readline()
    if not line: return None
    if not line.startswith("Content-Length:"): return None
    l = int(line.split(":")[1].strip())
    sys.stdin.readline() # \r\n
    body = sys.stdin.read(l)
    return json.loads(body)

def write_msg(d):
    j = json.dumps(d)
    sys.stdout.write(f"Content-Length: {len(j)}\r\n\r\n{j}")
    sys.stdout.flush()

while True:
    try:
        m = read_msg()
        if not m: break
        if "id" in m:
            if m["method"] == "initialize":
                write_msg({"jsonrpc": "2.0", "id": m["id"], "result": {"capabilities": {"definitionProvider": True, "referencesProvider": True}}})
            elif m["method"] == "textDocument/definition":
                uri = f"file://{sys.argv[1]}"
                write_msg({"jsonrpc": "2.0", "id": m["id"], "result": {"uri": uri, "range": {"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 5}}}})
            elif m["method"] == "textDocument/references":
                uri = f"file://{sys.argv[1]}"
                write_msg({"jsonrpc": "2.0", "id": m["id"], "result": [{"uri": uri, "range": {"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 5}}}]})
    except Exception:
        break
"#;
        let script_path = tmp.path().join("mock_lsp.py");
        std::fs::write(&script_path, script).unwrap();

        let cmd = LspCmd::Definition {
            out_file: out_file.to_str().unwrap().to_string(),
            line: 1,
            col: 1,
        };

        let res = run_lsp(
            cmd,
            db_path.clone(),
            gen_dir.clone(),
            weaveback_macro::evaluator::EvalConfig::default(),
            Some(format!("python3 {} {}", script_path.to_str().unwrap(), out_file.to_str().unwrap())),
            Some("fake".to_string()),
        );
        assert!(res.is_ok(), "run_lsp failed: {:?}", res.err());

        // Test References too
        let cmd = LspCmd::References {
            out_file: out_file.to_str().unwrap().to_string(),
            line: 1,
            col: 1,
        };
        let res = run_lsp(
            cmd,
            db_path,
            gen_dir,
            weaveback_macro::evaluator::EvalConfig::default(),
            Some(format!("python3 {} {}", script_path.to_str().unwrap(), out_file.to_str().unwrap())),
            Some("fake".to_string()),
        );
        assert!(res.is_ok(), "run_lsp (refs) failed: {:?}", res.err());
    }
}
