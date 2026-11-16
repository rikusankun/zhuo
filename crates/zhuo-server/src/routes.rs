//! HTTP 路由：SSE 流式对话（含记忆注入）+ 历史 + 重置 + 卡列表 + 记忆卡片管理。

pub mod memory;

use axum::extract::{Path, Query, State};
use axum::response::sse::{Event, Sse};
use axum::Json;
use base64::Engine;
use futures_util::stream::Stream;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::convert::Infallible;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};
use zhuo_llm::{StreamConfig, StreamOutcome};

use crate::prompt::{
    assemble_rag_messages, estimate_prompt_tokens, minimal_system_prompt, prompt_chars,
    render_state, retrieval_block, system_prompt, MAX_PROMPT_CHARS,
};
use crate::services::memory_service;
use crate::state::ServerState;

/// 统一 SSE 流类型：盒装动态流，保证所有返回路径是同一个具体类型。
type SseStream = std::pin::Pin<Box<dyn Stream<Item = Result<Event, Infallible>> + Send>>;

#[derive(Deserialize)]
pub struct ChatRequest {
    pub card: String,
    #[serde(default)]
    pub chat_id: String,
    pub message: String,
    #[serde(default)]
    pub regenerate: bool,
    #[serde(default)]
    pub display_message: String,
    #[serde(default)]
    pub persona: Option<ChatPersona>,
    #[serde(default)]
    pub world_info: Vec<String>,
    #[serde(default)]
    pub lore_entries: Vec<ChatLoreEntry>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChatPersona {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
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

#[derive(Deserialize)]
pub struct CreateChatRequest {
    pub card: String,
    #[serde(default)]
    pub title: String,
}

#[derive(Deserialize)]
pub struct RenameChatRequest {
    pub title: String,
}

#[derive(Deserialize)]
pub struct ImportCardRequest {
    pub filename: String,
    pub data: String,
}

#[derive(Deserialize)]
pub struct CreateCardRequest {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub first_mes: String,
    #[serde(default)]
    pub system_prompt: String,
}

#[derive(Deserialize)]
pub struct ExportQuery {
    #[serde(default = "default_export_format")]
    pub format: String,
}
fn default_export_format() -> String {
    "json".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ConnectionSettings {
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
    "ollama".into()
}
fn default_temperature() -> f32 {
    0.8
}
fn default_max_tokens() -> u32 {
    1024
}
fn default_top_p() -> f32 {
    1.0
}
fn default_repetition_penalty() -> f32 {
    1.0
}
fn default_num_gpu() -> i64 {
    15
}

fn ollama_root(base_url: &str) -> String {
    base_url
        .trim()
        .trim_end_matches('/')
        .strip_suffix("/v1")
        .unwrap_or(base_url.trim().trim_end_matches('/'))
        .trim_end_matches('/')
        .to_string()
}

pub(crate) fn api_base_url(connection: &ConnectionSettings, ollama_native: bool) -> String {
    let base = connection.base_url.trim_end_matches('/');
    if !ollama_native && connection.provider == "ollama" && !base.ends_with("/v1") {
        format!("{base}/v1")
    } else {
        base.to_string()
    }
}

pub(crate) fn uses_native_ollama(connection: &ConnectionSettings, enabled: bool) -> bool {
    enabled && connection.provider == "ollama"
}

fn ollama_model_names(payload: &Value) -> Vec<String> {
    payload
        .get("models")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|model| model.get("name").and_then(Value::as_str))
        .map(str::to_string)
        .collect()
}

async fn fetch_ollama_models(base_url: &str) -> Result<Vec<String>, String> {
    let url = format!("{}/api/tags", ollama_root(base_url));
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| format!("create Ollama client: {e}"))?;
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("Ollama tags request failed: {e}"))?;
    if !response.status().is_success() {
        let status = response.status();
        let detail = match response.text().await {
            Ok(detail) => detail.chars().take(500).collect::<String>(),
            Err(e) => format!("could not read error response: {e}"),
        };
        return Err(format!("Ollama tags returned HTTP {status}: {detail}"));
    }
    let payload: Value = response
        .json()
        .await
        .map_err(|e| format!("parse Ollama tags response: {e}"))?;
    Ok(ollama_model_names(&payload))
}

/// 连接设置的合并视图：以 .env 为基线（ServerState 已读入 ZHUO_API_BASE /
/// ZHUO_API_KEY / ZHUO_MODEL），再用数据库里用户在设置面板保存的值覆盖。
/// 优先级：.env 显式配置的 api_key 最高，避免「只读数据库」导致 .env 的 key
/// 明明生效、接口却显示未配置的不一致。
pub(crate) fn connection_settings(st: &ServerState) -> ConnectionSettings {
    let mut cfg = ConnectionSettings {
        provider: "ollama".into(),
        base_url: ollama_root(&st.base_url),
        api_key: (*st.api_key).clone(),
        model: (*st.model).clone(),
        summary_model: (*st.summary_model).clone(),
        num_gpu: st.main_num_gpu,
        temperature: 0.8,
        max_tokens: 1024,
        top_p: 1.0,
        frequency_penalty: 0.0,
        presence_penalty: 0.0,
        repetition_penalty: 1.0,
    };
    let mem = st.memory.lock().unwrap_or_else(|e| e.into_inner());
    let raw_saved = mem.get_setting("connection").ok().flatten();
    let has_saved_summary_model = raw_saved
        .as_deref()
        .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
        .is_some_and(|value| value.get("summary_model").is_some());
    let saved = raw_saved.and_then(|s| serde_json::from_str::<ConnectionSettings>(&s).ok());
    if let Some(saved) = saved {
        if !saved.base_url.trim().is_empty() {
            cfg.base_url = saved.base_url;
        }
        if !saved.model.trim().is_empty() {
            cfg.model = saved.model;
        }
        if has_saved_summary_model {
            cfg.summary_model = saved.summary_model;
        }
        cfg.num_gpu = saved.num_gpu.clamp(0, 99);
        if !st.env_api_key && !saved.api_key.trim().is_empty() && saved.api_key != "••••••••"
        {
            cfg.api_key = saved.api_key;
        }

        cfg.provider = saved.provider;
        cfg.temperature = saved.temperature;
        cfg.max_tokens = saved.max_tokens;
        cfg.top_p = saved.top_p;
        cfg.frequency_penalty = saved.frequency_penalty;
        cfg.presence_penalty = saved.presence_penalty;
        cfg.repetition_penalty = saved.repetition_penalty;
    }
    if cfg.provider == "ollama" {
        cfg.base_url = ollama_root(&cfg.base_url);
    }
    cfg
}

