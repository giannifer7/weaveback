# Serve AI Endpoint

AI request handling and backend integrations.

## AI assistant endpoint

`POST /__ai` builds a context object from the DB and the literate source,
calls the configured AI backend with streaming enabled, and forwards the 
response as Server-Sent Events back to the browser.

The backend is selected via the `--ai-backend` CLI flag:

* `claude-cli` (default) — shells out to `claude`. Uses the existing Claude 
  Code session; no API key required.
* `anthropic` — calls the Anthropic Messages API directly. Requires 
  `ANTHROPIC_API_KEY`.
* `gemini` — calls the Google Gemini API. Requires `GOOGLE_API_KEY`.
* `ollama` — calls a local Ollama instance. Uses `--ai-endpoint` 
  (default: `http://localhost:11434`).
* `openai` — calls an OpenAI-compatible API. Requires `OPENAI_API_KEY` 
  (optional for local providers) and `--ai-endpoint`.

Request JSON fields:

* `file` — adoc path relative to project root (optional; omit for general questions)
* `name` — chunk name (optional)
* `nth` — 0-based definition index (default 0)
* `question` — the user's question (required)

SSE events sent back to the browser:

* `event: token` / `data: {"t":"..."}` — one streamed text piece
* `event: done` / `data:` — end of stream
* `event: error` / `data: {"error":"..."}` — API or I/O failure

Pre-flight errors (missing `question`, no API key) return a plain JSON
`{ "ok": false, "error": "..." }` response with `Content-Type: application/json`,
consistent with the other endpoints.  The browser checks the response
`Content-Type` before deciding how to parse it.

`build_chunk_context` reads the database and source file to produce a JSON
context object passed as extra context to the model.  It is best-effort:
a failed DB open or missing chunk simply returns `null` and the question is
still forwarded without chunk-specific context.

`AiChannelReader` is a `Read` impl backed by an `mpsc::Receiver<String>`.
A background thread calls the Anthropic API, parses the SSE delta events,
and sends formatted SSE lines through the channel.  This decouples the
Anthropic response parsing from the tiny_http response write loop.

```rust
// <[serve-ai]>=
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
/// `----` listing-block fences and noweb chunk bodies.  The result is the
/// human-written narrative of the section — headings, paragraphs, admonitions,
/// lists — without any code.
///
/// Skipping chunk bodies here is defensive.  In well-formed literate sources,
/// chunk definitions should already live inside fenced code blocks.  If prose
/// extraction still encounters raw chunk markers in section text, that is a
/// source-structure problem and should eventually be reported by a linter
/// rather than silently normalized by every downstream consumer.
pub(crate) fn extract_prose(lines: &[&str], start: usize, end: usize) -> String {
    let end = end.min(lines.len());
    let mut in_fence = false;
    let mut in_chunk = false;
    let mut out: Vec<&str> = Vec::new();
    for l in lines.iter().take(end).skip(start) {
        let trimmed = l.trim();
        if trimmed == "----" {
            in_fence = !in_fence;
            continue;
        }
        if trimmed.starts_with("// <<") && trimmed.ends_with(">>=") {
            in_chunk = true;
            continue;
        }
        if trimmed == "// @" {
            in_chunk = false;
            continue;
        }
        if !in_fence && !in_chunk {
            out.push(l);
        }
    }
    // Trim leading/trailing blank lines.
    while out.first().map(|l| l.trim().is_empty()).unwrap_or(false) { out.remove(0); }
    while out.last().map(|l| l.trim().is_empty()).unwrap_or(false) { out.pop(); }
    out.join("\n")
}

/// Return the body text of each direct dependency of `chunk_name`.
/// Keys are chunk names; values are `{ "file": "…", "body": "…" }`.
pub fn dep_bodies(
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
pub fn git_log_for_file(project_root: &Path, src_file: &str) -> Vec<String> {
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

pub(crate) struct AiChannelReader {
    rx:  std::sync::mpsc::Receiver<String>,
    buf: Vec<u8>,
    pos: usize,
}

impl AiChannelReader {
    pub(crate) fn new(rx: std::sync::mpsc::Receiver<String>) -> Self {
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

pub(crate) fn sse_headers() -> Vec<Header> {
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

pub(in crate::server) fn handle_ai(mut request: Request, project_root: &Path, cfg: &TangleConfig) {
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
// @
```

