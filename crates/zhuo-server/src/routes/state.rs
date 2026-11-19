use axum::extract::{Query, State};
use axum::Json;
use serde_json::json;
use std::collections::HashMap;

use crate::routes::SetStateRequest;
use crate::state::ServerState;

/// GET /api/state?card= —— 读角色状态（无记录时 state 为 null）
pub async fn get_state(
    State(st): State<ServerState>,
    Query(q): Query<HashMap<String, String>>,
) -> Json<serde_json::Value> {
    let key = st.card_key(&q.get("card").cloned().unwrap_or_default());
    let mem = st.memory.lock().unwrap_or_else(|e| e.into_inner());
    match mem.get_state(&key) {
        Ok(Some(v)) => Json(json!({"card": key, "state": v})),
        Ok(None) => Json(json!({"card": key, "state": null})),
        Err(e) => Json(json!({"error": format!("{e:#}")})),
    }
}

/// PUT /api/state —— 手动更新角色状态
pub async fn set_state(
    State(st): State<ServerState>,
    Json(req): Json<SetStateRequest>,
) -> Json<serde_json::Value> {
    if !req.state.is_object() {
        return Json(
            json!({"error": "state 必须是 JSON 对象，如 {\"背包\":[\"木剑\"],\"位置\":\"冬堡学院\"}"}),
        );
    }
    let key = st.card_key(&req.card);
    let mem = st.memory.lock().unwrap_or_else(|e| e.into_inner());
    match mem.set_state(&key, &req.state) {
        Ok(()) => Json(json!({"ok": true, "card": key, "state": req.state})),
        Err(e) => Json(json!({"error": format!("{e:#}")})),
    }
}
