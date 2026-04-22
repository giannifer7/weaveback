// weaveback-lsp/src/tests.rs
// I'd Really Rather You Didn't edit this generated file.

use super::*;
use std::io::Write;

#[test]
fn test_get_lsp_config() {
    assert_eq!(get_lsp_config("rs"), Some(("rust-analyzer".to_string(), "rust".to_string())));
    assert_eq!(get_lsp_config("nim"), Some(("nimlsp".to_string(), "nim".to_string())));
    assert_eq!(get_lsp_config("py"), Some(("pyright-langserver --stdio".to_string(), "python".to_string())));
    assert_eq!(get_lsp_config("xyz"), None);
}

#[test]
fn test_lsp_error_display() {
    let err = LspError::Protocol("invalid root".into());
    assert_eq!(err.to_string(), "LSP error: invalid root");
}

#[test]
fn test_fake_lsp_lifecycle() {
    // A minimal Python script that reads JSON-RPC loop and responds
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

msg1 = read_msg()
if msg1 and "id" in msg1:
    write_msg({"jsonrpc": "2.0", "id": msg1["id"], "result": {"capabilities": {"definitionProvider": True}}})

msg2 = read_msg()
"#;

    let mut tmp_file = tempfile::NamedTempFile::new().unwrap();
    write!(tmp_file, "{}", script).unwrap();
    let path = tmp_file.path().to_owned();

    let mut client = LspClient::spawn("python3", &[path.to_str().unwrap()], std::env::current_dir().unwrap().as_path(), "fake".into()).unwrap();
    assert!(client.is_alive());

    let result = client.initialize(std::env::current_dir().unwrap().as_path());
    assert!(result.is_ok());
}