pub async fn ollama_tags(State(st): State<ServerState>) -> Json<serde_json::Value> {
    match fetch_ollama_models(&connection_settings(&st).base_url).await {
        Ok(models) => Json(json!({"models": models})),
        Err(e) => Json(json!({"error": e})),
    }
}

/// 标注实际生效的 api_key 来源，便于前端与 .env 对账。
fn api_key_source(st: &ServerState) -> &'static str {
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

#[derive(Deserialize)]
pub struct ResetRequest {
    pub card: String,
    #[serde(default)]
    pub memories: bool,
}

#[derive(Deserialize)]
pub struct SetStateRequest {
    pub card: String,
    pub state: serde_json::Value,
}

fn prompt_payload(
    hits: &[zhuo_memory::PromptHit],
    prompt_chars: usize,
    estimated_tokens: usize,
    history_messages: usize,
) -> serde_json::Value {
    json!({
        "memory_hits": hits.iter().map(|hit| json!({
            "type": hit.kind,
            "title": hit.title,
            "summary": hit.content,
        })).collect::<Vec<_>>(),
        "prompt_chars": prompt_chars,
        "estimated_tokens": estimated_tokens,
        "history_messages": history_messages,
    })
}

fn card_lore_entries(
    card: &zhuo_card::Card,
    world: &str,
    worldbook: Option<&zhuo_card::WorldBook>,
    client_entries: &[ChatLoreEntry],
    legacy_world_info: &[String],
    history: Option<&[String]>,
) -> Vec<zhuo_memory::LoreEntry> {
    let mut entries = Vec::new();

    for (field, title, content) in [
        ("description", "角色背景", card.description.as_str()),
        ("personality", "角色性格", card.personality.as_str()),
        ("scenario", "故事场景", card.scenario.as_str()),
        ("system_prompt", "角色指令", card.system_prompt.as_str()),
        ("first_mes", "开场白", card.first_mes.as_str()),
        ("mes_example", "示例对话", card.mes_example.as_str()),
        (
            "post_history_instructions",
            "历史后指令",
            card.post_history_instructions.as_str(),
        ),
        ("creator_notes", "创作者注释", card.creator_notes.as_str()),
    ] {
        push_lore_entry(
            &mut entries,
            format!("card:{field}"),
            title.into(),
            card.name.clone(),
            content,
            false,
        );
    }
    for (index, greeting) in card.alternate_greetings.iter().enumerate() {
        push_lore_entry(
            &mut entries,
            format!("card:alternate_greeting:{index}"),
            format!("替代开场白 {}", index + 1),
            card.name.clone(),
            greeting,
            false,
        );
    }
    if !card.tags.is_empty() {
        push_lore_entry(
            &mut entries,
            "card:tags".into(),
            "角色标签".into(),
            card.name.clone(),
            &card.tags.join(" "),
            false,
        );
    }

    if let Some(book) = &card.character_book {
        append_worldbook(&mut entries, "card-book", book, history);
    }
    if let Some(book) = worldbook {
        append_worldbook(&mut entries, "imported-book", book, history);
    }
    for (index, content) in world.split("\n\n").enumerate() {
        push_lore_entry(
            &mut entries,
            format!("world:{index}"),
            format!("世界设定 {}", index + 1),
            card.name.clone(),
            content,
            false,
        );
    }
    for (index, entry) in client_entries.iter().enumerate() {
        if !entry.enabled {
            continue;
        }
        if let Some(history) = history {
            let latest = history.last().map(|s| s.to_lowercase()).unwrap_or_default();
            if !entry
                .keys
                .split([',', '，'])
                .map(str::trim)
                .any(|key| !key.is_empty() && latest.contains(&key.to_lowercase()))
            {
                continue;
            }
        }
        push_lore_entry(
            &mut entries,
            format!("client-lore:{index}:{}", entry.id),
            "自定义世界书".into(),
            entry.keys.clone(),
            &entry.content,
            history.is_some(),
        );
    }
    for (index, content) in legacy_world_info.iter().enumerate() {
        push_lore_entry(
            &mut entries,
            format!("legacy-world-info:{index}"),
            "旧版世界信息".into(),
            card.name.clone(),
            content,
            false,
        );
    }
    entries
}

fn push_lore_entry(
    out: &mut Vec<zhuo_memory::LoreEntry>,
    source_key: String,
    title: String,
    triggers: String,
    content: &str,
    constant: bool,
) {
    if !content.trim().is_empty() {
        out.push(zhuo_memory::LoreEntry {
            source_key,
            title,
            triggers,
            secondary_triggers: String::new(),
            content: content.trim().to_string(),
            constant,
            enabled: true,
            selective: false,
            case_sensitive: false,
            match_whole_words: true,
            order: 0,
            scan_depth: None,
        });
    }
}

fn append_worldbook(
    out: &mut Vec<zhuo_memory::LoreEntry>,
    source: &str,
    book: &zhuo_card::WorldBook,
    history: Option<&[String]>,
) {
    let active = history.map(|messages| zhuo_card::activated_entries(book, messages));
    for (index, entry) in book
        .entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| entry.enabled)
    {
        if active
            .as_ref()
            .is_some_and(|entries| !entries.contains(entry))
        {
            continue;
        }
        out.push(zhuo_memory::LoreEntry {
            source_key: format!("{source}:{index}:{}", entry.id),
            title: if entry.id.is_empty() {
                format!("{} 条目 {}", book.name, index + 1)
            } else {
                format!("{} {}", book.name, entry.id)
            },
            triggers: entry.keys.join(","),
            secondary_triggers: entry.secondary_keys.join(","),
            content: entry.content.clone(),
            constant: entry.constant || history.is_some(),
            enabled: entry.enabled,
            selective: entry.selective,
            case_sensitive: entry.case_sensitive,
            match_whole_words: entry.match_whole_words,
            order: entry.order,
            scan_depth: entry.scan_depth,
        });
    }
}

/// GET /api/cards —— 卡列表（id/名称/描述/首句/精简 system）。id 是内容短 hash。
pub async fn cards(State(st): State<ServerState>) -> Json<serde_json::Value> {
    let list = st.list_cards();
    Json(json!(list
        .iter()
        .map(|c| {
            json!({
                "id": c.id,
                "name": c.name,
                "description": c.description,
                "first_mes": c.first_mes,
                "mes_example": c.mes_example,
                "system": system_prompt(c, ""),
            })
        })
        .collect::<Vec<_>>()))
}

