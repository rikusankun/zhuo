use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use zhuo_memory::MemoryHit;

use crate::services::memory_service::{self, SummaryOutcome};
use crate::state::ServerState;

const DEFAULT_LIMIT: usize = 50;

#[derive(Deserialize)]
pub struct UpdateMemoryRequest {
    pub content: String,
}

/// 手动添加记忆。`title` 接受但不持久化（记忆表无标题列），实际可检索的文本以 `content` 为准。
#[derive(Deserialize)]
pub struct AddMemoryRequest {
    pub card: String,
    pub content: String,
    #[serde(default)]
    pub title: Option<String>,
}

#[derive(Deserialize)]
pub struct SummarizeRequest {
    pub card: String,
}

/// POST /api/summarize — 手动触发记忆提取。
pub async fn summarize(
    State(st): State<ServerState>,
    Json(req): Json<SummarizeRequest>,
) -> (StatusCode, Json<Value>) {
    match memory_service::summarize_card(&st, &req.card).await {
        Ok(SummaryOutcome::Disabled) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "未选择记忆提取模型，自动提取关闭"})),
        ),
        Ok(SummaryOutcome::Busy) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"ok": false, "skipped": "该卡正在提取中，请稍后再试"})),
        ),
        Ok(SummaryOutcome::Skipped) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"ok": false, "skipped": "未处理消息不足或模型输出不可解析"})),
        ),
        Ok(SummaryOutcome::Completed(summary)) => (
            StatusCode::OK,
            Json(json!({"ok": true, "summary": summary})),
        ),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": format!("{e:#}")})),
        ),
    }
}

fn memory_list_payload(hits: &[MemoryHit]) -> Value {
    json!({
        "memories": hits
            .iter()
            .map(|hit| {
                json!({
                    "id": hit.id,
                    "summary": hit.summary,
                    "keywords": hit.keywords,
                    "source_round": hit.source_round,
                    "created_at": hit.created_at,
                })
            })
            .collect::<Vec<_>>(),
    })
}

/// GET /api/memories?card=<id>&limit=50 → { memories: Vec<MemoryHit> }
///
/// 严格按 card 隔离；card 为空返回 400。
pub async fn search_memories(
    State(st): State<ServerState>,
    Query(q): Query<HashMap<String, String>>,
) -> (StatusCode, Json<Value>) {
    let card = q.get("card").cloned().unwrap_or_default();
    if card.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "card 参数不能为空"})),
        );
    }
    let limit = q
        .get("limit")
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(DEFAULT_LIMIT)
        .clamp(1, 100);
    let key = st.card_key(&card);
    let store = st.memory.lock().unwrap_or_else(|e| e.into_inner());
    let hits = match q.get("q") {
        Some(query) if !query.trim().is_empty() => match store.search(&key, query, limit) {
            Ok(hits) => hits,
            Err(e) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({"error": format!("{e:#}")})),
                )
            }
        },
        _ => match store.list_memories(&key, limit) {
            Ok(hits) => hits,
            Err(e) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({"error": format!("{e:#}")})),
                )
            }
        },
    };
    (StatusCode::OK, Json(memory_list_payload(&hits)))
}

/// PUT /api/memories/<id>?card=<id> → Body: { content }
///
/// 更新单条记忆内容，按 card_name 隔离，防止跨角色误改。
pub async fn update_memory(
    State(st): State<ServerState>,
    Path(id): Path<String>,
    Query(q): Query<HashMap<String, String>>,
    Json(req): Json<UpdateMemoryRequest>,
) -> (StatusCode, Json<Value>) {
    let card = q.get("card").cloned().unwrap_or_default();
    if card.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "card 参数不能为空"})),
        );
    }
    let id: i64 = match id.parse() {
        Ok(id) => id,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "无效的记忆 ID"})),
            )
        }
    };
    if req.content.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "内容不能为空"})),
        );
    }
    let key = st.card_key(&card);
    let store = st.memory.lock().unwrap_or_else(|e| e.into_inner());
    match memory_service::update_memory(&store, &key, id, &req.content) {
        Ok(0) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "记忆不存在或不属于当前角色"})),
        ),
        Ok(n) => (
            StatusCode::OK,
            Json(json!({"ok": true, "id": id, "updated": n})),
        ),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": format!("{e:#}")})),
        ),
    }
}

