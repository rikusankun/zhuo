//! Provider adapters for different LLM providers.
//!
//! Each provider has different API format, auth, and SSE streaming format.
//! This module abstracts those differences so the main chat handler can
//! work with any provider.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

use crate::config::ConnectionSettings;
use zhuo_llm::Msg;

mod anthropic;
mod google;
mod openai;

pub use anthropic::AnthropicProvider;
pub use google::GoogleProvider;
pub use openai::OpenAiProvider;

/// Provider type enum
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProviderType {
    Ollama,
    OpenAI,
    Anthropic,
    Google,
    OpenRouter,
    DeepSeek,
    Moonshot,
    Aliyun,
    Baidu,
    Tencent,
    Custom,
}

/// Event yielded by provider stream to the SSE channel
#[derive(Debug, Clone)]
pub enum LlmEvent {
    Token(String),
    Done,
    Error(String),
    Usage(Option<u32>),
}

/// Callback type for sending events to the SSE channel
pub type EventCallback = Box<dyn Fn(LlmEvent) + Send + Sync>;

/// Trait for LLM chat providers
#[async_trait]
pub trait ChatProvider: Send + Sync {
    fn provider_type(&self) -> ProviderType;

    /// Build the chat completion URL
    fn chat_url(&self, connection: &ConnectionSettings, model: &str) -> String;

    /// Build HTTP headers for the request
    fn build_headers(&self, api_key: &str) -> HashMap<String, String>;

    /// Build the request body for chat completion
    fn build_request_body(
        &self,
        messages: Vec<Msg>,
        model: &str,
        temperature: f32,
        max_tokens: u32,
        top_p: f32,
        stop: Option<Vec<String>>,
    ) -> Value;

    /// Build the models listing URL
    fn models_url(&self, connection: &ConnectionSettings) -> String;

    /// Stream chat completion, yielding events via callback.
    /// Returns the full accumulated text content.
    async fn stream_chat(
        &self,
        url: String,
        headers: HashMap<String, String>,
        body: Value,
        on_event: EventCallback,
    ) -> Result<String, String>;
}

/// Provider registry: returns the provider adapter for a given provider name
pub fn provider_for(name: &str) -> Box<dyn ChatProvider> {
    match name {
        "anthropic" => Box::new(AnthropicProvider),
        "google" => Box::new(GoogleProvider),
        _ => Box::new(OpenAiProvider),
    }
}

/// Get provider type from name string
pub fn provider_type(name: &str) -> ProviderType {
    match name {
        "ollama" => ProviderType::Ollama,
        "openai" => ProviderType::OpenAI,
        "anthropic" => ProviderType::Anthropic,
        "google" => ProviderType::Google,
        "openrouter" => ProviderType::OpenRouter,
        "deepseek" => ProviderType::DeepSeek,
        "moonshot" => ProviderType::Moonshot,
        "aliyun" => ProviderType::Aliyun,
        "baidu" => ProviderType::Baidu,
        "tencent" => ProviderType::Tencent,
        "custom" => ProviderType::Custom,
        _ => ProviderType::OpenAI,
    }
}
