#[cfg(feature = "server")]
use std::collections::HashMap;
#[cfg(feature = "server")]
use std::io::{BufRead, Read};
use std::path::Path;
#[cfg(feature = "server")]
use std::path::PathBuf;
#[cfg(feature = "server")]
use std::sync::{Arc, Mutex};
#[cfg(feature = "server")]
use std::thread;

#[cfg(feature = "server")]
use notify::{RecursiveMode, Watcher};
#[cfg(feature = "server")]
use tiny_http::{Header, Request, Response, Server, StatusCode};
#[cfg(feature = "server")]
use weaveback_tangle::tangle_check;
#[cfg(feature = "server")]
struct SseReader {
    rx: std::sync::mpsc::Receiver<()>,
    buf: Vec<u8>,
    pos: usize,
}

#[cfg(feature = "server")]
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

#[cfg(feature = "server")]
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
#[cfg(feature = "server")]
type SseSenders = Arc<Mutex<Vec<std::sync::mpsc::SyncSender<()>>>>;

#[cfg(feature = "server")]
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
#[cfg(feature = "server")]
fn find_docgen_bin() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        let sibling = exe.with_file_name("weaveback-docgen");
        if sibling.exists() { return sibling; }
    }
    PathBuf::from("weaveback-docgen")
}

#[cfg(feature = "server")]
fn run_rebuild(project_root: &Path, tangle: bool, theme: bool) {
    if tangle {
        eprintln!("weaveback serve --watch: tangle...");
        let exe = std::env::current_exe()
            .unwrap_or_else(|_| PathBuf::from("weaveback"));
        let ok = std::process::Command::new(&exe)
            .arg("tangle")
            .current_dir(project_root)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !ok { eprintln!("weaveback serve --watch: tangle failed"); return; }
    }
    if theme {
        eprintln!("weaveback serve --watch: theme...");
        let ok = std::process::Command::new("node")
            .arg(project_root.join("scripts").join("serve-ui").join("build.mjs"))
            .current_dir(project_root)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !ok { eprintln!("weaveback serve --watch: theme build failed"); return; }
    }
    eprintln!("weaveback serve --watch: docs...");
    let _ = std::process::Command::new(find_docgen_bin())
        .args(["--special", "%", "--special", "^"])
        .current_dir(project_root)
        .status();
}

#[cfg(feature = "server")]
fn spawn_source_watcher(project_root: PathBuf) {
    use std::time::Duration;
    thread::spawn(move || {
        let (tx, rx) = std::sync::mpsc::channel();
        let mut watcher = match notify::recommended_watcher(tx) {
            Ok(w) => w,
            Err(e) => { eprintln!("weaveback serve: source watcher error: {e}"); return; }
        };
        if let Err(e) = watcher.watch(&project_root, RecursiveMode::Recursive) {
            eprintln!("weaveback serve: source watch error: {e}");
            return;
        }
        let docs_html  = project_root.join("docs").join("html");
        let target_dir = project_root.join("target");
        let theme_src  = project_root.join("scripts").join("serve-ui").join("src");
        while let Ok(first) = rx.recv() {
            let mut need_tangle = false;
            let mut need_theme  = false;
            if let Ok(event) = first {
                for p in &event.paths {
                    if p.starts_with(&docs_html) || p.starts_with(&target_dir) { continue; }
                    if p.extension().is_some_and(|e| e == "adoc") { need_tangle = true; }
                    if p.starts_with(&theme_src) { need_theme = true; }
                }
            }
            while let Ok(Ok(event)) = rx.recv_timeout(Duration::from_millis(500)) {
                for p in &event.paths {
                    if p.starts_with(&docs_html) || p.starts_with(&target_dir) { continue; }
                    if p.extension().is_some_and(|e| e == "adoc") { need_tangle = true; }
                    if p.starts_with(&theme_src) { need_theme = true; }
                }
            }
            if need_tangle || need_theme {
                run_rebuild(&project_root, need_tangle, need_theme);
            }
        }
        drop(watcher);
    });
}
#[cfg(feature = "server")]
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

#[cfg(feature = "server")]
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

#[cfg(feature = "server")]
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
#[cfg(feature = "server")]
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

