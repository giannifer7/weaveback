// weaveback-serve/src/server/source_edit.rs
// I'd Really Rather You Didn't edit this generated file.

use super::*;

pub fn apply_chunk_edit(src_text: &str, def_start: usize, def_end: usize, new_body: &str) -> String {
    let src_lines: Vec<&str> = src_text.lines().collect();
    if def_start >= src_lines.len() || def_end > src_lines.len() {
        return src_text.to_string();
    }
    let new_body_trimmed = new_body.trim_end_matches('\n');
    let new_body_lines: Vec<&str> = new_body_trimmed.lines().collect();
    let mut new_src: Vec<&str> = Vec::new();
    new_src.extend_from_slice(&src_lines[..def_start]);
    new_src.extend_from_slice(&new_body_lines);
    new_src.extend_from_slice(&src_lines[def_end - 1..]);
    let mut res = new_src.join("\n");
    if src_text.ends_with('\n') {
        res.push('\n');
    }
    res
}

pub fn extract_chunk_body(src_text: &str, def_start: usize, def_end: usize) -> Result<String, String> {
    let src_lines: Vec<&str> = src_text.lines().collect();
    if def_start >= src_lines.len() || def_end > src_lines.len() {
        return Err("bounds_error".to_string());
    }
    Ok(src_lines[def_start .. def_end - 1].join("\n"))
}

pub fn insert_note_into_source(src_text: &str, def_end: usize, note: &str) -> String {
    let lines: Vec<&str> = src_text.lines().collect();
    let insert_after = if def_end < lines.len() && lines[def_end].trim() == "----" {
        def_end + 1
    } else {
        def_end
    };

    let before = lines[..insert_after].join("\n");
    let after  = if insert_after < lines.len() { lines[insert_after..].join("\n") } else { String::new() };
    let note_block = format!("\n[NOTE]\n====\n{}\n====\n", note.trim());
    let mut new_content = if after.is_empty() {
        format!("{}{}", before, note_block)
    } else {
        format!("{}{}\n{}", before, note_block, after)
    };
    if src_text.ends_with('\n') && !new_content.ends_with('\n') {
        new_content.push('\n');
    }
    new_content
}
pub(crate) fn json_resp(val: serde_json::Value) -> Response<std::io::Cursor<Vec<u8>>> {
    Response::from_string(val.to_string())
        .with_header(Header::from_bytes("Content-Type", "application/json").unwrap())
        .with_header(Header::from_bytes("Access-Control-Allow-Origin", "*").unwrap())
}

pub(crate) fn tangle_oracle(
    project_root: &Path,
    modified_file: &str,
    new_content: &str,
    cfg: &TangleConfig,
) -> Result<(), String> {
    let modified_path = project_root.join(modified_file);
    let dir = match modified_path.parent() {
        Some(d) => d.to_path_buf(),
        None => return Err("cannot determine file directory".to_string()),
    };
    let ext = modified_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("adoc");

    // Load all files with the same extension from the same directory,
    // replacing the modified file with the new content.
    let mut texts: Vec<(String, String)> = Vec::new();
    let entries = match std::fs::read_dir(&dir) {
        Ok(e) => e,
        Err(e) => return Err(format!("io_error: {e}")),
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some(ext) {
            continue;
        }
        let rel = path
            .strip_prefix(project_root)
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|_| path.to_string_lossy().into_owned());
        if rel == modified_file {
            texts.push((new_content.to_string(), rel));
        } else if let Ok(content) = std::fs::read_to_string(&path) {
            texts.push((content, rel));
        }
    }

    let pairs: Vec<(&str, &str)> = texts
        .iter()
        .map(|(c, f)| (c.as_str(), f.as_str()))
        .collect();
    tangle_check(&pairs, &cfg.open_delim, &cfg.close_delim, &cfg.chunk_end, &cfg.comment_markers)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

pub(in crate::server) fn handle_apply(mut request: Request, project_root: &Path, cfg: &TangleConfig) {
    // Read and parse request body.
    let mut body_str = String::new();
    if let Err(e) = request.as_reader().read_to_string(&mut body_str) {
        let _ = request.respond(json_resp(serde_json::json!({
            "ok": false, "error": format!("io_error: {e}")
        })));
        return;
    }
    let params: serde_json::Value = match serde_json::from_str(&body_str) {
        Ok(v) => v,
        Err(e) => {
            let _ = request.respond(json_resp(serde_json::json!({
                "ok": false, "error": format!("invalid_json: {e}")
            })));
            return;
        }
    };

    let file     = params["file"].as_str().unwrap_or("").to_string();
    let name     = params["name"].as_str().unwrap_or("").to_string();
    let nth: u32 = params["nth"].as_u64().unwrap_or(0) as u32;
    let old_body = params["old_body"].as_str().unwrap_or("").to_string();
    let new_body = params["new_body"].as_str().unwrap_or("").to_string();

    if file.is_empty() || name.is_empty() {
        let _ = request.respond(json_resp(serde_json::json!({
            "ok": false, "error": "missing_params"
        })));
        return;
    }

    // Reject path traversal.
    if file.contains("..") || std::path::Path::new(&file).is_absolute() {
        let _ = request.respond(json_resp(serde_json::json!({
            "ok": false, "error": "invalid_path"
        })));
        return;
    }

    // Look up chunk bounds in the database.
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

    // Read source file.
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

    let def_start = entry.def_start as usize; // 1-indexed header line
    let def_end   = entry.def_end   as usize; // 1-indexed close-marker line

    let actual_body = match extract_chunk_body(&src_text, def_start, def_end) {
        Ok(b) => b,
        Err(e) => {
            let _ = request.respond(json_resp(serde_json::json!({
                "ok": false, "error": e
            })));
            return;
        }
    };

    if actual_body != old_body.trim_end_matches('\n') {
        let _ = request.respond(json_resp(serde_json::json!({
            "ok": false, "error": "body_mismatch"
        })));
        return;
    }

    // Build modified source text.
    let new_src_text = apply_chunk_edit(&src_text, def_start, def_end, &new_body);

    // Tangle oracle: verify the edit does not break expansion.
    if let Err(msg) = tangle_oracle(project_root, &file, &new_src_text, cfg) {
        let _ = request.respond(json_resp(serde_json::json!({
            "ok": false, "error": format!("tangle_failed: {msg}")
        })));
        return;
    }

    // Commit: write the modified source file.
    if let Err(e) = std::fs::write(&src_path, new_src_text.as_bytes()) {
        let _ = request.respond(json_resp(serde_json::json!({
            "ok": false, "error": format!("write_error: {e}")
        })));
        return;
    }

    let _ = request.respond(json_resp(serde_json::json!({ "ok": true })));
}

