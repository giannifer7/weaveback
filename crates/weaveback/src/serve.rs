use std::collections::HashMap;
use std::io::{BufRead, Read};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;

use notify::{RecursiveMode, Watcher};
use tiny_http::{Header, Request, Response, Server, StatusCode};
use weaveback_tangle::tangle_check;
struct SseReader {
    rx: std::sync::mpsc::Receiver<()>,
    buf: Vec<u8>,
    pos: usize,
}

impl SseReader {
    fn new(rx: std::sync::mpsc::Receiver<()>) -> Self {
        // Prime the buffer with a keepalive comment so the SSE connection is
        // established immediately.
        Self {
            rx,
            buf: b": weaveback-serve\n\n".to_vec(),
            pos: 0,
        }
    }
}

impl Read for SseReader {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        loop {
            if self.pos < self.buf.len() {
                let n = out.len().min(self.buf.len() - self.pos);
                out[..n].copy_from_slice(&self.buf[self.pos..self.pos + n]);
                self.pos += n;
                return Ok(n);
            }
            // Buffer exhausted — wait for the next reload signal.
            match self.rx.recv() {
                Ok(()) => {
                    self.buf = b"event: reload\ndata:\n\n".to_vec();
                    self.pos = 0;
                }
                Err(_) => return Ok(0), // sender dropped → EOF
            }
        }
    }
}
type SseSenders = Arc<Mutex<Vec<std::sync::mpsc::SyncSender<()>>>>;

fn spawn_watcher(watch_dir: PathBuf, senders: SseSenders) {
    thread::spawn(move || {
        let (tx, rx) = std::sync::mpsc::channel();
        let mut watcher = match notify::recommended_watcher(tx) {
            Ok(w) => w,
            Err(e) => { eprintln!("weaveback serve: watcher error: {e}"); return; }
        };
        if let Err(e) = watcher.watch(&watch_dir, RecursiveMode::Recursive) {
            eprintln!("weaveback serve: watch error: {e}");
            return;
        }
        for result in &rx {
            if result.is_ok() {
                let mut locked = senders.lock().unwrap();
                locked.retain(|s| s.send(()).is_ok());
            }
        }
        drop(watcher);
    });
}
fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("css")  => "text/css; charset=utf-8",
        Some("js")   => "application/javascript; charset=utf-8",
        Some("svg")  => "image/svg+xml",
        Some("png")  => "image/png",
        Some("ico")  => "image/x-icon",
        Some("json") => "application/json",
        _            => "application/octet-stream",
    }
}

fn safe_path(html_dir: &Path, url_path: &str) -> Option<PathBuf> {
    let rel = url_path.trim_start_matches('/');
    if rel.split('/').any(|c| c == "..") {
        return None;
    }
    let path = html_dir.join(rel);
    if path.is_dir() {
        let idx = path.join("index.html");
        if idx.exists() { Some(idx) } else { None }
    } else if path.exists() {
        Some(path)
    } else {
        None
    }
}

fn serve_static(request: Request, url: &str, html_dir: &Path) {
    let url_path = url.split('?').next().unwrap_or(url);

    // Redirect bare "/" to "/docs/index.html" so the browser's base URL is
    // correct and relative asset paths inside the HTML resolve properly.
    if url_path == "/" {
        let _ = request.respond(
            Response::from_string("")
                .with_status_code(302)
                .with_header(Header::from_bytes("Location", "/docs/index.html").unwrap()),
        );
        return;
    }

    match safe_path(html_dir, url_path) {
        None => {
            let _ = request.respond(Response::from_string("404 Not Found").with_status_code(404));
        }
        Some(path) => {
            let ct = content_type(&path);
            match std::fs::read(&path) {
                Ok(bytes) => {
                    let response = Response::from_data(bytes)
                        .with_header(
                            Header::from_bytes("Content-Type", ct).unwrap()
                        );
                    let _ = request.respond(response);
                }
                Err(_) => {
                    let _ = request.respond(
                        Response::from_string("500 Internal Server Error")
                            .with_status_code(500)
                    );
                }
            }
        }
    }
}
fn open_in_editor(file: &str, line: u32, project_root: &Path) {
    let editor = std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .unwrap_or_else(|_| "vi".to_string());
    let editor_base = Path::new(&editor)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase();
    let full = project_root.join(file);
    let full_str = full.to_string_lossy().into_owned();
    let args: Vec<String> = match editor_base.as_str() {
        "code" | "code-insiders" => {
            vec!["--goto".into(), format!("{}:{}", full_str, line)]
        }
        _ => vec![format!("+{}", line), full_str],
    };
    let _ = std::process::Command::new(&editor).args(&args).spawn();
}

