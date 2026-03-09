use keyring::Entry;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::storage::load_settings;

// ─── Types ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionRequest {
    pub prefix: String,
    pub suffix: String,
    pub collection: Option<String>,
    pub db: Option<String>,
    pub field_names: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionResponse {
    pub text: String,
}

// ─── Keychain helpers ────────────────────────────────────────────────────────

fn ai_keychain_entry(provider: &str) -> Option<Entry> {
    Entry::new("ferango-ai", provider).ok()
}

fn fetch_ai_key(provider: &str) -> Option<String> {
    ai_keychain_entry(provider)?.get_password().ok()
}

#[tauri::command]
pub fn save_ai_api_key(provider: String, key: String) -> Result<(), String> {
    let entry = ai_keychain_entry(&provider).ok_or("Failed to access keychain")?;
    entry.set_password(&key).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_ai_api_key_exists(provider: String) -> bool {
    fetch_ai_key(&provider).is_some()
}

// ─── HTTP client ─────────────────────────────────────────────────────────────

fn http_client() -> Result<Client, String> {
    Client::builder()
        .timeout(Duration::from_secs(8))
        .build()
        .map_err(|e| e.to_string())
}

// ─── System prompt ───────────────────────────────────────────────────────────

fn build_system_prompt() -> String {
    "You are a MongoDB query autocomplete assistant for the Ferango GUI.\n\
     You complete JavaScript-style mongosh queries.\n\n\
     Rules:\n\
     - Output ONLY the completion text (the code that comes after the cursor). Do not repeat the prefix.\n\
     - Keep completions short: finish the current statement or line, not entire scripts.\n\
     - Use mongosh syntax: db.getCollection(\"name\").find(), aggregate(), etc.\n\
     - If field names are provided, prefer those field names in filters and projections.\n\
     - Do not add markdown formatting, comments, or explanations.\n\
     - If you cannot suggest a meaningful completion, respond with an empty string."
        .to_string()
}

fn build_user_prompt(req: &CompletionRequest) -> String {
    let mut ctx = String::new();
    if let Some(db) = &req.db {
        ctx.push_str(&format!("Database: {}\n", db));
    }
    if let Some(col) = &req.collection {
        ctx.push_str(&format!("Collection: {}\n", col));
    }
    if !req.field_names.is_empty() {
        ctx.push_str(&format!("Available fields: {}\n", req.field_names.join(", ")));
    }
    if !ctx.is_empty() {
        ctx.push('\n');
    }
    format!(
        "{}Complete the following code (provide ONLY the continuation):\n```\n{}\n```",
        ctx, req.prefix
    )
}

// ─── Provider: Ollama ────────────────────────────────────────────────────────

async fn complete_ollama(
    endpoint: &str,
    model: &str,
    req: &CompletionRequest,
) -> Result<String, String> {
    let client = http_client()?;
    let is_fim = model.contains("codellama")
        || model.contains("deepseek-coder")
        || model.contains("starcoder")
        || model.contains("qwen2.5-coder");

    let body = if is_fim {
        // Fill-in-the-middle format
        serde_json::json!({
            "model": model,
            "prompt": format!("<PRE> {} <SUF>{} <MID>", req.prefix, req.suffix),
            "stream": false,
            "options": {
                "temperature": 0.2,
                "stop": ["\n\n", "<EOT>", "```"],
                "num_predict": 256
            }
        })
    } else {
        // Chat-style for general models
        serde_json::json!({
            "model": model,
            "system": build_system_prompt(),
            "prompt": build_user_prompt(req),
            "stream": false,
            "options": {
                "temperature": 0.2,
                "stop": ["\n\n", "```"],
                "num_predict": 256
            }
        })
    };

    let url = format!("{}/api/generate", endpoint.trim_end_matches('/'));
    let resp = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let text = json["response"].as_str().unwrap_or("").to_string();
    Ok(truncate_completion(text))
}

// ─── Provider: OpenAI ────────────────────────────────────────────────────────

async fn complete_openai(
    endpoint: &str,
    model: &str,
    api_key: &str,
    req: &CompletionRequest,
) -> Result<String, String> {
    let client = http_client()?;
    let url = format!("{}/v1/chat/completions", endpoint.trim_end_matches('/'));

    let body = serde_json::json!({
        "model": model,
        "messages": [
            { "role": "system", "content": build_system_prompt() },
            { "role": "user", "content": build_user_prompt(req) }
        ],
        "max_tokens": 256,
        "temperature": 0.2,
        "stop": ["\n\n"]
    });

    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let text = json["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("")
        .to_string();
    Ok(truncate_completion(text))
}

// ─── Provider: Claude ────────────────────────────────────────────────────────

async fn complete_claude(
    endpoint: &str,
    model: &str,
    api_key: &str,
    req: &CompletionRequest,
) -> Result<String, String> {
    let client = http_client()?;
    let url = format!("{}/v1/messages", endpoint.trim_end_matches('/'));

    let body = serde_json::json!({
        "model": model,
        "max_tokens": 256,
        "system": build_system_prompt(),
        "messages": [
            { "role": "user", "content": build_user_prompt(req) }
        ]
    });

    let resp = client
        .post(&url)
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let text = json["content"][0]["text"]
        .as_str()
        .unwrap_or("")
        .to_string();
    Ok(truncate_completion(text))
}

// ─── Truncation helper ───────────────────────────────────────────────────────

fn truncate_completion(text: String) -> String {
    let trimmed = text.trim_start_matches("```javascript")
        .trim_start_matches("```js")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim()
        .to_string();

    if trimmed.len() <= 500 {
        return trimmed;
    }
    // Truncate to last complete line within 500 chars
    match trimmed[..500].rfind('\n') {
        Some(pos) => trimmed[..pos].to_string(),
        None => trimmed[..500].to_string(),
    }
}

// ─── Tauri command: ai_complete ──────────────────────────────────────────────

#[tauri::command]
pub async fn ai_complete(request: CompletionRequest) -> Result<CompletionResponse, String> {
    let settings = load_settings();

    if !settings.ai_enabled {
        return Ok(CompletionResponse { text: String::new() });
    }

    let text = match settings.ai_provider.as_str() {
        "ollama" => {
            complete_ollama(&settings.ai_endpoint, &settings.ai_model, &request).await?
        }
        "openai" => {
            let key = fetch_ai_key("openai").ok_or("No OpenAI API key configured")?;
            complete_openai(&settings.ai_endpoint, &settings.ai_model, &key, &request).await?
        }
        "claude" => {
            let key = fetch_ai_key("claude").ok_or("No Claude API key configured")?;
            complete_claude(&settings.ai_endpoint, &settings.ai_model, &key, &request).await?
        }
        other => return Err(format!("Unknown AI provider: {}", other)),
    };

    Ok(CompletionResponse { text })
}

// ─── Tauri command: ai_check_health ──────────────────────────────────────────

#[tauri::command]
pub async fn ai_check_health() -> Result<bool, String> {
    let settings = load_settings();

    match settings.ai_provider.as_str() {
        "ollama" => {
            let client = http_client()?;
            let url = format!("{}/api/tags", settings.ai_endpoint.trim_end_matches('/'));
            match client.get(&url).send().await {
                Ok(resp) => Ok(resp.status().is_success()),
                Err(_) => Ok(false),
            }
        }
        "openai" | "claude" => {
            let has_key = fetch_ai_key(&settings.ai_provider).is_some();
            Ok(has_key)
        }
        _ => Ok(false),
    }
}