/// DELETE /api/memories/<id>?card=<id> → 删除单条记忆
///
/// 严格按 card_name 隔离，防止跨角色误删；删除后 eprintln 记录日志。
pub async fn delete_memory(
    State(st): State<ServerState>,
    Path(id): Path<String>,
    Query(q): Query<HashMap<String, String>>,
) -> (StatusCode, Json<Value>) {
    let card = q.get("card").cloned().unwrap_or_default();
    if card.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "card 参数不能为空"})),
        );
    }
    let id: i64 = match id.parse() {
        Ok(id) => id,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "无效的记忆 ID"})),
            )
        }
    };
    let key = st.card_key(&card);
    let store = st.memory.lock().unwrap_or_else(|e| e.into_inner());
    match memory_service::delete_memory(&store, &key, id) {
        Ok(0) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "记忆不存在或不属于当前角色"})),
        ),
        Ok(n) => {
            eprintln!("[memory] card={} id={} deleted={}", key, id, n);
            (
                StatusCode::OK,
                Json(json!({"ok": true, "id": id, "deleted": n})),
            )
        }
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": format!("{e:#}")})),
        ),
    }
}

/// POST /api/memories → Body: { card, content, title?: string }
///
/// 手动添加一条记忆，按 card 隔离。
pub async fn add_memory(
    State(st): State<ServerState>,
    Json(req): Json<AddMemoryRequest>,
) -> (StatusCode, Json<Value>) {
    if req.card.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "card 参数不能为空"})),
        );
    }
    if req.content.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "内容不能为空"})),
        );
    }
    let key = st.card_key(&req.card);
    // `title` 被接受但不持久化：记忆表无标题列（不改表结构），实际可检索的文本以 `content` 为准。
    let _ = &req.title;
    let store = st.memory.lock().unwrap_or_else(|e| e.into_inner());
    match memory_service::add_memory(&store, &key, &req.content, "", 0) {
        Ok((id, total)) => (
            StatusCode::OK,
            Json(json!({"ok": true, "id": id, "total": total, "card": key})),
        ),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": format!("{e:#}")})),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::extract::{Path, Query, State};
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};
    use zhuo_memory::MemoryStore;

    fn test_state() -> ServerState {
        let store = MemoryStore::open_in_memory().unwrap();
        // cards_dir 指向不存在的目录，使 card_key 原样返回传入的 card 名，测试更聚焦
        ServerState {
            base_url: Arc::new("http://localhost:11434/v1".into()),
            api_key: Arc::new("ollama".into()),
            model: Arc::new("test".into()),
            cards_dir: Arc::new(PathBuf::from("/nonexistent-zhuo-cards")),
            world: Arc::new("".into()),
            include_usage: false,
            memory: Arc::new(Mutex::new(store)),
            summary_model: Arc::new("".into()),
            summary_rounds: 8,
            state_extract_rounds: 8,
            extracting: Arc::new(Mutex::new(std::collections::HashSet::new())),
            main_num_gpu: 15,
            ollama_native: true,
            env_api_key: false,
        }
    }

    /// 在锁作用域内同步写入种子记忆并返回 id，确保 MutexGuard 不跨越任何 .await。
    fn seed(st: &ServerState, card: &str, content: &str) -> i64 {
        let store = st.memory.lock().unwrap_or_else(|e| e.into_inner());
        store.add_memory(card, content, "种子", 1).unwrap()
    }

    #[tokio::test]
    async fn get_memories_isolated_by_card() {
        let st = test_state();
        seed(&st, "苏菲", "苏菲喜欢糖葫芦");
        seed(&st, "冬堡", "冬堡常年下雪");

        let res = search_memories(
            State(st.clone()),
            Query(HashMap::from([("card".into(), "苏菲".into())])),
        )
        .await;
        assert_eq!(res.0, StatusCode::OK);
        let body = res.1 .0;
        let memories = body.get("memories").and_then(|v| v.as_array()).unwrap();
        assert_eq!(memories.len(), 1);
        assert_eq!(
            memories[0].get("summary").and_then(|v| v.as_str()).unwrap(),
            "苏菲喜欢糖葫芦"
        );

        let res = search_memories(
            State(st),
            Query(HashMap::from([("card".into(), "冬堡".into())])),
        )
        .await;
        assert_eq!(res.0, StatusCode::OK);
        let body = res.1 .0;
        let memories = body.get("memories").and_then(|v| v.as_array()).unwrap();
        assert_eq!(memories.len(), 1);
        assert_eq!(
            memories[0].get("summary").and_then(|v| v.as_str()).unwrap(),
            "冬堡常年下雪"
        );
    }

    #[tokio::test]
    async fn get_memories_empty_card_returns_400() {
        let st = test_state();
        let res = search_memories(
            State(st),
            Query(HashMap::from([("card".into(), "".into())])),
        )
        .await;
        assert_eq!(res.0, StatusCode::BAD_REQUEST);
        assert!(res.1 .0.get("error").is_some());
    }

    #[tokio::test]
    async fn update_memory_content_takes_effect_and_is_isolated() {
        let st = test_state();
        let id = seed(&st, "苏菲", "旧内容");
        seed(&st, "冬堡", "冬堡内容");

        let res = update_memory(
            State(st.clone()),
            Path(id.to_string()),
            Query(HashMap::from([("card".into(), "苏菲".into())])),
            Json(UpdateMemoryRequest {
                content: "新内容：山楂".into(),
            }),
        )
        .await;
        assert_eq!(res.0, StatusCode::OK);

        // 他角色同 id 不应被改动
        let res = update_memory(
            State(st.clone()),
            Path(id.to_string()),
            Query(HashMap::from([("card".into(), "冬堡".into())])),
            Json(UpdateMemoryRequest {
                content: "不该生效".into(),
            }),
        )
        .await;
        assert_eq!(res.0, StatusCode::NOT_FOUND);

        // 下次召回使用新内容
        let res = search_memories(
            State(st),
            Query(HashMap::from([("card".into(), "苏菲".into())])),
        )
        .await;
        assert_eq!(res.0, StatusCode::OK);
        let body = res.1 .0;
        let memories = body.get("memories").and_then(|v| v.as_array()).unwrap();
        assert_eq!(
            memories[0].get("summary").and_then(|v| v.as_str()).unwrap(),
            "新内容：山楂"
        );
    }

    #[tokio::test]
    async fn update_memory_empty_card_returns_400() {
        let st = test_state();
        let id = seed(&st, "苏菲", "x");
        let res = update_memory(
            State(st),
            Path(id.to_string()),
            Query(HashMap::new()),
            Json(UpdateMemoryRequest {
                content: "x".into(),
            }),
        )
        .await;
        assert_eq!(res.0, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn delete_memory_removed_and_logged() {
        let st = test_state();
        let sophie_id = seed(&st, "苏菲", "苏菲记忆");
        let dongbao_id = seed(&st, "冬堡", "冬堡记忆");

        let res = delete_memory(
            State(st.clone()),
            Path(sophie_id.to_string()),
            Query(HashMap::from([("card".into(), "苏菲".into())])),
        )
        .await;
        assert_eq!(res.0, StatusCode::OK);

        // 跨角色误删同 id：不应生效
        let res = delete_memory(
            State(st.clone()),
            Path(sophie_id.to_string()),
            Query(HashMap::from([("card".into(), "冬堡".into())])),
        )
        .await;
        assert_eq!(res.0, StatusCode::NOT_FOUND);

        let res = search_memories(
            State(st.clone()),
            Query(HashMap::from([("card".into(), "苏菲".into())])),
        )
        .await;
        assert_eq!(res.0, StatusCode::OK);
        let body = res.1 .0;
        let memories = body.get("memories").and_then(|v| v.as_array()).unwrap();
        assert!(memories.is_empty());

        // 冬堡记忆还在（用同一份 state，新建 state 里不会有前面写入的记忆）
        let res = search_memories(
            State(st.clone()),
            Query(HashMap::from([("card".into(), "冬堡".into())])),
        )
        .await;
        let body = res.1 .0;
        let memories = body.get("memories").and_then(|v| v.as_array()).unwrap();
        assert_eq!(memories.len(), 1);
        assert_eq!(
            memories[0].get("id").and_then(|v| v.as_i64()).unwrap(),
            dongbao_id
        );
    }

    #[tokio::test]
    async fn delete_memory_empty_card_returns_400() {
        let st = test_state();
        let res = delete_memory(State(st), Path("1".into()), Query(HashMap::new())).await;
        assert_eq!(res.0, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn add_memory_manual_and_recalled() {
        let st = test_state();
        let res = add_memory(
            State(st.clone()),
            Json(AddMemoryRequest {
                card: "苏菲".into(),
                content: "苏菲的秘密基地在冬堡图书馆顶层".into(),
                title: Some("秘密基地".into()),
            }),
        )
        .await;
        assert_eq!(res.0, StatusCode::OK);
        let body = res.1 .0;
        let id = body.get("id").and_then(|v| v.as_i64()).unwrap();
        assert!(id > 0);

        // 下次召回能检索到
        let res = search_memories(
            State(st),
            Query(HashMap::from([
                ("card".into(), "苏菲".into()),
                ("q".into(), "图书馆".into()),
            ])),
        )
        .await;
        assert_eq!(res.0, StatusCode::OK);
        let body = res.1 .0;
        let memories = body.get("memories").and_then(|v| v.as_array()).unwrap();
        assert!(!memories.is_empty());
        assert!(memories.iter().any(|m| m
            .get("summary")
            .and_then(|v| v.as_str())
            .unwrap()
            .contains("图书馆顶层")));
    }

    #[tokio::test]
    async fn add_memory_empty_card_returns_400() {
        let st = test_state();
        let res = add_memory(
            State(st),
            Json(AddMemoryRequest {
                card: "".into(),
                content: "x".into(),
                title: None,
            }),
        )
        .await;
        assert_eq!(res.0, StatusCode::BAD_REQUEST);
    }
}