pub async fn create_card(
    State(st): State<ServerState>,
    Json(req): Json<CreateCardRequest>,
) -> Json<serde_json::Value> {
    let name = req.name.trim();
    if name.is_empty() {
        return Json(json!({"error":"角色名称不能为空"}));
    }

    let raw = json!({
        "name": name,
        "description": req.description.trim(),
        "first_mes": req.first_mes.trim(),
        "system_prompt": req.system_prompt.trim(),
    });
    let card = match zhuo_card::Card::from_json(raw) {
        Ok(card) => card,
        Err(e) => return Json(json!({"error":format!("角色信息无效：{e:#}")})),
    };
    if st.find_card(&card.id).is_ok() {
        return Json(json!({"error":"相同内容的角色已存在，或角色 ID 冲突"}));
    }
    let path = st.cards_dir.join(format!("{}.json", card.id));
    let bytes = match serde_json::to_vec_pretty(&card.raw) {
        Ok(bytes) => bytes,
        Err(e) => return Json(json!({"error":format!("序列化角色卡失败：{e}")})),
    };
    if let Err(e) = std::fs::create_dir_all(&*st.cards_dir) {
        return Json(json!({"error":format!("保存角色卡失败：{e}")}));
    }
    let mut file = match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
    {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            return Json(json!({"error":"相同内容的角色已存在"}))
        }
        Err(e) => return Json(json!({"error":format!("保存角色卡失败：{e}")})),
    };
    let write_result = std::io::Write::write_all(&mut file, &bytes);
    drop(file);
    if let Err(e) = write_result {
        if let Err(remove_error) = std::fs::remove_file(&path) {
            return Json(json!({
                "error":format!("保存角色卡失败：{e}；清理不完整文件也失败：{remove_error}")
            }));
        }
        return Json(json!({"error":format!("保存角色卡失败：{e}")}));
    }

    let result = (|| -> anyhow::Result<()> {
        let mem = st.memory.lock().unwrap_or_else(|e| e.into_inner());
        let worldbook = match mem.get_setting("worldbook")? {
            Some(raw) => Some(serde_json::from_str::<zhuo_card::WorldBook>(&raw)?),
            None => None,
        };
        let entries = card_lore_entries(&card, &st.world, worldbook.as_ref(), &[], &[], None);
        mem.replace_lore_entries(&card.id, &entries)?;
        Ok(())
    })();
    if let Err(e) = result {
        if let Err(remove_error) = std::fs::remove_file(&path) {
            return Json(json!({
                "error":format!("索引角色设定失败：{e:#}；清理角色卡文件也失败：{remove_error}")
            }));
        }
        return Json(json!({"error":format!("索引角色设定失败：{e:#}")}));
    }

    Json(json!({
        "ok": true,
        "card": {
            "id": card.id,
            "name": card.name,
            "description": card.description,
            "first_mes": card.first_mes,
        }
    }))
}

pub async fn delete_card(
    State(st): State<ServerState>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    let card = match st.find_card(&id) {
        Ok(card) if card.id == id => card,
        Ok(_) => return Json(json!({"error":"删除角色必须使用角色 ID"})),
        Err(e) => return Json(json!({"error":format!("{e:#}")})),
    };
    let paths = st.card_source_paths(&card.id);
    if paths.is_empty() {
        return Json(json!({"error":"找不到角色卡源文件"}));
    }
    let Some(_guard) = st.extraction_guard(&card.id) else {
        return Json(json!({"error":"角色正在整理记忆，请稍后再删除"}));
    };
    let legacy_name = (st
        .list_cards()
        .iter()
        .filter(|candidate| candidate.name == card.name)
        .count()
        == 1)
        .then_some(card.name.as_str());

    let result = (|| -> anyhow::Result<()> {
        let sources = paths
            .iter()
            .map(|path| std::fs::read(path).map(|bytes| (path, bytes)))
            .collect::<std::io::Result<Vec<_>>>()?;
        for (index, (path, _)) in sources.iter().enumerate() {
            if let Err(error) = std::fs::remove_file(path) {
                for (removed, bytes) in &sources[..index] {
                    std::fs::write(removed, bytes)?;
                }
                anyhow::bail!("删除角色卡失败，已恢复文件：{error}");
            }
        }

        let delete_result = st
            .memory
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .delete_card_data(&card.id, legacy_name);
        if let Err(error) = delete_result {
            for (path, source) in &sources {
                if let Err(restore_error) = std::fs::write(path, source) {
                    anyhow::bail!(
                        "删除角色数据失败：{error:#}；恢复角色卡文件也失败：{restore_error}"
                    );
                }
            }
            return Err(anyhow::anyhow!(
                "删除角色数据失败，角色卡文件已恢复：{error:#}"
            ));
        }
        Ok(())
    })();

    match result {
        Ok(()) => Json(json!({"ok":true})),
        Err(e) => Json(json!({"error":format!("{e:#}")})),
    }
}

pub async fn import_card(
    State(st): State<ServerState>,
    Json(req): Json<ImportCardRequest>,
) -> Json<serde_json::Value> {
    let bytes = match base64::engine::general_purpose::STANDARD.decode(req.data.trim()) {
        Ok(v) => v,
        Err(e) => return Json(json!({"error": format!("角色卡不是有效的 Base64：{e}")})),
    };
    let raw = match zhuo_card::parse_card_bytes(&bytes) {
        Ok(v) => v,
        Err(e) => return Json(json!({"error": format!("无法解析角色卡：{e:#}")})),
    };
    let card = match zhuo_card::Card::from_json(raw) {
        Ok(v) => v,
        Err(e) => return Json(json!({"error": format!("角色卡字段无效：{e:#}")})),
    };
    let is_png = bytes.starts_with(b"\x89PNG\r\n\x1a\n");
    if let Ok(existing) = st.find_card(&card.id) {
        if existing.raw != card.raw {
            return Json(json!({"error":"角色 ID 冲突，未覆盖已有角色"}));
        }
    }
    let ext = if is_png { "png" } else { "json" };
    let path = st.cards_dir.join(format!("{}.{}", card.id, ext));
    let write_result = (|| -> std::io::Result<bool> {
        std::fs::create_dir_all(&*st.cards_dir)?;
        let mut file = match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                if zhuo_card::load_card(&path).is_ok_and(|existing| existing.raw == card.raw) {
                    return Ok(false);
                }
                return Err(e);
            }
            Err(e) => return Err(e),
        };
        let result = std::io::Write::write_all(&mut file, &bytes);
        drop(file);
        if let Err(e) = result {
            std::fs::remove_file(&path)?;
            return Err(e);
        }
        Ok(true)
    })();
    let created = match write_result {
        Ok(created) => created,
        Err(e) => {
            return Json(json!({"error": format!("保存角色卡失败：{e}")}));
        }
    };
    let index_result = (|| -> anyhow::Result<()> {
        let mem = st.memory.lock().unwrap_or_else(|e| e.into_inner());
        let worldbook = mem
            .get_setting("worldbook")?
            .map(|raw| serde_json::from_str::<zhuo_card::WorldBook>(&raw))
            .transpose()?;
        let entries = card_lore_entries(&card, &st.world, worldbook.as_ref(), &[], &[], None);
        mem.replace_lore_entries(&card.id, &entries)
    })();
    if let Err(e) = index_result {
        if created {
            if let Err(cleanup) = std::fs::remove_file(&path) {
                return Json(
                    json!({"error": format!("索引角色设定失败：{e:#}；清理文件失败：{cleanup}")}),
                );
            }
        }
        return Json(json!({"error": format!("索引角色设定失败：{e:#}")}));
    }
    Json(
        json!({"ok": true, "card": {"id": card.id, "name": card.name, "description": card.description, "first_mes": card.first_mes}, "filename": req.filename}),
    )
}