fn parse_query(url: &str) -> HashMap<String, String> {
    let query = url.splitn(2, '?').nth(1).unwrap_or("");
    query
        .split('&')
        .filter_map(|pair| {
            let mut it = pair.splitn(2, '=');
            let k = it.next()?;
            let v = it.next().unwrap_or("");
            Some((
                percent_decode(k),
                percent_decode(v),
            ))
        })
        .collect()
}

fn percent_decode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut bytes = s.bytes();
    while let Some(b) = bytes.next() {
        if b == b'%' {
            let hi = bytes.next().and_then(|c| char::from(c).to_digit(16));
            let lo = bytes.next().and_then(|c| char::from(c).to_digit(16));
            if let (Some(h), Some(l)) = (hi, lo) {
                out.push(char::from((h * 16 + l) as u8));
                continue;
            }
        }
        out.push(char::from(b));
    }
    out
}
/// Which backend `/__ai` uses to answer questions.
///
/// * `ClaudeCli` (default) — shells out to `claude -p --output-format
///   stream-json`.  Uses the existing Claude Code session; no API key required.
/// * `Api` — calls the Anthropic API directly via HTTP.  Requires the
///   `ANTHROPIC_API_KEY` environment variable.
pub enum AiBackend {
    ClaudeCli,
    Api,
}

pub struct TangleConfig {
    pub open_delim:      String,
    pub close_delim:     String,
    pub chunk_end:       String,
    pub comment_markers: Vec<String>,
    pub ai_backend:      AiBackend,
}

impl Default for TangleConfig {
    fn default() -> Self {
        Self {
            open_delim:      "<[".into(),
            close_delim:     "]>".into(),
            chunk_end:       "@@".into(),
            comment_markers: vec!["//".into()],
            ai_backend:      AiBackend::ClaudeCli,
        }
    }
}
fn json_resp(val: serde_json::Value) -> Response<std::io::Cursor<Vec<u8>>> {
    Response::from_string(val.to_string())
        .with_header(Header::from_bytes("Content-Type", "application/json").unwrap())
        .with_header(Header::from_bytes("Access-Control-Allow-Origin", "*").unwrap())
}

