//! HTTP 路由：SSE 流式对话（含记忆注入）+ 历史 + 重置 + 卡列表 + 记忆卡片管理。
//! 域处理已拆分至子模块：connection / cards / chat / history / state / compatibility。

pub mod memory;

pub mod connection;
pub mod cards;
pub mod chat;
pub mod history;
pub mod state;
pub mod compatibility;

pub use connection::{get_connection, ollama_tags, set_connection, connection_models};
pub use cards::{create_card, delete_card, cards, export_card, import_card, import_preset, import_regex, import_worldbook};
pub use chat::{chat, create_chat, delete_chat, list_chats, rename_chat};
pub use history::{clear_history, history, reset};
pub use state::{get_state, set_state};
pub use compatibility::compatibility;

use axum::response::sse::{Event, Sse};
use axum::Json;
use futures_util::stream::Stream;
use serde::Deserialize;
use serde_json::{json, Value};
use std::convert::Infallible;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};

use crate::config::{ChatLoreEntry, ollama_root};

/// 统一 SSE 流类型：盒装动态流，保证所有返回路径是同一个具体类型。
pub type SseStream = std::pin::Pin<Box<dyn Stream<Item = Result<Event, Infallible>> + Send>>;

#[derive(Deserialize)]
pub struct ChatRequest {
    pub card: String,
    #[serde(default)]
    pub chat_id: String,
    pub message: String,
    #[serde(default)]
    pub regenerate: bool,
    #[serde(default)]
    pub display_message: String,
    #[serde(default)]
    pub persona: Option<ChatPersona>,
    #[serde(default)]
    pub world_info: Vec<String>,
    #[serde(default)]
    pub lore_entries: Vec<ChatLoreEntry>,
    #[serde(default)]
    pub model: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChatPersona {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Deserialize)]
pub struct CreateChatRequest {
    pub card: String,
    #[serde(default)]
    pub title: String,
}

#[derive(Deserialize)]
pub struct RenameChatRequest {
    pub title: String,
}

#[derive(Deserialize)]
pub struct ImportCardRequest {
    pub filename: String,
    pub data: String,
}

#[derive(Deserialize)]
pub struct CreateCardRequest {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub first_mes: String,
    #[serde(default)]
    pub system_prompt: String,
}

#[derive(Deserialize)]
pub struct ExportQuery {
    #[serde(default = "default_export_format")]
    pub format: String,
}
fn default_export_format() -> String {
    "json".into()
}

#[derive(Deserialize)]
pub struct ConnectionModelsRequest {
    pub provider: String,
    pub base_url: String,
    pub api_key: String,
}

#[derive(Deserialize)]
pub struct ResetRequest {
    pub card: String,
    #[serde(default)]
    pub memories: bool,
}

#[derive(Deserialize)]
pub struct SetStateRequest {
    pub card: String,
    pub state: serde_json::Value,
}

pub const DEFAULT_USER_NAME: &str = "旅人";

fn extract_model_names(payload: &Value) -> Vec<String> {
    let arr = payload
        .get("data")
        .or_else(|| payload.get("数据"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    arr.into_iter()
        .filter_map(|m| {
            m.get("id")
                .or_else(|| m.get("ID"))
                .or_else(|| m.get("name"))
                .and_then(Value::as_str)
        })
        .map(str::to_string)
        .collect()
}

fn ollama_model_names(payload: &Value) -> Vec<String> {
    payload
        .get("models")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|model| {
            model
                .get("name")
                .or_else(|| model.get("ID"))
                .or_else(|| model.get("id"))
                .and_then(Value::as_str)
        })
        .map(str::to_string)
        .collect()
}

pub async fn fetch_ollama_models(base_url: &str) -> Result<Vec<String>, String> {
    let url = format!("{}/api/tags", ollama_root(base_url));
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| format!("create Ollama client: {e}"))?;
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("Ollama tags request failed: {e}"))?;
    if !response.status().is_success() {
        let status = response.status();
        let detail = match response.text().await {
            Ok(detail) => detail.chars().take(500).collect::<String>(),
            Err(e) => format!("could not read error response: {e}"),
        };
        return Err(format!("Ollama tags returned HTTP {status}: {detail}"));
    }
    let payload: Value = response
        .json()
        .await
        .map_err(|e| format!("parse Ollama tags response: {e}"))?;
    Ok(ollama_model_names(&payload))
}

