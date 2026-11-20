//! Google provider adapter (Generative Language API)
//!
//! Docs: https://ai.google.dev/api/rest/v1beta/models/streamGenerateContent

use async_trait::async_trait;
use super::{ChatProvider, EventCallback, LlmEvent, ProviderType};
use crate::config::ConnectionSettings;
use serde_json::Value;
use std::collections::HashMap;
use zhuo_llm::Msg;

pub struct GoogleProvider;

#[async_trait]
impl ChatProvider for GoogleProvider {
    fn provider_type(&self) -> ProviderType {
        ProviderType::Google
    }

    fn chat_url(&self, connection: &ConnectionSettings, model: &str) -> String {
        let base = connection.base_url.trim_end_matches('/');
        let api_key = &connection.api_key;
        let model_param = if model.is_empty() { "gemini-1.5-pro" } else { model };
        format!(
            "{}/v1beta/models/{}:streamGenerateContent?key={}",
            base, model_param, api_key
        )
    }

    fn build_headers(&self, _api_key: &str) -> HashMap<String, String> {
        let mut m = HashMap::new();
        m.insert("Content-Type".to_string(), "application/json".to_string());
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
        // Google separates system instruction from conversation
        let (system, conversation): (Vec<_>, Vec<_>) =
            messages.into_iter().partition(|m| m.role == "system");

        let system_text = system
            .into_iter()
            .map(|m| m.content)
            .collect::<Vec<_>>()
            .join("\n");

        // Google uses "user" and "model" roles (not "assistant")
        let contents: Value = serde_json::to_value(
            conversation
                .into_iter()
                .map(|m| {
                    let role = if m.role == "assistant" { "model" } else { "user" };
                    serde_json::json!({
                        "role": role,
                        "parts": [{"text": m.content}],
                    })
                })
                .collect::<Vec<_>>(),
        )
        .unwrap_or(Value::Null);

        let mut generation_config = serde_json::json!({});
        if temperature > 0.0 {
            generation_config["temperature"] = Value::from(temperature);
        }
        if max_tokens > 0 {
            generation_config["maxOutputTokens"] = Value::from(max_tokens);
        }
        if top_p > 0.0 && top_p < 1.0 {
            generation_config["topP"] = Value::from(top_p);
        }

        if let Some(stop) = _stop {
            if !stop.is_empty() {
                generation_config["stopSequences"] = serde_json::to_value(stop).unwrap_or(Value::Null);
            }
        }

        let mut body = serde_json::json!({
            "contents": contents,
            "generationConfig": generation_config,
        });

        if !system_text.is_empty() {
            body["systemInstruction"] = serde_json::json!({
                "parts": [{"text": system_text}],
            });
        }

        body
    }

    fn models_url(&self, connection: &ConnectionSettings) -> String {
        format!("{}/v1beta/models", connection.base_url.trim_end_matches('/'))
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
                        if let Some(candidates) = ev.get("candidates").and_then(|c| c.as_array()) {
                            for candidate in candidates {
                                if let Some(content) = candidate.get("content").and_then(|c| c.get("parts")) {
                                    if let Some(parts) = content.as_array() {
                                        for part in parts {
                                            if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                                                if !text.is_empty() {
                                                    partial.push_str(text);
                                                    on_event(LlmEvent::Token(text.to_string()));
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(partial)
    }
}