fn tangle_oracle(
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

fn handle_apply(mut request: Request, project_root: &Path, cfg: &TangleConfig) {
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

    let src_lines: Vec<&str> = src_text.lines().collect();
    let def_start = entry.def_start as usize; // 1-indexed header line
    let def_end   = entry.def_end   as usize; // 1-indexed close-marker line

    // Body occupies lines[def_start .. def_end-1] (0-indexed, exclusive end).
    if def_start >= src_lines.len() || def_end > src_lines.len() {
        let _ = request.respond(json_resp(serde_json::json!({
            "ok": false, "error": "bounds_error"
        })));
        return;
    }

    let body_lines = &src_lines[def_start .. def_end - 1];
    let actual_body = body_lines.join("\n");

    if actual_body != old_body.trim_end_matches('\n') {
        let _ = request.respond(json_resp(serde_json::json!({
            "ok": false, "error": "body_mismatch"
        })));
        return;
    }

    // Build modified source text.
    let new_body_trimmed = new_body.trim_end_matches('\n');
    let new_body_lines: Vec<&str> = new_body_trimmed.lines().collect();
    let mut new_src: Vec<&str> = Vec::new();
    new_src.extend_from_slice(&src_lines[..def_start]);
    new_src.extend_from_slice(&new_body_lines);
    new_src.extend_from_slice(&src_lines[def_end - 1..]);
    let new_src_text = new_src.join("\n") + "\n";

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

    let src_lines: Vec<&str> = src_text.lines().collect();
    let def_start = entry.def_start as usize; // 1-indexed header line
    let def_end   = entry.def_end   as usize; // 1-indexed close-marker line

    if def_start >= src_lines.len() || def_end > src_lines.len() {
        let _ = request.respond(json_resp(serde_json::json!({
            "ok": false, "error": "bounds_error"
        })));
        return;
    }

    let body = src_lines[def_start .. def_end - 1].join("\n");

    let _ = request.respond(json_resp(serde_json::json!({
        "ok":        true,
        "body":      body,
        "def_start": entry.def_start,
        "def_end":   entry.def_end,
    })));
}
use std::process::Stdio;

fn build_chunk_context(
    project_root: &Path,
    file: &str,
    name: &str,
    nth: u32,
) -> serde_json::Value {
    let db_path = project_root.join("weaveback.db");
    let db = match weaveback_tangle::WeavebackDb::open_read_only(&db_path) {
        Ok(d) => d,
        Err(_) => return serde_json::Value::Null,
    };
    let entry = match db.get_chunk_def(file, name, nth) {
        Ok(Some(e)) => e,
        _ => return serde_json::Value::Null,
    };
    let src_path = project_root.join(file);
    let src_text = match std::fs::read_to_string(&src_path) {
        Ok(t) => t,
        Err(_) => return serde_json::Value::Null,
    };
    let src_lines: Vec<&str> = src_text.lines().collect();
    let def_start = entry.def_start as usize;
    let def_end   = entry.def_end   as usize;

    let body = if def_start < src_lines.len() && def_end <= src_lines.len() {
        src_lines[def_start..def_end - 1].join("\n")
    } else {
        String::new()
    };

    // prose_before: up to 8 lines immediately before the chunk header
    let before_end   = def_start.saturating_sub(1);
    let before_start = before_end.saturating_sub(8);
    let prose_before = src_lines[before_start..before_end].join("\n");

    // prose_after: up to 4 lines immediately after the chunk end-marker
    let after_start = def_end.min(src_lines.len());
    let after_end   = (after_start + 4).min(src_lines.len());
    let prose_after = src_lines[after_start..after_end].join("\n");

    let deps: Vec<String> = db.query_chunk_deps(name)
        .unwrap_or_default()
        .into_iter().map(|(to, _)| to).collect();
    let rev_deps: Vec<String> = db.query_reverse_deps(name)
        .unwrap_or_default()
        .into_iter().map(|(from, _)| from).collect();
    let output_files: Vec<String> = db.query_chunk_output_files(name)
        .unwrap_or_default();

    serde_json::json!({
        "file":                 file,
        "name":                 name,
        "nth":                  nth,
        "body":                 body,
        "def_start":            entry.def_start,
        "def_end":              entry.def_end,
        "prose_before":         prose_before,
        "prose_after":          prose_after,
        "dependencies":         deps,
        "reverse_dependencies": rev_deps,
        "output_files":         output_files,
    })
}

struct AiChannelReader {
    rx:  std::sync::mpsc::Receiver<String>,
    buf: Vec<u8>,
    pos: usize,
}

impl AiChannelReader {
    fn new(rx: std::sync::mpsc::Receiver<String>) -> Self {
        // Prime with a keepalive comment so EventSource confirms the connection.
        Self { rx, buf: b": weaveback-ai\n\n".to_vec(), pos: 0 }
    }
}

impl Read for AiChannelReader {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        loop {
            if self.pos < self.buf.len() {
                let n = out.len().min(self.buf.len() - self.pos);
                out[..n].copy_from_slice(&self.buf[self.pos..self.pos + n]);
                self.pos += n;
                return Ok(n);
            }
            match self.rx.recv() {
                Ok(s) => { self.buf = s.into_bytes(); self.pos = 0; }
                Err(_) => return Ok(0),
            }
        }
    }
}

fn sse_headers() -> Vec<Header> {
    vec![
        Header::from_bytes("Content-Type",               "text/event-stream").unwrap(),
        Header::from_bytes("Cache-Control",              "no-cache").unwrap(),
        Header::from_bytes("X-Accel-Buffering",          "no").unwrap(),
        Header::from_bytes("Access-Control-Allow-Origin","*").unwrap(),
    ]
}

