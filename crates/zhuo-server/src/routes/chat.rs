use axum::extract::{Path, Query, State};
use axum::response::sse::Sse;
use axum::Json;
use std::collections::HashMap;
use tokio::sync::mpsc::unbounded_channel;

use crate::config::{api_base_url, connection_settings, uses_native_ollama, RegexScript};
use crate::providers::{provider_for, LlmEvent};
use crate::services::{lore, memory_service};
use crate::services::lore::card_lore_entries;
use crate::prompt::{
    assemble_rag_messages, estimate_prompt_tokens, minimal_system_prompt, prompt_chars,
    retrieval_block, render_state, MAX_PROMPT_CHARS,
};
use crate::routes::{
    ChatRequest, CreateChatRequest, DEFAULT_USER_NAME, RenameChatRequest,
    generation_error_message, prompt_payload, sse_error, unfold_rx,
};
use crate::routes::SseStream;
use crate::state::ServerState;
use zhuo_llm::{StreamConfig, StreamOutcome};
use serde_json::json;

pub async fn chat(State(st): State<ServerState>, Json(req): Json<ChatRequest>) -> Sse<SseStream> {
    if req.message.trim().is_empty() {
        return sse_error("消息不能为空".into());
    }
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
                .unwrap_or_else(|| DEFAULT_USER_NAME.to_string()),
        )
    };

    let (tx, rx) = unbounded_channel::<String>();
    let prompt_size = prompt_chars(&msgs);
    let estimated_tokens = estimate_prompt_tokens(&msgs);
    let _ = tx.send(
        prompt_payload(&prompt_hits, prompt_size, estimated_tokens, history_count).to_string(),
    );

    let connection = connection_settings(&st);
    let connection_clone = connection.clone();
    let native = uses_native_ollama(&connection, st.ollama_native);
    let base = api_base_url(&connection, native);
    let key = connection_clone.api_key;
    let model = req.model.as_deref().unwrap_or(&connection_clone.model).to_string();
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
        let stop_strings: Vec<String> = [
            format!("\n{}:", user_name),
            format!("\n{}：", user_name),
            format!("\n{}:", char_name),
            format!("\n{}：", char_name),
        ]
        .into_iter()
        .collect();
        let is_ollama = connection_clone.provider == "ollama";
        let provider = provider_for(&connection_clone.provider);

        let mut partial = String::new();
        let result = if is_ollama {
            let cfg = StreamConfig {
                base_url: base.as_str(),
                api_key: key.as_str(),
                model: model.as_str(),
                include_usage: st2.include_usage,
                dump_path: None,
                response_format: None,
                temperature: Some(connection_clone.temperature),
                max_tokens: Some(connection_clone.max_tokens),
                top_p: Some(connection_clone.top_p),
                frequency_penalty: Some(connection_clone.frequency_penalty),
                presence_penalty: Some(connection_clone.presence_penalty),
                repetition_penalty: Some(connection_clone.repetition_penalty),
                num_gpu: (connection_clone.provider == "ollama").then_some(connection_clone.num_gpu).flatten(),
                ollama_native: native,
                stop: Some(&stop_strings),
            };
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
        } else {
            let url = provider.chat_url(&connection, &model);
            let headers = provider.build_headers(&key);
            let body = provider.build_request_body(
                msgs.clone(),
                &model,
                connection_clone.temperature,
                connection_clone.max_tokens,
                connection_clone.top_p,
                Some(stop_strings.clone()),
            );
            let tx2 = tx.clone();
            let stream_result = provider.stream_chat(url, headers, body, Box::new(move |event| {
                match event {
                    LlmEvent::Token(tok) => {
                        let _ = tx2.send(json!({ "token": tok }).to_string());
                    }
                    LlmEvent::Error(err) => {
                        let _ = tx2.send(json!({ "error": err }).to_string());
                    }
                    LlmEvent::Usage(Some(n)) => {
                        let _ = tx2.send(json!({ "usage": { "total_tokens": n } }).to_string());
                    }
                    _ => {}
                }
            }));
            match tokio::time::timeout(
                std::time::Duration::from_secs(300),
                stream_result,
            ).await {
                Ok(Ok(full_text)) => {
                    partial = full_text.clone();
                    Some(Ok(zhuo_llm::StreamOutcome {
                        content: full_text,
                        aborted: false,
                        ..Default::default()
                    }))
                }
                Ok(Err(e)) => Some(Err(anyhow::anyhow!(e))),
                Err(_) => Some(Err(anyhow::anyhow!("provider request timed out"))),
            }
        };
        let result = match result {
            None => Ok(StreamOutcome {
                content: partial.clone(),
                aborted: true,
                ..Default::default()
            }),
            Some(Err(e)) if !partial.is_empty() => {
                let _ = tx.send(json!({"error": generation_error_message(&e)}).to_string());
                Ok(StreamOutcome {
                    content: partial.clone(),
                    aborted: true,
                    ..Default::default()
                })
            }
            Some(result) => result,
        };

        match result {
            Ok(StreamOutcome { content, usage, .. }) if !content.is_empty() => {
                let mut processed = lore::apply_output_regex(&content, &regex_scripts);
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
                    first = first.replace(token, DEFAULT_USER_NAME);
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