pub async fn export_card(
    State(st): State<ServerState>,
    Path(id): Path<String>,
    Query(q): Query<ExportQuery>,
) -> Json<serde_json::Value> {
    let card = match st.find_card(&id) {
        Ok(c) => c,
        Err(e) => return Json(json!({"error":format!("{e:#}")})),
    };
    let v2 = card.to_v2_json();
    if q.format.eq_ignore_ascii_case("png") {
        let Some(path) = st.card_source_path(&card.id) else {
            return Json(json!({"error":"找不到角色卡源文件"}));
        };
        let source = match std::fs::read(path) {
            Ok(v) => v,
            Err(e) => return Json(json!({"error":e.to_string()})),
        };
        if !source.starts_with(b"\x89PNG") {
            return Json(json!({"error":"该角色没有 PNG 源图，请导出 V2 JSON"}));
        }
        match zhuo_card::write_v2_png(&source, &v2) {
            Ok(bytes) => Json(
                json!({"filename":format!("{}.png",card.name),"mime":"image/png","data":base64::engine::general_purpose::STANDARD.encode(bytes)}),
            ),
            Err(e) => Json(json!({"error":format!("{e:#}")})),
        }
    } else {
        let bytes = serde_json::to_vec_pretty(&v2).unwrap_or_default();
        Json(
            json!({"filename":format!("{}.json",card.name),"mime":"application/json","data":base64::engine::general_purpose::STANDARD.encode(bytes)}),
        )
    }
}

pub async fn import_worldbook(
    State(st): State<ServerState>,
    Json(req): Json<ImportCardRequest>,
) -> Json<serde_json::Value> {
    let bytes = match base64::engine::general_purpose::STANDARD.decode(req.data.trim()) {
        Ok(v) => v,
        Err(e) => return Json(json!({"error":e.to_string()})),
    };
    let book = match zhuo_card::parse_world_book_bytes(&bytes) {
        Ok(v) => v,
        Err(e) => return Json(json!({"error":format!("{e:#}")})),
    };
    let raw = serde_json::to_string(&book).unwrap_or_default();
    let count = book.entries.len();
    let mem = st.memory.lock().unwrap_or_else(|e| e.into_inner());
    match mem.set_setting("worldbook", &raw) {
        Ok(()) => {
            for card in st.list_cards() {
                let entries = card_lore_entries(&card, &st.world, Some(&book), &[], &[], None);
                if let Err(e) = mem.replace_lore_entries(&card.id, &entries) {
                    return Json(json!({"error": format!("保存世界书索引失败：{e:#}")}));
                }
            }
            Json(json!({"ok":true,"name":book.name,"entries":count}))
        }
        Err(e) => Json(json!({"error":format!("{e:#}")})),
    }
}

pub async fn import_preset(
    State(st): State<ServerState>,
    Json(req): Json<ImportCardRequest>,
) -> Json<serde_json::Value> {
    let bytes = match base64::engine::general_purpose::STANDARD.decode(req.data.trim()) {
        Ok(v) => v,
        Err(e) => return Json(json!({"error":e.to_string()})),
    };
    let v: serde_json::Value = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(e) => return Json(json!({"error":format!("预设 JSON 无效：{e}")})),
    };
    let mut cfg = connection_settings(&st);
    if let Some(n) = v.get("temperature").and_then(Value::as_f64) {
        cfg.temperature = n as f32
    }
    if let Some(n) = v.get("top_p").and_then(Value::as_f64) {
        cfg.top_p = n as f32
    }
    if let Some(n) = v.get("frequency_penalty").and_then(Value::as_f64) {
        cfg.frequency_penalty = n as f32
    }
    if let Some(n) = v.get("presence_penalty").and_then(Value::as_f64) {
        cfg.presence_penalty = n as f32
    }
    if let Some(n) = v
        .get("repetition_penalty")
        .or_else(|| v.get("rep_pen"))
        .and_then(Value::as_f64)
    {
        cfg.repetition_penalty = n as f32
    }
    if let Some(n) = v
        .get("openai_max_tokens")
        .or_else(|| v.get("max_tokens"))
        .and_then(Value::as_u64)
    {
        cfg.max_tokens = n.clamp(1, 32768) as u32
    }
    if let Some(m) = v
        .get("openai_model")
        .or_else(|| v.get("custom_model"))
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
    {
        cfg.model = m.into()
    }
    let result = set_connection(State(st.clone()), Json(cfg)).await;
    if result.0.get("error").is_some() {
        return result;
    }
    let Json(mut response) = get_connection(State(st)).await;
    response["ok"] = json!(true);
    Json(response)
}

pub async fn import_regex(
    State(st): State<ServerState>,
    Json(req): Json<ImportCardRequest>,
) -> Json<serde_json::Value> {
    let bytes = match base64::engine::general_purpose::STANDARD.decode(req.data.trim()) {
        Ok(v) => v,
        Err(e) => return Json(json!({"error":e.to_string()})),
    };
    let value: Value = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(e) => return Json(json!({"error":format!("Regex JSON 无效：{e}")})),
    };
    let scripts: Vec<RegexScript> = if value.is_array() {
        serde_json::from_value(value).unwrap_or_default()
    } else {
        serde_json::from_value(value)
            .map(|v| vec![v])
            .unwrap_or_default()
    };
    if scripts.is_empty() {
        return Json(json!({"error":"未找到有效的 ST Regex 脚本"}));
    }
    let mem = st.memory.lock().unwrap_or_else(|e| e.into_inner());
    match mem.set_setting("regex_scripts", &serde_json::to_string(&scripts).unwrap()) {
        Ok(()) => Json(json!({"ok":true,"scripts":scripts})),
        Err(e) => Json(json!({"error":format!("{e:#}")})),
    }
}