/// Call `claude -p --output-format stream-json` as a subprocess.
///
/// Uses the existing Claude Code session credentials — no API key needed.
/// The system context is appended to the default Claude Code system prompt via
/// `--append-system-prompt`.  `user_content` is passed as the `-p` argument.
fn call_claude_cli(
    system_prompt: String,
    user_content: String,
    tx: std::sync::mpsc::Sender<String>,
) {
    let mut child = match std::process::Command::new("claude")
        .args([
            "-p", &user_content,
            "--output-format", "stream-json",
            "--append-system-prompt", &system_prompt,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            let msg = format!(
                "event: error\ndata: {}\n\nevent: done\ndata:\n\n",
                serde_json::json!({"error": format!("cannot spawn claude: {e}")})
            );
            let _ = tx.send(msg);
            return;
        }
    };

    let stdout = child.stdout.take().expect("piped stdout");
    let mut reader = std::io::BufReader::new(stdout);
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        let trimmed = line.trim();
        if trimmed.is_empty() { continue; }
        let v: serde_json::Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if v["type"] == "stream_event" {
            if v["event"]["delta"]["type"] == "text_delta" {
                if let Some(text) = v["event"]["delta"]["text"].as_str() {
                    if !text.is_empty() {
                        let data = serde_json::json!({"t": text}).to_string();
                        if tx.send(format!("event: token\ndata: {data}\n\n")).is_err() {
                            let _ = child.kill();
                            return;
                        }
                    }
                }
            }
        } else if v["type"] == "result" {
            break;
        }
    }
    let _ = child.wait();
    let _ = tx.send("event: done\ndata:\n\n".to_string());
}

/// Call the Anthropic Messages API directly.
///
/// Requires `ANTHROPIC_API_KEY`.  Parses the native Anthropic SSE stream
/// (`content_block_delta` events) and forwards text deltas to the channel.
fn call_anthropic_api(
    api_key: String,
    api_body: serde_json::Value,
    tx: std::sync::mpsc::Sender<String>,
) {
    let resp = match ureq::AgentBuilder::new()
        .build()
        .post("https://api.anthropic.com/v1/messages")
        .set("x-api-key", &api_key)
        .set("anthropic-version", "2023-06-01")
        .set("content-type", "application/json")
        .send_json(&api_body)
    {
        Ok(r) => r,
        Err(e) => {
            let msg = format!(
                "event: error\ndata: {}\n\nevent: done\ndata:\n\n",
                serde_json::json!({"error": format!("{e}")})
            );
            let _ = tx.send(msg);
            return;
        }
    };

    let mut reader = std::io::BufReader::new(resp.into_reader());
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        let trimmed = line.trim_end_matches('\n').trim_end_matches('\r');
        if !trimmed.starts_with("data: ") { continue; }
        let json_str = &trimmed["data: ".len()..];
        if json_str == "[DONE]" { break; }
        let v: serde_json::Value = match serde_json::from_str(json_str) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if v["type"] == "content_block_delta" {
            if let Some(text) = v["delta"]["text"].as_str() {
                if !text.is_empty() {
                    let data = serde_json::json!({"t": text}).to_string();
                    if tx.send(format!("event: token\ndata: {data}\n\n")).is_err() {
                        return;
                    }
                }
            }
        } else if v["type"] == "message_stop" {
            break;
        }
    }
    let _ = tx.send("event: done\ndata:\n\n".to_string());
}

