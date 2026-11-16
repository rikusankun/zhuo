use axum::extract::{Query, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use zhuo_memory::MemoryHit;

use crate::services::memory_service::{self, MemoryQuery, SummaryOutcome};
use crate::state::ServerState;

#[derive(Deserialize)]
pub struct AddMemoryRequest {
    pub card: String,
    pub summary: String,
    #[serde(default)]
    pub keywords: String,
    #[serde(default)]
    pub source_round: i64,
}

#[derive(Deserialize)]
pub struct SummarizeRequest {
    pub card: String,
}

fn memory_payload(hits: &[MemoryHit]) -> Value {
    json!({
        "memory_hits": hits.iter().map(|hit| json!({
            "summary": hit.summary,
            "keywords": hit.keywords,
            "source_round": hit.source_round,
        })).collect::<Vec<_>>(),
    })
}

/// POST /api/memories — manually add a memory card.
pub async fn add_memory(
    State(st): State<ServerState>,
    Json(req): Json<AddMemoryRequest>,
) -> Json<Value> {
    let key = st.card_key(&req.card);
    let store = st.memory.lock().unwrap_or_else(|e| e.into_inner());
    match memory_service::add_memory(&store, &key, &req.summary, &req.keywords, req.source_round) {
        Ok((id, total)) => Json(json!({"ok": true, "id": id, "total": total})),
        Err(e) => Json(json!({"error": format!("{e:#}")})),
    }
}

/// GET /api/memories?card=&q=&limit= — recent list or FTS search.
pub async fn search_memories(
    State(st): State<ServerState>,
    Query(q): Query<HashMap<String, String>>,
) -> Json<Value> {
    let key = st.card_key(&q.get("card").cloned().unwrap_or_default());
    let query = q.get("q").cloned().unwrap_or_default();
    let limit = q.get("limit").and_then(|s| s.parse::<usize>().ok()).unwrap_or(20).clamp(1, 100);
    let store = st.memory.lock().unwrap_or_else(|e| e.into_inner());
    match memory_service::query_memories(&store, &key, &query, limit) {
        Ok(MemoryQuery::Recent {
            total,
            unprocessed,
            memories,
        }) => Json(json!({
            "total": total,
            "unprocessed": unprocessed,
            "memories": memories.iter().map(|hit| json!({
                "id": hit.id,
                "summary": hit.summary,
                "keywords": hit.keywords,
                "source_round": hit.source_round,
                "created_at": hit.created_at,
            })).collect::<Vec<_>>(),
        })),
        Ok(MemoryQuery::Search(hits)) => Json(memory_payload(&hits)),
        Err(e) => Json(json!({"error": format!("{e:#}")})),
    }
}

/// POST /api/summarize — manually trigger memory extraction.
pub async fn summarize(
    State(st): State<ServerState>,
    Json(req): Json<SummarizeRequest>,
) -> Json<Value> {
    match memory_service::summarize_card(&st, &req.card).await {
        Ok(SummaryOutcome::Disabled) => Json(json!({"error": "未选择记忆提取模型，自动提取关闭"})),
        Ok(SummaryOutcome::Busy) => {
            Json(json!({"ok": false, "skipped": "该卡正在提取中，请稍后再试"}))
        }
        Ok(SummaryOutcome::Skipped) => {
            Json(json!({"ok": false, "skipped": "未处理消息不足或模型输出不可解析"}))
        }
        Ok(SummaryOutcome::Completed(summary)) => Json(json!({"ok": true, "summary": summary})),
        Err(e) => Json(json!({"error": format!("{e:#}")})),
    }
}
