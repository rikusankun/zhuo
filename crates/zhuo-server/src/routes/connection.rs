use axum::extract::{State};
use axum::Json;
use serde_json::json;

use crate::config::{api_key_source, connection_settings, ConnectionSettings};
use crate::routes::{ConnectionModelsRequest};
use crate::state::ServerState;

pub async fn ollama_tags(State(st): State<ServerState>) -> Json<serde_json::Value> {
    match super::fetch_ollama_models(&connection_settings(&st).base_url).await {
        Ok(models) => Json(json!({"models": models})),
        Err(e) => Json(json!({"error": e})),
    }
}

pub async fn connection_models(
    Json(payload): Json<ConnectionModelsRequest>,
) -> Json<serde_json::Value> {
    match super::fetch_models(&payload.provider, &payload.base_url, &payload.api_key).await {
        Ok(models) => Json(json!({"models": models})),
        Err(e) => Json(json!({"models": [], "error": e})),
    }
}

pub async fn get_connection(State(st): State<ServerState>) -> Json<serde_json::Value> {
    let mut cfg = connection_settings(&st);
    let configured = !cfg.base_url.trim().is_empty() && !cfg.model.trim().is_empty();
    if configured && !cfg.api_key.trim().is_empty() {
        cfg.api_key = "••••••••".into();
    }
    let connection_value = if configured { json!(cfg) } else { json!(null) };
    Json(json!({
        "connection": connection_value,
        "api_key_configured": configured,
        "api_key_source": api_key_source(&st),
    }))
}

pub async fn set_connection(
    State(st): State<ServerState>,
    Json(mut cfg): Json<ConnectionSettings>,
) -> Json<serde_json::Value> {
    if cfg.base_url.trim().is_empty() || cfg.model.trim().is_empty() {
        return Json(json!({"error":"Base URL 和模型不能为空"}));
    }
    if !matches!(cfg.provider.as_str(), "ollama" | "openai" | "openai_compatible") {
        return Json(json!({"error":"不支持的服务类型"}));
    }
    cfg.base_url = cfg.base_url.trim().trim_end_matches('/').to_string();
    let valid_url = reqwest::Url::parse(&cfg.base_url).is_ok_and(|url| {
        matches!(url.scheme(), "http" | "https")
            && url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none()
    });
    if !valid_url {
        return Json(
            json!({"error":"Base URL 必须是有效的 HTTP/HTTPS 地址，不能包含凭据、查询或片段"}),
        );
    }
    cfg.model = cfg.model.trim().to_string();
    cfg.summary_model = cfg.summary_model.trim().to_string();
    if cfg.api_key.trim().is_empty() || cfg.api_key == "••••••••" {
        cfg.api_key = connection_settings(&st).api_key;
    }
    cfg.temperature = cfg.temperature.clamp(0.0, 2.0);
    cfg.max_tokens = cfg.max_tokens.clamp(1, 32768);
    cfg.top_p = cfg.top_p.clamp(0.0, 1.0);
    cfg.frequency_penalty = cfg.frequency_penalty.clamp(-2.0, 2.0);
    cfg.presence_penalty = cfg.presence_penalty.clamp(-2.0, 2.0);
    cfg.repetition_penalty = cfg.repetition_penalty.clamp(0.0, 2.0);
    if let Some(gpu) = cfg.num_gpu {
        cfg.num_gpu = Some(gpu.clamp(0, 99));
    }
    let raw = serde_json::to_string(&cfg).unwrap_or_default();
    let mem = st.memory.lock().unwrap_or_else(|e| e.into_inner());
    match mem.set_setting("connection", &raw) {
        Ok(()) => Json(json!({"ok":true})),
        Err(e) => Json(json!({"error":format!("{e:#}")})),
    }
}
