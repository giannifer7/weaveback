# Serve Chunk Context

Chunk body lookup and source-context extraction helpers.

## Chunk content endpoint

`GET /__chunk` returns the current body of a named chunk and its line bounds,
so the browser inline editor can pre-fill a textarea before posting an edit.

Query parameters: `file` (adoc path relative to project root), `name` (chunk
name as stored in `chunk_defs`), `nth` (0-based definition index, default 0).

Response JSON on success:

```json
{ "ok": true, "body": "line1\nline2\n...", "def_start": 42, "def_end": 47 }
```

`body` contains lines `def_start+1` through `def_end-1` (1-indexed), joined
with `\n` (no trailing newline).  `def_start` and `def_end` are the 1-indexed
line numbers of the chunk header and close marker, stored verbatim from the DB.

```rust
// <[serve-chunk]>=
fn handle_chunk(request: Request, url: &str, project_root: &Path) {
    let params = parse_query(url);
    let file = params.get("file").map(|s| s.as_str()).unwrap_or("").to_string();
    let name = params.get("name").map(|s| s.as_str()).unwrap_or("").to_string();
    let nth: u32 = params.get("nth").and_then(|s| s.parse().ok()).unwrap_or(0);

    if file.is_empty() || name.is_empty() {
        let _ = request.respond(json_resp(serde_json::json!({
            "ok": false, "error": "missing_params"
        })));
        return;
    }
    if file.contains("..") || std::path::Path::new(&file).is_absolute() {
        let _ = request.respond(json_resp(serde_json::json!({
            "ok": false, "error": "invalid_path"
        })));
        return;
    }

    let db_path = project_root.join("weaveback.db");
    let db = match weaveback_tangle::WeavebackDb::open_read_only(&db_path) {
        Ok(d) => d,
        Err(e) => {
            let _ = request.respond(json_resp(serde_json::json!({
                "ok": false, "error": format!("db_error: {e}")
            })));
            return;
        }
    };
    let entry = match db.get_chunk_def(&file, &name, nth) {
        Ok(Some(e)) => e,
        Ok(None) => {
            let _ = request.respond(json_resp(serde_json::json!({
                "ok": false, "error": "chunk_not_found"
            })));
            return;
        }
        Err(e) => {
            let _ = request.respond(json_resp(serde_json::json!({
                "ok": false, "error": format!("db_error: {e}")
            })));
            return;
        }
    };

    let src_path = project_root.join(&file);
    let src_text = match std::fs::read_to_string(&src_path) {
        Ok(t) => t,
        Err(e) => {
            let _ = request.respond(json_resp(serde_json::json!({
                "ok": false, "error": format!("io_error: {e}")
            })));
            return;
        }
    };

    let def_start = entry.def_start as usize;
    let def_end   = entry.def_end   as usize;

    let body = match extract_chunk_body(&src_text, def_start, def_end) {
        Ok(b) => b,
        Err(e) => {
            let _ = request.respond(json_resp(serde_json::json!({
                "ok": false, "error": e
            })));
            return;
        }
    };

    let _ = request.respond(json_resp(serde_json::json!({
        "ok":        true,
        "body":      body,
        "def_start": entry.def_start,
        "def_end":   entry.def_end,
    })));
}
// @
```

