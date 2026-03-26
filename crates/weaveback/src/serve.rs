use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};
use std::thread;

use tiny_http::{Header, Request, Response, Server, StatusCode};
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
fn collect_mtimes(dir: &Path) -> HashMap<PathBuf, SystemTime> {
    let mut map = HashMap::new();
    let Ok(entries) = std::fs::read_dir(dir) else { return map };
    for entry in entries.flatten() {
        let path = entry.path();
        if let Ok(meta) = path.metadata() {
            if let Ok(mtime) = meta.modified() {
                map.insert(path.clone(), mtime);
            }
        }
        if path.is_dir() {
            map.extend(collect_mtimes(&path));
        }
    }
    map
}

type SseSenders = Arc<Mutex<Vec<std::sync::mpsc::SyncSender<()>>>>;

fn spawn_watcher(watch_dir: PathBuf, senders: SseSenders) {
    thread::spawn(move || {
        let mut last = collect_mtimes(&watch_dir);
        loop {
            thread::sleep(Duration::from_millis(500));
            let current = collect_mtimes(&watch_dir);
            if current != last {
                last = current;
                let mut locked = senders.lock().unwrap();
                locked.retain(|s| s.send(()).is_ok());
            }
        }
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
    // Redirect bare "/" to docs/index.html if present, else docs/html root.
    let url_path = if url_path == "/" { "/docs/index.html" } else { url_path };

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
fn handle_request(
    request: Request,
    html_dir: &Path,
    senders: &SseSenders,
    project_root: &Path,
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

pub fn run_serve(port: u16, html_override: Option<PathBuf>) -> Result<(), String> {
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
        thread::spawn(move || {
            handle_request(request, &html_dir2, &senders2, &root2);
        });
    }

    Ok(())
}