pub async fn fetch_openai_models(base_url: &str, api_key: &str) -> Result<Vec<String>, String> {
    let base = base_url.trim_end_matches('/');
    let url = format!("{base}/models");
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| format!("create OpenAI client: {e}"))?;
    let response = {
        let mut req = client.get(url);
        if !api_key.is_empty() {
            req = req.header("Authorization", format!("Bearer {api_key}"));
        }
        req.send()
            .await
            .map_err(|e| format!("OpenAI models request failed: {e}"))?
    };
    if !response.status().is_success() {
        let status = response.status();
        let detail = match response.text().await {
            Ok(detail) => detail.chars().take(500).collect::<String>(),
            Err(e) => format!("could not read error response: {e}"),
        };
        return Err(format!("OpenAI models returned HTTP {status}: {detail}"));
    }
    let payload: Value = response
        .json()
        .await
        .map_err(|e| format!("parse OpenAI models response: {e}"))?;
    Ok(extract_model_names(&payload))
}

pub async fn fetch_models(provider: &str, base_url: &str, api_key: &str) -> Result<Vec<String>, String> {
    if provider.is_empty() {
        return Err("尚未选择服务类型".into());
    }
    match provider {
        "ollama" => fetch_ollama_models(base_url).await,
        "anthropic" => fetch_anthropic_models(base_url, api_key).await,
        "google" => fetch_google_models(base_url, api_key).await,
        _ => fetch_openai_models(base_url, api_key).await,
    }
}

async fn fetch_anthropic_models(base_url: &str, api_key: &str) -> Result<Vec<String>, String> {
    let url = format!("{}/v1/models", base_url.trim_end_matches('/'));
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("create Anthropic client: {e}"))?;
    let response = client
        .get(&url)
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .send()
        .await
        .map_err(|e| format!("Anthropic models request failed: {e}"))?;
    if !response.status().is_success() {
        let status = response.status();
        let detail = response.text().await.unwrap_or_default();
        return Err(format!("Anthropic models returned HTTP {status}: {detail}"));
    }
    let payload: Value = response.json().await.map_err(|e| format!("parse Anthropic models response: {e}"))?;
    Ok(payload
        .get("data")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|m| m.get("id").and_then(Value::as_str))
        .map(str::to_string)
        .collect())
}

async fn fetch_google_models(base_url: &str, api_key: &str) -> Result<Vec<String>, String> {
    let url = format!("{}/v1beta/models?key={}", base_url.trim_end_matches('/'), api_key);
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("create Google client: {e}"))?;
    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Google models request failed: {e}"))?;
    if !response.status().is_success() {
        let status = response.status();
        let detail = response.text().await.unwrap_or_default();
        return Err(format!("Google models returned HTTP {status}: {detail}"));
    }
    let payload: Value = response.json().await.map_err(|e| format!("parse Google models response: {e}"))?;
    Ok(payload
        .get("models")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|m| m.get("name").and_then(Value::as_str))
        .map(|name| name.strip_prefix("models/").unwrap_or(name).to_string())
        .collect())
}

fn prompt_payload(
    hits: &[zhuo_memory::PromptHit],
    prompt_chars: usize,
    estimated_tokens: usize,
    history_messages: usize,
) -> serde_json::Value {
    json!({
        "memory_hits": hits.iter().map(|hit| json!({
            "type": hit.kind,
            "title": hit.title,
            "summary": hit.content,
        })).collect::<Vec<_>>(),
        "prompt_chars": prompt_chars,
        "estimated_tokens": estimated_tokens,
        "history_messages": history_messages,
    })
}

pub fn sse_error(msg: String) -> Sse<SseStream> {
    let (tx, rx) = unbounded_channel::<String>();
    let _ = tx.send(json!({ "error": msg }).to_string());
    let _ = tx.send("[DONE]".into());
    Sse::new(unfold_rx(rx))
}

pub fn unfold_rx(rx: UnboundedReceiver<String>) -> SseStream {
    Box::pin(futures_util::stream::unfold(rx, |mut rx| async move {
        rx.recv()
            .await
            .map(|payload| (Ok(Event::default().data(payload)), rx))
    }))
}

pub fn generation_error_message(error: &anyhow::Error) -> String {
    if error.to_string() == zhuo_llm::FIRST_TOKEN_TIMEOUT_ERROR {
        "生成超时，请检查上下文或降低显存层数".to_string()
    } else {
        format!("{error:#}")
    }
}

/// GET / —— 健康检查
pub async fn index() -> Json<serde_json::Value> {
    Json(json!({"status": "zhuo-server ok"}))
}
