use axum::extract::{Query, State};
use axum::Json;
use serde_json::json;
use std::collections::HashMap;

use crate::routes::ResetRequest;
use crate::state::ServerState;

/// POST /api/reset —— 清空对话历史（可选连记忆一起清）
pub async fn reset(
    State(st): State<ServerState>,
    Json(req): Json<ResetRequest>,
) -> Json<serde_json::Value> {
    let key = st.card_key(&req.card);
    let mem = st.memory.lock().unwrap_or_else(|e| e.into_inner());
    let r = mem.clear_messages(&key);
    let m = if req.memories {
        mem.clear_memories(&key)
    } else {
        Ok(0)
    };
    match (r, m) {
        (Ok(a), Ok(b)) => Json(json!({"ok": true, "messages_cleared": a, "memories_cleared": b})),
        (Err(e), _) | (_, Err(e)) => Json(json!({"error": format!("{e:#}")})),
    }
}

/// DELETE /api/history?card= —— 新建会话：只清当前对话记录，记忆与状态保留。
pub async fn clear_history(
    State(st): State<ServerState>,
    Query(q): Query<HashMap<String, String>>,
) -> Json<serde_json::Value> {
    let key = st.card_key(&q.get("card").cloned().unwrap_or_default());
    let mem = st.memory.lock().unwrap_or_else(|e| e.into_inner());
    match mem.clear_messages(&key) {
        Ok(n) => Json(json!({"ok": true, "messages_cleared": n})),
        Err(e) => Json(json!({"error": format!("{e:#}")})),
    }
}

/// GET /api/history?card= —— 从 SQLite 读对话历史（card 传 id 或名称均可）
pub async fn history(
    State(st): State<ServerState>,
    Query(q): Query<HashMap<String, String>>,
) -> Json<serde_json::Value> {
    let mem = st.memory.lock().unwrap_or_else(|e| e.into_inner());
    let result = if let Some(chat) = q.get("chat") {
        mem.chat_history(chat, 200)
    } else {
        let key = st.card_key(&q.get("card").cloned().unwrap_or_default());
        mem.history(&key, 200)
    };
    match result {
        Ok(msgs) => Json(json!(msgs
            .iter()
            .map(|m| json!({"role": m.role, "content": m.content}))
            .collect::<Vec<_>>())),
        Err(e) => Json(json!({"error": format!("{e:#}")})),
    }
}