#[cfg(feature = "server")]
fn parse_query(url: &str) -> HashMap<String, String> {
    let query = url.split_once('?').map(|x| x.1).unwrap_or("");
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

#[cfg(feature = "server")]
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
#[cfg(feature = "server")]
#[derive(Clone, Debug)]
pub enum AiBackend {
    /// Shells out to `claude -p --output-format stream-json`.
    /// Uses the existing Claude Code session; no API key required.
    ClaudeCli,
    /// Calls the Anthropic API directly via HTTP.
    /// Requires the `ANTHROPIC_API_KEY` environment variable.
    Anthropic,
    /// Calls the Google Gemini API directly via HTTP.
    /// Requires the `GOOGLE_API_KEY` environment variable.
    Gemini,
    /// Calls a local Ollama API via HTTP.
    Ollama,
    /// Calls an OpenAI-compatible API via HTTP.
    /// Requires the `OPENAI_API_KEY` environment variable (if not using a local provider).
    OpenAi,
}

#[cfg(feature = "server")]
pub struct TangleConfig {
    pub open_delim:      String,
    pub close_delim:     String,
    pub chunk_end:       String,
    pub comment_markers: Vec<String>,
    pub ai_backend:      AiBackend,
    pub ai_model:        Option<String>,
    pub ai_endpoint:     Option<String>,
}

#[cfg(feature = "server")]
impl Default for TangleConfig {
    fn default() -> Self {
        Self {
            open_delim:      "<[".into(),
            close_delim:     "]>".into(),
            chunk_end:       "@@".into(),
            comment_markers: vec!["//".into()],
            ai_backend:      AiBackend::ClaudeCli,
            ai_model:        None,
            ai_endpoint:     None,
        }
    }
}
#[cfg(feature = "server")]
fn json_resp(val: serde_json::Value) -> Response<std::io::Cursor<Vec<u8>>> {
    Response::from_string(val.to_string())
        .with_header(Header::from_bytes("Content-Type", "application/json").unwrap())
        .with_header(Header::from_bytes("Access-Control-Allow-Origin", "*").unwrap())
}

#[cfg(feature = "server")]
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

#[cfg(feature = "server")]
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
#[cfg(feature = "server")]
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
#[cfg(feature = "server")]
use std::process::Stdio;

// ── AsciiDoc source helpers ───────────────────────────────────────────────────

/// Return the heading depth if `line` is an AsciiDoc `=`-style heading
/// (1 = `=`, 2 = `==`, …), otherwise `None`.
pub(crate) fn heading_level(line: &str) -> Option<usize> {
    let t = line.trim_end();
    if t.is_empty() { return None; }
    let count = t.bytes().take_while(|&b| b == b'=').count();
    if count > 0 && t.len() > count && t.as_bytes()[count] == b' ' {
        Some(count)
    } else {
        None
    }
}

/// Find the `(start, end)` line range (0-based, end exclusive) of the AsciiDoc
/// section that contains line `def_start`.  The section starts at the nearest
/// heading above `def_start` and ends just before the next heading at the same
/// or shallower nesting level.
pub(crate) fn section_range(lines: &[&str], def_start: usize) -> (usize, usize) {
    let mut sec_start = 0usize;
    let mut sec_level = 1usize;
    for i in (0..def_start).rev() {
        if let Some(level) = heading_level(lines[i]) {
            sec_start = i;
            sec_level = level;
            break;
        }
    }
    let sec_end = lines[def_start..]
        .iter()
        .enumerate()
        .skip(1)
        .find(|(_, l)| heading_level(l).map(|lvl| lvl <= sec_level).unwrap_or(false))
        .map(|(i, _)| def_start + i)
        .unwrap_or(lines.len());
    (sec_start, sec_end)
}

/// Build the heading breadcrumb trail leading to `def_start`.
/// Returns titles from outermost to innermost, e.g.
/// `["Module overview", "Parsing", "Error recovery"]`.
pub(crate) fn title_chain(lines: &[&str], def_start: usize) -> Vec<String> {
    let mut chain: Vec<(usize, String)> = Vec::new();
    for line in lines.iter().take(def_start) {
        if let Some(level) = heading_level(line) {
            let title = line[level + 1..].trim().to_string();
            chain.retain(|(l, _)| *l < level);
            chain.push((level, title));
        }
    }
    chain.into_iter().map(|(_, t)| t).collect()
}

/// Extract all prose lines from `lines[start..end]`, skipping content inside
/// `----` listing-block fences.  The result is the human-written narrative
/// of the section — headings, paragraphs, admonitions, lists — without any
/// code.
pub(crate) fn extract_prose(lines: &[&str], start: usize, end: usize) -> String {
    let end = end.min(lines.len());
    let mut in_fence = false;
    let mut out: Vec<&str> = Vec::new();
    for l in lines.iter().take(end).skip(start) {
        if l.trim() == "----" { in_fence = !in_fence; continue; }
        if !in_fence { out.push(l); }
    }
    // Trim leading/trailing blank lines.
    while out.first().map(|l| l.trim().is_empty()).unwrap_or(false) { out.remove(0); }
    while out.last().map(|l| l.trim().is_empty()).unwrap_or(false) { out.pop(); }
    out.join("\n")
}

/// Return the body text of each direct dependency of `chunk_name`.
/// Keys are chunk names; values are `{ "file": "…", "body": "…" }`.
pub(crate) fn dep_bodies(
    db: &weaveback_tangle::WeavebackDb,
    project_root: &Path,
    dep_names: &[(String, String)],
) -> serde_json::Map<String, serde_json::Value> {
    let mut map = serde_json::Map::new();
    for (dep_name, _) in dep_names {
        let defs = match db.find_chunk_defs_by_name(dep_name) {
            Ok(d) if !d.is_empty() => d,
            _ => continue,
        };
        let def = &defs[0];
        let src_path = project_root.join(&def.src_file);
        let src_text = match std::fs::read_to_string(&src_path) {
            Ok(t) => t,
            Err(_) => continue,
        };
        let src_lines: Vec<&str> = src_text.lines().collect();
        let s = def.def_start as usize;
        let e = def.def_end as usize;
        let body = if s < src_lines.len() && e <= src_lines.len() && e > 0 {
            src_lines[s..e - 1].join("\n")
        } else {
            String::new()
        };
        map.insert(dep_name.clone(), serde_json::json!({
            "file": def.src_file,
            "body": body,
        }));
    }
    map
}

/// Return recent `git log --oneline` entries for `src_file`.
pub(crate) fn git_log_for_file(project_root: &Path, src_file: &str) -> Vec<String> {
    let root = project_root.to_string_lossy();
    match std::process::Command::new("git")
        .args(["-C", &root, "log", "--follow", "-n", "5", "--oneline", "--", src_file])
        .output()
    {
        Ok(o) if o.status.success() =>
            String::from_utf8_lossy(&o.stdout).lines().map(|l| l.to_string()).collect(),
        _ => Vec::new(),
    }
}

// ── Context builder ───────────────────────────────────────────────────────────

pub(crate) fn build_chunk_context(
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

    // Chunk body (lines between the open and close markers).
    let body = if def_start < src_lines.len() && def_end <= src_lines.len() && def_end > 0 {
        src_lines[def_start..def_end - 1].join("\n")
    } else {
        String::new()
    };

    // Section context: title breadcrumb + full prose of the enclosing section.
    let chain  = title_chain(&src_lines, def_start);
    let (sec_start, sec_end) = section_range(&src_lines, def_start);
    let section_prose = extract_prose(&src_lines, sec_start, sec_end);

    // Dependency graph.
    let raw_deps: Vec<(String, String)> = db.query_chunk_deps(name).unwrap_or_default();
    let dep_map = dep_bodies(&db, project_root, &raw_deps);
    let rev_deps: Vec<String> = db.query_reverse_deps(name)
        .unwrap_or_default()
        .into_iter().map(|(from, _)| from).collect();
    let output_files: Vec<String> = db.query_chunk_output_files(name).unwrap_or_default();

    // Recent git history for this source file.
    let log = git_log_for_file(project_root, file);

    serde_json::json!({
        "file":                 file,
        "name":                 name,
        "nth":                  nth,
        "body":                 body,
        "def_start":            entry.def_start,
        "def_end":              entry.def_end,
        "section_title_chain":  chain,
        "section_prose":        section_prose,
        "dependencies":         serde_json::Value::Object(dep_map),
        "reverse_dependencies": rev_deps,
        "output_files":         output_files,
        "git_log":              log,
    })
}

#[cfg(feature = "server")]
struct AiChannelReader {
    rx:  std::sync::mpsc::Receiver<String>,
    buf: Vec<u8>,
    pos: usize,
}

#[cfg(feature = "server")]
impl AiChannelReader {
    fn new(rx: std::sync::mpsc::Receiver<String>) -> Self {
        // Prime with a keepalive comment so EventSource confirms the connection.
        Self { rx, buf: b": weaveback-ai\n\n".to_vec(), pos: 0 }
    }
}

#[cfg(feature = "server")]
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

#[cfg(feature = "server")]
fn sse_headers() -> Vec<Header> {
    vec![
        Header::from_bytes("Content-Type",               "text/event-stream").unwrap(),
        Header::from_bytes("Cache-Control",              "no-cache").unwrap(),
        Header::from_bytes("X-Accel-Buffering",          "no").unwrap(),
        Header::from_bytes("Access-Control-Allow-Origin","*").unwrap(),
    ]
}

/// Call `claude -p --output-format stream-json --verbose` as a subprocess.
///
/// Uses the existing Claude Code session credentials — no API key needed.
/// The system context is appended to the default Claude Code system prompt via
/// `--append-system-prompt`.  `user_content` is passed as the `-p` argument.
///
/// The `stream-json --verbose` format emits one JSON object per line.
/// We handle two event types:
/// * `type == "assistant"` — message with `message.content[].text` fields;
///   send each text chunk as a token event.
/// * `type == "result"` — final summary; stop reading.
#[cfg(feature = "server")]
fn call_claude_cli(
    system_prompt: String,
    user_content: String,
    tx: std::sync::mpsc::Sender<String>,
) {
    let mut child = match std::process::Command::new("claude")
        .args([
            "-p", &user_content,
            "--output-format", "stream-json",
            "--verbose",
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
        if v["type"] == "assistant" {
            // Content is an array of blocks; we want text blocks.
            if let Some(content) = v["message"]["content"].as_array() {
                for block in content {
                    if block["type"] == "text"
                        && let Some(text) = block["text"].as_str()
                        && !text.is_empty() {
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
#[cfg(feature = "server")]
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
        if v["type"] == "content_block_delta"
            && let Some(text) = v["delta"]["text"].as_str()
            && !text.is_empty() {
            let data = serde_json::json!({"t": text}).to_string();
            if tx.send(format!("event: token\ndata: {data}\n\n")).is_err() {
                return;
            }
        } else if v["type"] == "message_stop" {
            break;
        }
    }
    let _ = tx.send("event: done\ndata:\n\n".to_string());
}

/// Call the Google Gemini API directly via HTTP.
///
/// Requires `GOOGLE_API_KEY`. Uses the `streamGenerateContent` endpoint.
#[cfg(feature = "server")]
fn call_gemini_api(
    api_key: String,
    model: String,
    system_prompt: String,
    user_content: String,
    tx: std::sync::mpsc::Sender<String>,
) {
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:streamGenerateContent?key={}",
        model, api_key
    );

    let body = serde_json::json!({
        "contents": [
            {
                "role": "user",
                "parts": [{ "text": format!("System: {}\n\n{}", system_prompt, user_content) }]
            }
        ],
        "generationConfig": {
            "maxOutputTokens": 1024,
        }
    });

    let resp = match ureq::AgentBuilder::new()
        .build()
        .post(&url)
        .set("Content-Type", "application/json")
        .send_json(&body)
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
        let trimmed = line.trim();
        if trimmed.is_empty() { continue; }

        // Gemini stream format is a JSON array of objects, but delivered as individual
        // chunks. Sometimes it starts with '[' and ends with ']'.
        let clean = trimmed.trim_start_matches(',').trim_start_matches('[').trim_end_matches(']');
        if clean.is_empty() { continue; }

        let v: serde_json::Value = match serde_json::from_str(clean) {
            Ok(v) => v,
            Err(_) => continue,
        };

        if let Some(text) = v["candidates"][0]["content"]["parts"][0]["text"].as_str() {
            let data = serde_json::json!({"t": text}).to_string();
            if tx.send(format!("event: token\ndata: {data}\n\n")).is_err() {
                return;
            }
        }
    }
    let _ = tx.send("event: done\ndata:\n\n".to_string());
}

/// Call a local Ollama API via HTTP.
///
/// Uses the `/api/chat` endpoint with `stream: true`.
#[cfg(feature = "server")]
fn call_ollama_api(
    base_url: String,
    model: String,
    system_prompt: String,
    user_content: String,
    tx: std::sync::mpsc::Sender<String>,
) {
    let url = format!("{}/api/chat", base_url.trim_end_matches('/'));
    let body = serde_json::json!({
        "model": model,
        "messages": [
            { "role": "system", "content": system_prompt },
            { "role": "user", "content": user_content }
        ],
        "stream": true,
    });

    let resp = match ureq::AgentBuilder::new()
        .build()
        .post(&url)
        .set("Content-Type", "application/json")
        .send_json(&body)
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
        let trimmed = line.trim();
        if trimmed.is_empty() { continue; }

        let v: serde_json::Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(_) => continue,
        };

        if let Some(text) = v["message"]["content"].as_str() {
            let data = serde_json::json!({"t": text}).to_string();
            if tx.send(format!("event: token\ndata: {data}\n\n")).is_err() {
                return;
            }
        }
        if v["done"].as_bool().unwrap_or(false) {
            break;
        }
    }
    let _ = tx.send("event: done\ndata:\n\n".to_string());
}

/// Call an OpenAI-compatible API directly via HTTP.
///
/// Handles standard Chat Completions streaming format.
#[cfg(feature = "server")]
fn call_openai_api(
    api_key: Option<String>,
    base_url: String,
    model: String,
    system_prompt: String,
    user_content: String,
    tx: std::sync::mpsc::Sender<String>,
) {
    let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));
    let body = serde_json::json!({
        "model": model,
        "messages": [
            { "role": "system", "content": system_prompt },
            { "role": "user", "content": user_content }
        ],
        "stream": true,
    });

    let mut req = ureq::AgentBuilder::new()
        .build()
        .post(&url)
        .set("Content-Type", "application/json");

    if let Some(key) = api_key {
        req = req.set("Authorization", &format!("Bearer {}", key));
    }

    let resp = match req.send_json(&body) {
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
        let trimmed = line.trim();
        if !trimmed.starts_with("data: ") { continue; }
        let data_str = &trimmed[6..];
        if data_str == "[DONE]" { break; }

        let v: serde_json::Value = match serde_json::from_str(data_str) {
            Ok(v) => v,
            Err(_) => continue,
        };

        if let Some(text) = v["choices"][0]["delta"]["content"].as_str() {
            let data = serde_json::json!({"t": text}).to_string();
            if tx.send(format!("event: token\ndata: {data}\n\n")).is_err() {
                return;
            }
        }
    }
    let _ = tx.send("event: done\ndata:\n\n".to_string());
}

