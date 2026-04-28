// weaveback-api/src/mcp/helpers.rs
// I'd Really Rather You Didn't edit this generated file.

use serde_json::{json, Value};
use std::io::Write;

pub(super) fn send_response<W: Write>(writer: &mut W, id: Option<Value>, result: Value) {
    let mut resp = json!({ "jsonrpc": "2.0" });
    if let Some(id) = id {
        resp.as_object_mut().unwrap().insert("id".to_string(), id);
        resp.as_object_mut().unwrap().insert("result".to_string(), result);
    }
    let _ = writeln!(writer, "{}", serde_json::to_string(&resp).unwrap());
}

pub(super) fn send_text<W: Write>(writer: &mut W, id: Option<Value>, text: &str) {
    send_response(writer, id, json!({
        "content": [{ "type": "text", "text": text }]
    }));
}

pub(super) fn send_error<W: Write>(writer: &mut W, id: Option<Value>, msg: &str) {
    send_response(writer, id, json!({
        "isError": true,
        "content": [{ "type": "text", "text": msg }]
    }));
}

