// weaveback-api/src/lsp_runner/tests.rs
// I'd Really Rather You Didn't edit this generated file.

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