#[cfg(feature = "server")]
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
        AiBackend::Anthropic => {
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
            let model = cfg.ai_model.clone().unwrap_or_else(|| "claude-3-5-sonnet-20240620".to_string());
            let api_body = serde_json::json!({
                "model":    model,
                "max_tokens": 1024,
                "stream":   true,
                "system":   system_prompt,
                "messages": [{ "role": "user", "content": user_content }],
            });
            thread::spawn(move || call_anthropic_api(api_key, api_body, tx));
        }
        AiBackend::Gemini => {
            let api_key = match std::env::var("GOOGLE_API_KEY") {
                Ok(k) if !k.is_empty() => k,
                _ => {
                    let _ = request.respond(json_resp(serde_json::json!({
                        "ok": false,
                        "error": "no_api_key: set GOOGLE_API_KEY env var"
                    })));
                    return;
                }
            };
            let model = cfg.ai_model.clone().unwrap_or_else(|| "gemini-1.5-pro".to_string());
            thread::spawn(move || call_gemini_api(api_key, model, system_prompt, user_content, tx));
        }
        AiBackend::Ollama => {
            let base_url = cfg.ai_endpoint.clone().unwrap_or_else(|| "http://localhost:11434".to_string());
            let model = cfg.ai_model.clone().unwrap_or_else(|| "llama3".to_string());
            thread::spawn(move || call_ollama_api(base_url, model, system_prompt, user_content, tx));
        }
        AiBackend::OpenAi => {
            let api_key = std::env::var("OPENAI_API_KEY").ok();
            let base_url = cfg.ai_endpoint.clone().unwrap_or_else(|| "https://api.openai.com/v1".to_string());
            let model = cfg.ai_model.clone().unwrap_or_else(|| "gpt-4o".to_string());
            thread::spawn(move || call_openai_api(api_key, base_url, model, system_prompt, user_content, tx));
        }
    }

    let reader = AiChannelReader::new(rx);
    let response = Response::new(StatusCode(200), sse_headers(), reader, None, None);
    let _ = request.respond(response);
}
#[cfg(feature = "server")]
fn handle_save_note(mut request: Request, project_root: &Path) {
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

    let lines: Vec<&str> = src_text.lines().collect();
    // def_end is 1-indexed; lines[def_end - 1] = chunk end marker.
    // lines[def_end] (0-indexed) should be the closing "----" fence.
    let def_end_0 = entry.def_end as usize;
    let insert_after = if def_end_0 < lines.len() && lines[def_end_0].trim() == "----" {
        def_end_0 + 1
    } else {
        def_end_0
    };

    let had_trailing_newline = src_text.ends_with('\n');
    let before = lines[..insert_after].join("\n");
    let after  = if insert_after < lines.len() { lines[insert_after..].join("\n") } else { String::new() };
    let note_block = format!("\n[NOTE]\n====\n{}\n====\n", note.trim());
    let mut new_content = if after.is_empty() {
        format!("{}{}", before, note_block)
    } else {
        format!("{}{}\n{}", before, note_block, after)
    };
    if had_trailing_newline && !new_content.ends_with('\n') { new_content.push('\n'); }

    match std::fs::write(&src_path, &new_content) {
        Ok(()) => { let _ = request.respond(json_resp(serde_json::json!({"ok":true}))); }
        Err(e) => { let _ = request.respond(json_resp(serde_json::json!({"ok":false,"error":format!("{e}")}))); }
    }
}
#[cfg(feature = "server")]
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

    if url == "/__save_note" {
        handle_save_note(request, project_root);
        return;
    }

    serve_static(request, &url, html_dir);
}
#[cfg(feature = "server")]
fn find_project_root() -> PathBuf {
    let mut dir = std::env::current_dir().expect("cannot determine cwd");
    loop {
        let cargo_toml = dir.join("Cargo.toml");
        if cargo_toml.exists()
            && let Ok(content) = std::fs::read_to_string(&cargo_toml)
            && content.contains("[workspace]") {
            return dir;
        }
        if !dir.pop() {
            break;
        }
    }
    std::env::current_dir().unwrap()
}

#[cfg(feature = "server")]
pub fn run_serve(
    port: u16,
    html_override: Option<PathBuf>,
    tangle_cfg: TangleConfig,
    watch: bool,
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
    if watch {
        spawn_source_watcher(project_root.clone());
    }

    let addr = format!("127.0.0.1:{port}");
    let server = Server::http(&addr).map_err(|e| e.to_string())?;

    let tangle_cfg = Arc::new(tangle_cfg);

    println!("weaveback serve: http://127.0.0.1:{port}/");
    println!("  Serving: {}", html_dir.display());
    println!("  Editor:  $VISUAL / $EDITOR ({})",
        std::env::var("VISUAL")
            .or_else(|_| std::env::var("EDITOR"))
            .unwrap_or_else(|_| "vi (fallback)".into()));
    if watch {
        println!("  Watch:   .adoc + theme sources (tangle + docs on change)");
    }
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
