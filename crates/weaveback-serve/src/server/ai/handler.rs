// weaveback-serve/src/server/ai/handler.rs
// I'd Really Rather You Didn't edit this generated file.

use super::*;

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

