//! 连接配置与模型提供商判定：供 routes 与 services 共同使用。
//! 服务层不应依赖路由层，故此处下沉所有与连接设置相关的纯函数。

use serde::{Deserialize, Serialize};
use crate::state::ServerState;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionSettings {
    #[serde(default = "default_provider")]
    pub provider: String,
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub summary_model: String,
    #[serde(default = "default_num_gpu")]
    pub num_gpu: i64,
    #[serde(default = "default_temperature")]
    pub temperature: f32,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    #[serde(default = "default_top_p")]
    pub top_p: f32,
    #[serde(default)]
    pub frequency_penalty: f32,
    #[serde(default)]
    pub presence_penalty: f32,
    #[serde(default = "default_repetition_penalty")]
    pub repetition_penalty: f32,
}

fn default_provider() -> String {
    String::new()
}
fn default_temperature() -> f32 {
    0.0
}
fn default_max_tokens() -> u32 {
    0
}
fn default_top_p() -> f32 {
    0.0
}
fn default_repetition_penalty() -> f32 {
    0.0
}
fn default_num_gpu() -> i64 {
    0
}

pub fn ollama_root(base_url: &str) -> String {
    base_url
        .trim()
        .trim_end_matches('/')
        .strip_suffix("/v1")
        .unwrap_or(base_url.trim().trim_end_matches('/'))
        .trim_end_matches('/')
        .to_string()
}

pub fn api_base_url(connection: &ConnectionSettings, ollama_native: bool) -> String {
    let base = connection.base_url.trim_end_matches('/');
    if !ollama_native && connection.provider == "ollama" && !base.ends_with("/v1") {
        format!("{base}/v1")
    } else {
        base.to_string()
    }
}

pub fn uses_native_ollama(connection: &ConnectionSettings, enabled: bool) -> bool {
    enabled && connection.provider == "ollama"
}

pub fn connection_settings(st: &ServerState) -> ConnectionSettings {
    // 空白默认：settings 表里 connection 不存在就是不存在，不回退读环境变量
    let mut cfg = ConnectionSettings {
        provider: String::new(),
        base_url: String::new(),
        api_key: String::new(),
        model: String::new(),
        summary_model: String::new(),
        num_gpu: 0,
        temperature: 0.0,
        max_tokens: 0,
        top_p: 0.0,
        frequency_penalty: 0.0,
        presence_penalty: 0.0,
        repetition_penalty: 0.0,
    };
    let mem = st.memory.lock().unwrap_or_else(|e| e.into_inner());
    let raw_saved = mem.get_setting("connection").ok().flatten();
    let saved = raw_saved.and_then(|s| serde_json::from_str::<ConnectionSettings>(&s).ok());
    if let Some(saved) = saved {
        cfg.provider = saved.provider;
        if !saved.base_url.trim().is_empty() {
            cfg.base_url = saved.base_url;
        }
        if !saved.model.trim().is_empty() {
            cfg.model = saved.model;
        }
        cfg.summary_model = saved.summary_model;
        cfg.num_gpu = saved.num_gpu.clamp(0, 99);
        if !st.env_api_key && !saved.api_key.trim().is_empty() && saved.api_key != "••••••••" {
            cfg.api_key = saved.api_key;
        }
        cfg.temperature = saved.temperature;
        cfg.max_tokens = saved.max_tokens;
        cfg.top_p = saved.top_p;
        cfg.frequency_penalty = saved.frequency_penalty;
        cfg.presence_penalty = saved.presence_penalty;
        cfg.repetition_penalty = saved.repetition_penalty;
    }
    if !cfg.provider.is_empty() && cfg.provider == "ollama" {
        cfg.base_url = ollama_root(&cfg.base_url);
    }
    cfg
}

pub fn api_key_source(st: &ServerState) -> &'static str {
    if st.env_api_key {
        return "env";
    }
    let mem = st.memory.lock().unwrap_or_else(|e| e.into_inner());
    let in_db = mem
        .get_setting("connection")
        .ok()
        .flatten()
        .and_then(|s| serde_json::from_str::<ConnectionSettings>(&s).ok())
        .map(|c| !c.api_key.trim().is_empty() && c.api_key != "••••••••")
        .unwrap_or(false);
    if in_db {
        "settings"
    } else {
        "default"
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChatLoreEntry {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub keys: String,
    #[serde(default)]
    pub content: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RegexScript {
    #[serde(default, alias = "scriptName")]
    pub script_name: String,
    #[serde(default, alias = "findRegex")]
    pub find_regex: String,
    #[serde(default, alias = "replaceString")]
    pub replace_string: String,
    #[serde(default)]
    pub placement: Vec<i64>,
    #[serde(default)]
    pub disabled: bool,
    #[serde(default, alias = "promptOnly")]
    pub prompt_only: bool,
}
