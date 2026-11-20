//! Anthropic provider adapter (Messages API)
//!
//! Docs: https://docs.anthropic.com/claude/reference/messages-post

use async_trait::async_trait;
use super::{ChatProvider, EventCallback, LlmEvent, ProviderType};
use crate::config::ConnectionSettings;
use serde_json::Value;
use std::collections::HashMap;
use zhuo_llm::Msg;

pub struct AnthropicProvider;

#[async_trait]
impl ChatProvider for AnthropicProvider {
    fn provider_type(&self) -> ProviderType {
        ProviderType::Anthropic
    }

    fn chat_url(&self, connection: &ConnectionSettings, _model: &str) -> String {
        format!(
            "{}/v1/messages",
            connection.base_url.trim_end_matches('/')
        )
    }

    fn build_headers(&self, api_key: &str) -> HashMap<String, String> {
        let mut m = HashMap::new();
        m.insert("Content-Type".to_string(), "application/json".to_string());
        m.insert("anthropic-version".to_string(), "2023-06-01".to_string());
        if !api_key.is_empty() {
            m.insert("x-api-key".to_string(), api_key.to_string());
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
        _stop: Option<Vec<String>>,
    ) -> Value {
        // Anthropic separates system messages from conversation messages
        let (system, conversation): (Vec<_>, Vec<_>) =
            messages.into_iter().partition(|m| m.role == "system");

        let system_text = system
            .into_iter()
            .map(|m| m.content)
            .collect::<Vec<_>>()
            .join("\n");

        let msg_list: Value = serde_json::to_value(
            conversation
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
            "max_tokens": max_tokens,
            "messages": msg_list,
            "stream": true,
        });

        if !system_text.is_empty() {
            body["system"] = Value::String(system_text);
        }

        if temperature > 0.0 {
            body["temperature"] = Value::from(temperature);
        }

        if top_p > 0.0 && top_p < 1.0 {
            body["top_p"] = Value::from(top_p);
        }

        if let Some(stop) = _stop {
            body["stop_sequences"] = serde_json::to_value(stop).unwrap_or(Value::Null);
        }

        body
    }

    fn models_url(&self, connection: &ConnectionSettings) -> String {
        format!("{}/v1/models", connection.base_url.trim_end_matches('/'))
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
                        let event_type = ev.get("type").and_then(|t| t.as_str()).unwrap_or("");

                        match event_type {
                            "content_block_delta" => {
                                if let Some(delta) = ev.get("delta") {
                                    if let Some(text) = delta.get("text").and_then(|t| t.as_str()) {
                                        if !text.is_empty() {
                                            partial.push_str(text);
                                            on_event(LlmEvent::Token(text.to_string()));
                                        }
                                    }
                                }
                            }
                            "message_delta" => {
                                if let Some(usage) = ev.get("usage") {
                                    if let Some(total) = usage.get("output_tokens").and_then(|t| t.as_u64()) {
                                        on_event(LlmEvent::Usage(Some(total as u32)));
                                    }
                                }
                            }
                            "message_stop" => {
                                break;
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        Ok(partial)
    }
}