pub async fn compatibility(State(st): State<ServerState>) -> Json<serde_json::Value> {
    let mem = st.memory.lock().unwrap_or_else(|e| e.into_inner());
    let book = mem
        .get_setting("worldbook")
        .ok()
        .flatten()
        .and_then(|s| serde_json::from_str::<zhuo_card::WorldBook>(&s).ok());
    let scripts = mem
        .get_setting("regex_scripts")
        .ok()
        .flatten()
        .and_then(|s| serde_json::from_str::<Vec<RegexScript>>(&s).ok())
        .unwrap_or_default();
    Json(json!({"worldbook":book,"regex_scripts":scripts}))
}

/// POST /api/chat —— SSE 流式对话。
/// 帧序：{"memory_hits":[...]} → {"token":"..."}* → {"usage":{...}} → {"error":...?} → [DONE]
pub async fn chat(State(st): State<ServerState>, Json(req): Json<ChatRequest>) -> Sse<SseStream> {
    if req.message.trim().is_empty() {
        return sse_error("消息不能为空".into());
    }
    // 1. 卡 + 状态 + 历史 + 记忆检索（短临界区，不跨 await 持锁）
    //    存储键一律用卡 id（内容短 hash）——同名卡彻底隔离
    let (
        card_key,
        active_chat,
        expected_last_id,
        msgs,
        prompt_hits,
        history_count,
        char_name,
        user_name,
    ) = {
        let mem = st.memory.lock().unwrap_or_else(|e| e.into_inner());
        let card = match st.find_card(&req.card) {
            Ok(c) => c,
            Err(e) => return sse_error(format!("{e:#}")),
        };
        let chat_id = if req.chat_id.is_empty() {
            format!("legacy-{}", card.id)
        } else {
            req.chat_id.clone()
        };
        let active_chat = match mem.get_chat(&chat_id) {
            Ok(Some(c)) if c.card == card.id => c,
            Ok(Some(_)) => return sse_error("会话不属于当前角色".into()),
            Ok(None) if req.chat_id.is_empty() => {
                match mem.create_chat(&chat_id, &card.id, "默认对话") {
                    Ok(c) => c,
                    Err(e) => return sse_error(format!("create chat: {e:#}")),
                }
            }
            Ok(None) => return sse_error("会话不存在，请刷新后重试".into()),
            Err(e) => return sse_error(format!("read chat: {e:#}")),
        };
        let expected_last_id = match mem.last_chat_message_id(&chat_id) {
            Ok(id) => id,
            Err(e) => return sse_error(format!("read last message: {e:#}")),
        };
        let mut history = match mem.chat_history(&chat_id, 200) {
            Ok(h) => h,
            Err(e) => return sse_error(format!("read history: {e:#}")),
        };
        if req.regenerate {
            let len = history.len();
            if len < 2
                || history[len - 1].role != "assistant"
                || history[len - 2].role != "user"
                || history[len - 2].content != req.message
            {
                return sse_error("只能重新生成最后一轮已保存的回复".into());
            }
            history.truncate(len - 2);
        }
        let worldbook = match mem.get_setting("worldbook") {
            Ok(Some(raw)) => match serde_json::from_str::<zhuo_card::WorldBook>(&raw) {
                Ok(book) => Some(book),
                Err(e) => return sse_error(format!("parse saved worldbook: {e}")),
            },
            Ok(None) => None,
            Err(e) => return sse_error(format!("read saved worldbook: {e:#}")),
        };
        let mut scan: Vec<_> = history.iter().map(|m| m.content.clone()).collect();
        scan.push(req.message.clone());
        let lore = card_lore_entries(
            &card,
            &st.world,
            worldbook.as_ref(),
            &req.lore_entries,
            &req.world_info,
            Some(&scan),
        );
        if let Err(e) = mem.replace_lore_entries(&card.id, &lore) {
            return sse_error(format!("index character lore: {e:#}"));
        }
        let hits = match mem.search_prompt_context(&card.id, &req.message) {
            Ok(hits) => hits,
            Err(e) => return sse_error(format!("retrieve prompt context: {e:#}")),
        };
        let retrieval = retrieval_block(&hits);
        let state = match mem.get_state(&card.id) {
            Ok(state) => state.and_then(|state| render_state(&state)),
            Err(e) => return sse_error(format!("read character state: {e:#}")),
        };
        let persona = req
            .persona
            .as_ref()
            .map(|persona| (persona.name.as_str(), persona.description.as_str()));
        let system = minimal_system_prompt(&card, persona);
        let messages = assemble_rag_messages(
            &system,
            retrieval.as_deref(),
            state.as_deref(),
            &history,
            &req.message,
        );
        let history_count = messages
            .iter()
            .filter(|message| message.role == "user" || message.role == "assistant")
            .count()
            .saturating_sub(1);
        let chars = prompt_chars(&messages);
        if chars > MAX_PROMPT_CHARS {
            return sse_error(format!(
                "Prompt 长度限制失效：{chars} 字，硬性上限为 {MAX_PROMPT_CHARS} 字"
            ));
        }
        eprintln!(
            "[prompt] card={} chars={} estimated_tokens={} retrieved={} history={}",
            card.name,
            chars,
            estimate_prompt_tokens(&messages),
            hits.len(),
            history_count
        );
        (
            card.id,
            active_chat.id,
            expected_last_id,
            messages,
            hits,
            history_count,
            card.name.clone(),
            persona
                .map(|(n, _)| n.to_string())
                .unwrap_or_else(|| "用户".to_string()),
        )
    };

    // 2. 通道 + 第一帧：本次注入的记忆（前端展示"已注入 N 条相关记忆"）
    let (tx, rx) = unbounded_channel::<String>();
    let prompt_size = prompt_chars(&msgs);
    let estimated_tokens = estimate_prompt_tokens(&msgs);
    let _ = tx.send(
        prompt_payload(&prompt_hits, prompt_size, estimated_tokens, history_count).to_string(),
    );

    // 3. 单段 spawn：调 Ollama 流式，token 转发；成功后消息与 token 用量入库
    let connection = connection_settings(&st);
    let native = uses_native_ollama(&connection, st.ollama_native);
    let base = api_base_url(&connection, native);
    let key = connection.api_key;
    let model = connection.model;
    let st2 = st.clone();
    let input = if req.display_message.trim().is_empty() {
        req.message.clone()
    } else {
        req.display_message.clone()
    };
    let regex_scripts: Vec<RegexScript> = st
        .memory
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get_setting("regex_scripts")
        .ok()
        .flatten()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default();

    tokio::spawn(async move {
        let cfg = StreamConfig {
            base_url: base.as_str(),
            api_key: key.as_str(),
            model: model.as_str(),
            include_usage: st2.include_usage,
            dump_path: None,
            response_format: None,
            temperature: Some(connection.temperature),
            max_tokens: Some(connection.max_tokens),
            top_p: Some(connection.top_p),
            frequency_penalty: Some(connection.frequency_penalty),
            presence_penalty: Some(connection.presence_penalty),
            repetition_penalty: Some(connection.repetition_penalty),
            // 主模型按 ZHUO_MAIN_NUM_GPU 分配 GPU 层数（默认 99）
            num_gpu: (connection.provider == "ollama").then_some(connection.num_gpu),
            ollama_native: native,
        };
        // token 转发；send 失败 = 客户端已断开（点「停止生成」或关页面）→ 返回 false
        // 中断上游 Ollama 请求，已生成的部分内容仍会入库
        let mut partial = String::new();
        let result = {
            let generation = zhuo_llm::stream_chat_ex_with_first_token_timeout(
                &cfg,
                &msgs,
                Some(std::time::Duration::from_secs(30)),
                |tok| {
                    partial.push_str(tok);
                    tx.send(json!({ "token": tok }).to_string()).is_ok()
                },
            );
            tokio::select! {
                result = generation => Some(result),
                _ = tx.closed() => None,
            }
        };
        let result = match result {
            None => Ok(StreamOutcome {
                content: partial,
                aborted: true,
                ..Default::default()
            }),
            Some(Err(e)) if !partial.is_empty() => {
                let _ = tx.send(json!({"error": generation_error_message(&e)}).to_string());
                Ok(StreamOutcome {
                    content: partial,
                    aborted: true,
                    ..Default::default()
                })
            }
            Some(result) => result,
        };

        match result {
            Ok(StreamOutcome { content, usage, .. }) if !content.is_empty() => {
                let mut processed = apply_output_regex(&content, &regex_scripts);
                for token in ["{{user}}", "{{User}}", "{{USER}}"] {
                    processed = processed.replace(token, &user_name);
                }
                for token in ["{{char}}", "{{Char}}", "{{CHAR}}"] {
                    processed = processed.replace(token, &char_name);
                }
                let mem = st2.memory.lock().unwrap_or_else(|e| e.into_inner());
                if let Err(e) = mem.commit_chat_turn(
                    &active_chat,
                    &card_key,
                    &input,
                    &processed,
                    expected_last_id,
                    req.regenerate,
                ) {
                    let _ = tx.send(json!({"error":format!("保存对话失败：{e:#}")}).to_string());
                    let _ = tx.send("[DONE]".into());
                    return;
                }
                drop(mem);
                let _ =
                    tx.send(json!({"processed": processed, "raw_content": content}).to_string());
                if let Some(u) = usage {
                    let _ = tx.send(json!({ "usage": u }).to_string());
                }

                // 后台记忆提取：满 N 轮未处理即触发（per-card 防并发；失败不影响主流程）
                if !connection_settings(&st2).summary_model.trim().is_empty() {
                    let st3 = st2.clone();
                    let cn = card_key.clone();
                    tokio::spawn(async move {
                        let r = memory_service::summarize_card(&st3, &cn).await;
                        match r {
                            Ok(memory_service::SummaryOutcome::Completed(s)) => {
                                eprintln!("[summarize] {cn}: 自动提取 -> {s}")
                            }
                            Ok(memory_service::SummaryOutcome::Disabled)
                            | Ok(memory_service::SummaryOutcome::Busy)
                            | Ok(memory_service::SummaryOutcome::Skipped) => {}
                            Err(e) => eprintln!("[summarize] {cn}: 提取失败(已跳过): {e:#}"),
                        }
                    });
                }
            }
            Ok(_) => {
                let _ = tx.send(json!({ "error": "空回复" }).to_string());
            }
            Err(e) => {
                let _ = tx.send(json!({ "error": generation_error_message(&e) }).to_string());
            }
        }
        let _ = tx.send("[DONE]".into());
    });

    let stream = unfold_rx(rx);
    Sse::new(stream)
}

