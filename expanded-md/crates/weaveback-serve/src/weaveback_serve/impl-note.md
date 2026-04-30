# Serve Note Handler

Save-as-note endpoint for assistant suggestions.

## Save-as-note handler

`handle_save_note` receives `POST /__save_note` with `{ file, name, nth, note }` and
inserts a `[NOTE]` admonition block into the `.adoc` source immediately after
the closing `----` fence of the chunk's listing block.  This persists AI
responses as first-class literate documentation — they become part of
`section_prose` for all future context queries.

```rust
// <[serve-save-note]>=
pub(in crate::server) fn handle_save_note(mut request: Request, project_root: &Path) {
    let mut body_str = String::new();
    if request.as_reader().read_to_string(&mut body_str).is_err() {
        let _ = request.respond(json_resp(serde_json::json!({"ok":false,"error":"io_error"})));
        return;
    }
    let params: serde_json::Value = match serde_json::from_str(&body_str) {
        Ok(v) => v,
        Err(_) => {
            let _ = request.respond(json_resp(serde_json::json!({"ok":false,"error":"invalid_json"})));
            return;
        }
    };
    let file = match params["file"].as_str().filter(|s| !s.is_empty()) {
        Some(f) => f,
        None => { let _ = request.respond(json_resp(serde_json::json!({"ok":false,"error":"missing_file"}))); return; }
    };
    let name = match params["name"].as_str().filter(|s| !s.is_empty()) {
        Some(n) => n,
        None => { let _ = request.respond(json_resp(serde_json::json!({"ok":false,"error":"missing_name"}))); return; }
    };
    let nth: u32 = params["nth"].as_u64().unwrap_or(0) as u32;
    let note = match params["note"].as_str().filter(|s| !s.is_empty()) {
        Some(n) => n,
        None => { let _ = request.respond(json_resp(serde_json::json!({"ok":false,"error":"missing_note"}))); return; }
    };

    let db_path = project_root.join("weaveback.db");
    let db = match weaveback_tangle::WeavebackDb::open_read_only(&db_path) {
        Ok(d) => d,
        Err(e) => { let _ = request.respond(json_resp(serde_json::json!({"ok":false,"error":format!("{e}")}))); return; }
    };
    let entry = match db.get_chunk_def(file, name, nth) {
        Ok(Some(e)) => e,
        _ => { let _ = request.respond(json_resp(serde_json::json!({"ok":false,"error":"chunk_not_found"}))); return; }
    };

    let src_path = project_root.join(file);
    let src_text = match std::fs::read_to_string(&src_path) {
        Ok(t) => t,
        Err(e) => { let _ = request.respond(json_resp(serde_json::json!({"ok":false,"error":format!("{e}")}))); return; }
    };

    let new_content = insert_note_into_source(&src_text, entry.def_end as usize, note);

    match std::fs::write(&src_path, &new_content) {
        Ok(()) => { let _ = request.respond(json_resp(serde_json::json!({"ok":true}))); }
        Err(e) => { let _ = request.respond(json_resp(serde_json::json!({"ok":false,"error":format!("{e}")}))); }
    }
}
// @
```