fn handle_ai(mut request: Request, project_root: &Path, cfg: &TangleConfig) {
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

    let question = params["question"].as_str().unwrap_or("").to_string();
    if question.is_empty() {
        let _ = request.respond(json_resp(serde_json::json!({"ok":false,"error":"missing_question"})));
        return;
    }

    let file = params["file"].as_str().unwrap_or("").to_string();
    let name = params["name"].as_str().unwrap_or("").to_string();
    let nth: u32 = params["nth"].as_u64().unwrap_or(0) as u32;

    let context = if !file.is_empty() && !name.is_empty() {
        build_chunk_context(project_root, &file, &name, nth)
    } else {
        serde_json::Value::Null
    };

    let system_prompt =
        "You are an AI assistant embedded in weaveback, a literate programming toolchain. \
         You help the user understand, improve, and debug code chunks in their literate \
         source documents (.adoc files with noweb-style chunk definitions). \
         When you propose a code edit, output it as a fenced code block. \
         Be concise and precise. \
         If chunk context is provided, use it to ground your answer."
        .to_string();

    let user_content = if context.is_null() {
        question.clone()
    } else {
        format!(
            "Chunk context:\n```json\n{}\n```\n\nQuestion: {}",
            serde_json::to_string_pretty(&context).unwrap_or_default(),
            question,
        )
    };

    let (tx, rx) = std::sync::mpsc::channel::<String>();

    match cfg.ai_backend {
        AiBackend::ClaudeCli => {
            thread::spawn(move || call_claude_cli(system_prompt, user_content, tx));
        }
        AiBackend::Api => {
            let api_key = match std::env::var("ANTHROPIC_API_KEY") {
                Ok(k) if !k.is_empty() => k,
                _ => {
                    let _ = request.respond(json_resp(serde_json::json!({
                        "ok": false,
                        "error": "no_api_key: set ANTHROPIC_API_KEY env var"
                    })));
                    return;
                }
            };
            let api_body = serde_json::json!({
                "model":    "claude-sonnet-4-6",
                "max_tokens": 1024,
                "stream":   true,
                "system":   system_prompt,
                "messages": [{ "role": "user", "content": user_content }],
            });
            thread::spawn(move || call_anthropic_api(api_key, api_body, tx));
        }
    }

    let reader = AiChannelReader::new(rx);
    let response = Response::new(StatusCode(200), sse_headers(), reader, None, None);
    let _ = request.respond(response);
}
fn handle_request(
    request: Request,
    html_dir: &Path,
    senders: &SseSenders,
    project_root: &Path,
    tangle_cfg: &TangleConfig,
) {
    let url = request.url().to_string();

    if url == "/__events" || url.starts_with("/__events?") {
        let (tx, rx) = std::sync::mpsc::sync_channel(4);
        senders.lock().unwrap().push(tx);
        let reader = SseReader::new(rx);
        let response = Response::new(
            StatusCode(200),
            vec![
                Header::from_bytes("Content-Type", "text/event-stream").unwrap(),
                Header::from_bytes("Cache-Control", "no-cache").unwrap(),
                Header::from_bytes("X-Accel-Buffering", "no").unwrap(),
            ],
            reader,
            None,
            None,
        );
        let _ = request.respond(response);
        return;
    }

    if url.starts_with("/__open") {
        let params = parse_query(&url);
        let file = params.get("file").map(|s| s.as_str()).unwrap_or("");
        let line: u32 = params
            .get("line")
            .and_then(|s| s.parse().ok())
            .unwrap_or(1);
        if !file.is_empty() {
            open_in_editor(file, line, project_root);
        }
        let response = Response::from_string("ok")
            .with_header(Header::from_bytes("Access-Control-Allow-Origin", "*").unwrap());
        let _ = request.respond(response);
        return;
    }

    if url == "/__apply" || url.starts_with("/__apply?") {
        handle_apply(request, project_root, tangle_cfg);
        return;
    }

    if url.starts_with("/__chunk") {
        handle_chunk(request, &url, project_root);
        return;
    }

    if url == "/__ai" || url.starts_with("/__ai?") {
        handle_ai(request, project_root, tangle_cfg);
        return;
    }

    serve_static(request, &url, html_dir);
}
fn find_project_root() -> PathBuf {
    let mut dir = std::env::current_dir().expect("cannot determine cwd");
    loop {
        let cargo_toml = dir.join("Cargo.toml");
        if cargo_toml.exists() {
            if let Ok(content) = std::fs::read_to_string(&cargo_toml) {
                if content.contains("[workspace]") {
                    return dir;
                }
            }
        }
        if !dir.pop() {
            break;
        }
    }
    std::env::current_dir().unwrap()
}

pub fn run_serve(
    port: u16,
    html_override: Option<PathBuf>,
    tangle_cfg: TangleConfig,
) -> Result<(), String> {
    let project_root = find_project_root();
    let html_dir = html_override.unwrap_or_else(|| project_root.join("docs").join("html"));
    let html_dir = if html_dir.exists() {
        html_dir.canonicalize().map_err(|e| e.to_string())?
    } else {
        return Err(format!(
            "docs directory not found: {}\nRun `just docs` first to generate the HTML documentation.",
            html_dir.display()
        ));
    };

    let senders: SseSenders = Arc::new(Mutex::new(Vec::new()));
    spawn_watcher(html_dir.clone(), senders.clone());

    let addr = format!("127.0.0.1:{port}");
    let server = Server::http(&addr).map_err(|e| e.to_string())?;

    let tangle_cfg = Arc::new(tangle_cfg);

    println!("weaveback serve: http://127.0.0.1:{port}/");
    println!("  Serving: {}", html_dir.display());
    println!("  Editor:  $VISUAL / $EDITOR ({})",
        std::env::var("VISUAL")
            .or_else(|_| std::env::var("EDITOR"))
            .unwrap_or_else(|_| "vi (fallback)".into()));
    println!("  Press Ctrl-C to stop.");

    for request in server.incoming_requests() {
        let html_dir2     = html_dir.clone();
        let senders2      = senders.clone();
        let root2         = project_root.clone();
        let cfg2          = tangle_cfg.clone();
        thread::spawn(move || {
            handle_request(request, &html_dir2, &senders2, &root2, &cfg2);
        });
    }

    Ok(())
}
