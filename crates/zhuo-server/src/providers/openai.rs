//! OpenAI-compatible provider adapter (OpenAI, OpenRouter, DeepSeek, Moonshot, Aliyun, Ollama-native, Custom)

use async_trait::async_trait;
use super::{ChatProvider, EventCallback, LlmEvent, ProviderType};
use crate::config::ConnectionSettings;
use serde_json::Value;
use std::collections::HashMap;
use zhuo_llm::Msg;

pub struct OpenAiProvider;

#[async_trait]
impl ChatProvider for OpenAiProvider {
    fn provider_type(&self) -> ProviderType {
        ProviderType::OpenAI
    }

    fn chat_url(&self, connection: &ConnectionSettings, model: &str) -> String {
        let base = connection.base_url.trim_end_matches('/');
        if connection.provider == "ollama" {
            format!("{}/api/chat", base)
        } else {
            format!("{}/v1/chat/completions", base)
        }
    }

    fn build_headers(&self, api_key: &str) -> HashMap<String, String> {
        let mut m = HashMap::new();
        m.insert("Content-Type".to_string(), "application/json".to_string());
        if !api_key.is_empty() {
            m.insert("Authorization".to_string(), format!("Bearer {}", api_key));
        }
        m
    }

    fn build_request_body(
        &self,
        messages: Vec<Msg>,
        model: &str,
        temperature: f32,
        max_tokens: u32,
        top_p: f32,
        stop: Option<Vec<String>>,
    ) -> Value {
        let msg_list: Value = serde_json::to_value(
            messages
                .into_iter()
                .map(|m| {
                    serde_json::json!({
                        "role": m.role,
                        "content": m.content,
                    })
                })
                .collect::<Vec<_>>(),
        )
        .unwrap_or(Value::Null);

        let mut body = serde_json::json!({
            "model": model,
            "messages": msg_list,
            "temperature": temperature,
            "max_tokens": max_tokens,
            "top_p": top_p,
            "stream": true,
        });

        if let Some(stop) = stop {
            body["stop"] = serde_json::to_value(stop).unwrap_or(Value::Null);
        }

        body
    }

    fn models_url(&self, connection: &ConnectionSettings) -> String {
        if connection.provider == "ollama" {
            format!(
                "{}/api/tags",
                crate::config::ollama_root(&connection.base_url)
            )
        } else {
            format!("{}/v1/models", connection.base_url.trim_end_matches('/'))
        }
    }

    async fn stream_chat(
        &self,
        url: String,
        headers: HashMap<String, String>,
        body: Value,
        on_event: EventCallback,
    ) -> Result<String, String> {
        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .map_err(|e| format!("create client: {e}"))?;

        let mut req = client.post(&url);
        for (k, v) in &headers {
            req = req.header(k, v);
        }
        let resp = req.json(&body).send().await.map_err(|e| format!("request failed: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("HTTP {status}: {text}"));
        }

        let mut partial = String::new();
        use futures_util::StreamExt;
        let mut stream = resp.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| format!("stream read: {e}"))?;
            let text = String::from_utf8_lossy(&chunk);
            for line in text.lines() {
                let line = line.trim();
                if let Some(payload) = line.strip_prefix("data: ") {
                    if payload == "[DONE]" {
                        break;
                    }
                    if let Ok(ev) = serde_json::from_str::<Value>(payload) {
                        // OpenAI-compatible delta
                        if let Some(delta) = ev
                            .get("choices")
                            .and_then(|c| c.get(0))
                            .and_then(|c| c.get("delta"))
                        {
                            if let Some(content) = delta.get("content").and_then(|c| c.as_str()) {
                                if !content.is_empty() {
                                    partial.push_str(content);
                                    on_event(LlmEvent::Token(content.to_string()));
                                }
                            }
                        }
                        // Usage
                        if let Some(usage) = ev.get("usage") {
                            if let Some(total) = usage.get("total_tokens").and_then(|t| t.as_u64()) {
                                on_event(LlmEvent::Usage(Some(total as u32)));
                            }
                        }
                    }
                }
            }
        }

        Ok(partial)
    }
}
