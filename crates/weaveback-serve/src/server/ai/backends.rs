// weaveback-serve/src/server/ai/backends.rs
// I'd Really Rather You Didn't edit this generated file.

use super::*;

use std::process::Stdio;

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
pub(in crate::server::ai) fn call_claude_cli(
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
pub(in crate::server::ai) fn call_anthropic_api(
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
pub(in crate::server::ai) fn call_gemini_api(
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
pub(in crate::server::ai) fn call_ollama_api(
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
pub(in crate::server::ai) fn call_openai_api(
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