fn generation_error_message(error: &anyhow::Error) -> String {
    if error.to_string() == zhuo_llm::FIRST_TOKEN_TIMEOUT_ERROR {
        "生成超时，请检查上下文或降低显存层数".to_string()
    } else {
        format!("{error:#}")
    }
}

fn apply_output_regex(input: &str, scripts: &[RegexScript]) -> String {
    scripts
        .iter()
        .filter(|s| {
            !s.disabled
                && !s.prompt_only
                && !s.find_regex.trim().is_empty()
                && (s.placement.is_empty() || s.placement.contains(&2))
        })
        .fold(input.to_string(), |text, script| {
            let raw = script.find_regex.trim();
            let (pattern, flags) = if raw.starts_with('/') && raw[1..].rfind('/').is_some() {
                let end = raw.rfind('/').unwrap();
                (&raw[1..end], &raw[end + 1..])
            } else {
                (raw, "")
            };
            let mut builder = regex::RegexBuilder::new(pattern);
            builder
                .case_insensitive(flags.contains('i'))
                .multi_line(flags.contains('m'))
                .dot_matches_new_line(flags.contains('s'));
            let Ok(re) = builder.build() else { return text };
            let replacement = script
                .replace_string
                .replace("{{match}}", "$0")
                .replace("$&", "$0");
            if flags.contains('g') {
                re.replace_all(&text, replacement.as_str()).to_string()
            } else {
                re.replace(&text, replacement.as_str()).to_string()
            }
        })
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

pub async fn list_chats(
    State(st): State<ServerState>,
    Query(q): Query<HashMap<String, String>>,
) -> Json<serde_json::Value> {
    let card = q.get("card").map(|s| st.card_key(s));
    let mem = st.memory.lock().unwrap_or_else(|e| e.into_inner());
    match mem.list_chats(card.as_deref()) {
        Ok(v) => Json(json!(v)),
        Err(e) => Json(json!({"error": format!("{e:#}")})),
    }
}

pub async fn create_chat(
    State(st): State<ServerState>,
    Json(req): Json<CreateChatRequest>,
) -> Json<serde_json::Value> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let card = match st.find_card(&req.card) {
        Ok(v) => v,
        Err(e) => return Json(json!({"error": format!("{e:#}")})),
    };
    let id = format!(
        "chat-{}-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    );
    let title = if req.title.trim().is_empty() {
        "新对话"
    } else {
        req.title.trim()
    };
    let mem = st.memory.lock().unwrap_or_else(|e| e.into_inner());
    match mem.create_chat(&id, &card.id, title) {
        Ok(chat) => {
            if !card.first_mes.trim().is_empty() {
                let mut first = card.first_mes.clone();
                for token in ["{{user}}", "{{User}}", "{{USER}}"] {
                    first = first.replace(token, "用户");
                }
                for token in ["{{char}}", "{{Char}}", "{{CHAR}}"] {
                    first = first.replace(token, &card.name);
                }
                let _ = mem.add_chat_message(&id, &card.id, "assistant", &first);
            }
            Json(json!(chat))
        }
        Err(e) => Json(json!({"error": format!("{e:#}")})),
    }
}

pub async fn rename_chat(
    State(st): State<ServerState>,
    Path(id): Path<String>,
    Json(req): Json<RenameChatRequest>,
) -> Json<serde_json::Value> {
    if req.title.trim().is_empty() {
        return Json(json!({"error":"标题不能为空"}));
    }
    let mem = st.memory.lock().unwrap_or_else(|e| e.into_inner());
    match mem.rename_chat(&id, req.title.trim()) {
        Ok(n) => Json(json!({"ok": n > 0})),
        Err(e) => Json(json!({"error":format!("{e:#}")})),
    }
}

pub async fn delete_chat(
    State(st): State<ServerState>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    let mem = st.memory.lock().unwrap_or_else(|e| e.into_inner());
    match mem.delete_chat(&id) {
        Ok(n) => Json(json!({"ok": n > 0})),
        Err(e) => Json(json!({"error":format!("{e:#}")})),
    }
}

pub async fn get_connection(State(st): State<ServerState>) -> Json<serde_json::Value> {
    let mut cfg = connection_settings(&st);
    // Ollama 本地端点的 "ollama" 也是已配置的 key，不能再被当成「未配置」占位符。
    let configured = !cfg.api_key.trim().is_empty();
    cfg.api_key = if configured {
        "••••••••".into()
    } else {
        String::new()
    };
    Json(json!({
        "connection": cfg,
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
    if !matches!(cfg.provider.as_str(), "ollama" | "openai") {
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
    // 前端回填的是掩码或空串 → 保留实际生效的 key（.env 优先），不要把它抹掉。
    if cfg.api_key.trim().is_empty() || cfg.api_key == "••••••••" {
        cfg.api_key = connection_settings(&st).api_key;
    }
    cfg.temperature = cfg.temperature.clamp(0.0, 2.0);
    cfg.max_tokens = cfg.max_tokens.clamp(1, 32768);
    cfg.top_p = cfg.top_p.clamp(0.0, 1.0);
    cfg.frequency_penalty = cfg.frequency_penalty.clamp(-2.0, 2.0);
    cfg.presence_penalty = cfg.presence_penalty.clamp(-2.0, 2.0);
    cfg.repetition_penalty = cfg.repetition_penalty.clamp(0.0, 2.0);
    cfg.num_gpu = cfg.num_gpu.clamp(0, 99);
    let raw = serde_json::to_string(&cfg).unwrap_or_default();
    let mem = st.memory.lock().unwrap_or_else(|e| e.into_inner());
    match mem.set_setting("connection", &raw) {
        Ok(()) => Json(json!({"ok":true})),
        Err(e) => Json(json!({"error":format!("{e:#}")})),
    }
}

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
/// （POST /api/reset 是老入口，保留兼容；前端「新会话」按钮走这个。）
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

/// PUT /api/state —— 手动更新角色状态（蓝图 Phase 4：状态自动提取暂不做，先手动验证注入）
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

/// GET / —— 健康检查
pub async fn index() -> Json<serde_json::Value> {
    Json(json!({"status": "zhuo-server ok"}))
}

fn sse_error(msg: String) -> Sse<SseStream> {
    let (tx, rx) = unbounded_channel::<String>();
    let _ = tx.send(json!({ "error": msg }).to_string());
    let _ = tx.send("[DONE]".into());
    Sse::new(unfold_rx(rx))
}

/// unbounded_channel → SSE 流：同步 send 在 async runtime 内安全（非 blocking_send）。
fn unfold_rx(rx: UnboundedReceiver<String>) -> SseStream {
    Box::pin(futures_util::stream::unfold(rx, |mut rx| async move {
        rx.recv()
            .await
            .map(|payload| (Ok(Event::default().data(payload)), rx))
    }))
}

#[cfg(test)]
mod compatibility_tests {
    use super::*;

    #[test]
    fn remote_provider_never_uses_ollama_native_endpoint() {
        let settings: ConnectionSettings = serde_json::from_value(json!({
            "provider":"openai", "base_url":"https://example.com/v1", "model":"test"
        }))
        .unwrap();
        assert!(!uses_native_ollama(&settings, true));
        assert_eq!(api_base_url(&settings, false), "https://example.com/v1");
    }

    #[test]
    fn chat_lore_obeys_worldbook_activation_before_retrieval() {
        let card = zhuo_card::Card::from_json(json!({"name":"test", "character_book":{
            "scan_depth":1, "entries":[{"id":"secret", "keys":["Rose"],
                "secondary_keys":["garden"], "selective":true, "case_sensitive":true,
                "content":"secret rule"}]}}))
        .unwrap();
        let miss = vec!["Rose garden".to_string(), "rose garden".to_string()];
        let hit = vec!["Rose garden".to_string()];
        let entries = card_lore_entries(&card, "", None, &[], &[], Some(&miss));
        assert!(!entries.iter().any(|e| e.content == "secret rule"));
        let entries = card_lore_entries(&card, "", None, &[], &[], Some(&hit));
        assert!(entries
            .iter()
            .any(|e| e.content == "secret rule" && e.constant));
    }

    #[test]
    fn ollama_root_accepts_openai_compatible_base_urls() {
        assert_eq!(
            ollama_root("http://127.0.0.1:11434/v1/"),
            "http://127.0.0.1:11434"
        );
        assert_eq!(
            ollama_root("http://127.0.0.1:11434"),
            "http://127.0.0.1:11434"
        );
    }

    #[test]
    fn non_native_ollama_requests_keep_the_v1_path() {
        let connection: ConnectionSettings = serde_json::from_value(json!({
            "provider": "ollama",
            "base_url": "http://127.0.0.1:11434",
            "model": "main"
        }))
        .unwrap();
        assert_eq!(
            api_base_url(&connection, false),
            "http://127.0.0.1:11434/v1"
        );
        assert_eq!(api_base_url(&connection, true), "http://127.0.0.1:11434");
    }

    #[test]
    fn parses_ollama_model_names() {
        let models = ollama_model_names(&json!({
            "models": [
                {"name": "qwen:latest"},
                {"name": "llama:8b"},
                {"size": 123}
            ]
        }));
        assert_eq!(models, ["qwen:latest", "llama:8b"]);
    }

    #[tokio::test]
    async fn ollama_tags_proxy_requests_local_api_tags() {
        use std::io::{Read, Write};

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            let mut request = Vec::new();
            while !request.windows(4).any(|part| part == b"\r\n\r\n") {
                let mut chunk = [0; 1024];
                let count = stream.read(&mut chunk).unwrap();
                assert!(count > 0, "client closed before sending headers");
                request.extend_from_slice(&chunk[..count]);
            }
            let request = String::from_utf8_lossy(&request);
            assert!(request.starts_with("GET /api/tags "));
            let body = r#"{"models":[{"name":"qwen:latest"}]}"#;
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .unwrap();
        });

        let models = fetch_ollama_models(&format!("http://{address}/v1"))
            .await
            .unwrap();
        server.join().unwrap();
        assert_eq!(models, ["qwen:latest"]);
    }

    #[test]
    fn connection_settings_deserialize_old_and_new_model_fields() {
        let old: ConnectionSettings = serde_json::from_value(json!({
            "provider": "ollama",
            "base_url": "http://127.0.0.1:11434",
            "api_key": "",
            "model": "main",
            "temperature": 0.8,
            "max_tokens": 1024,
            "top_p": 1.0,
            "frequency_penalty": 0.0,
            "presence_penalty": 0.0,
            "repetition_penalty": 1.0
        }))
        .unwrap();
        assert_eq!(old.num_gpu, 15);
        assert!(old.summary_model.is_empty());

        let current: ConnectionSettings = serde_json::from_value(json!({
            "summary_model": "extract",
            "num_gpu": 24,
            "provider": "ollama",
            "base_url": "http://127.0.0.1:11434",
            "api_key": "",
            "model": "main",
            "temperature": 0.8,
            "max_tokens": 1024,
            "top_p": 1.0,
            "frequency_penalty": 0.0,
            "presence_penalty": 0.0,
            "repetition_penalty": 1.0
        }))
        .unwrap();
        assert_eq!(current.summary_model, "extract");
        assert_eq!(current.num_gpu, 24);
    }

    #[test]
    fn chat_request_accepts_legacy_and_structured_context_payloads() {
        let legacy: ChatRequest = serde_json::from_value(json!({
            "card": "card-id",
            "message": "Hello"
        }))
        .unwrap();
        assert!(legacy.persona.is_none());
        assert!(legacy.world_info.is_empty());

        let current: ChatRequest = serde_json::from_value(json!({
            "card": "card-id",
            "message": "Hello",
            "persona": {"name": "Traveler", "description": "A curious traveler."},
            "world_info": ["The old bridge is guarded."],
            "lore_entries": [{
                "id": "bridge",
                "keys": "bridge, old road",
                "content": "The bridge is guarded.",
                "enabled": true
            }]
        }))
        .unwrap();
        assert_eq!(current.persona.unwrap().name, "Traveler");
        assert_eq!(current.world_info, ["The old bridge is guarded."]);
        assert_eq!(current.lore_entries[0].id, "bridge");
        assert_eq!(current.lore_entries[0].keys, "bridge, old road");
    }

    #[test]
    fn card_and_worldbook_content_become_separate_index_entries() {
        let card = zhuo_card::Card::from_json(json!({
            "name": "苏菲",
            "description": "角色背景".repeat(120),
            "personality": "性格设定",
            "character_book": {
                "name": "卡内世界书",
                "entries": [{
                    "id": "tower",
                    "keys": ["法师塔"],
                    "content": "法师塔位于北方。",
                    "enabled": true
                }]
            }
        }))
        .unwrap();
        let worldbook: zhuo_card::WorldBook = serde_json::from_value(json!({
            "name": "全局世界书",
            "entries": [{
                "id": "market",
                "keys": ["集市"],
                "content": "集市每天开放。",
                "enabled": true
            }]
        }))
        .unwrap();
        let entries = card_lore_entries(
            &card,
            "初始世界背景",
            Some(&worldbook),
            &[ChatLoreEntry {
                id: "local".into(),
                keys: "桥梁".into(),
                content: "桥梁设定".into(),
                enabled: true,
            }],
            &[],
            None,
        );
        assert!(entries
            .iter()
            .any(|entry| entry.source_key == "card:description"));
        assert!(entries
            .iter()
            .any(|entry| entry.source_key == "card-book:0:tower"));
        assert!(entries
            .iter()
            .any(|entry| entry.source_key == "imported-book:0:market"));
        assert!(entries
            .iter()
            .any(|entry| entry.source_key == "client-lore:0:local"));
        assert!(entries.iter().any(|entry| entry.content == "初始世界背景"));
    }

    #[test]
    fn first_token_timeout_has_an_actionable_sse_error_message() {
        let error = anyhow::anyhow!(zhuo_llm::FIRST_TOKEN_TIMEOUT_ERROR);
        assert_eq!(
            generation_error_message(&error),
            "生成超时，请检查上下文或降低显存层数"
        );
    }

    #[test]
    fn st_output_regex_honors_flags_placement_and_capture_groups() {
        let scripts = vec![RegexScript {
            script_name: "clean".into(),
            find_regex: "/foo (bar)/gi".into(),
            replace_string: "[$1]".into(),
            placement: vec![2],
            disabled: false,
            prompt_only: false,
        }];
        assert_eq!(
            apply_output_regex("Foo bar, foo BAR", &scripts),
            "[bar], [BAR]"
        );
        let ignored = RegexScript {
            placement: vec![1],
            ..scripts[0].clone()
        };
        assert_eq!(apply_output_regex("foo bar", &[ignored]), "foo bar");
    }
}
