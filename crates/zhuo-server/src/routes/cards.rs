use axum::extract::{Path, Query, State};
use axum::Json;
use base64::Engine;
use serde_json::json;

use crate::config::{connection_settings, RegexScript};
use crate::services::lore::card_lore_entries;
use crate::routes::{CreateCardRequest, ExportQuery, ImportCardRequest};
use crate::state::ServerState;

pub async fn cards(State(st): State<ServerState>) -> Json<serde_json::Value> {
    use crate::prompt::system_prompt;
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
    if let Some(n) = v.get("temperature").and_then(serde_json::Value::as_f64) {
        cfg.temperature = n as f32
    }
    if let Some(n) = v.get("top_p").and_then(serde_json::Value::as_f64) {
        cfg.top_p = n as f32
    }
    if let Some(n) = v.get("frequency_penalty").and_then(serde_json::Value::as_f64) {
        cfg.frequency_penalty = n as f32
    }
    if let Some(n) = v.get("presence_penalty").and_then(serde_json::Value::as_f64) {
        cfg.presence_penalty = n as f32
    }
    if let Some(n) = v
        .get("repetition_penalty")
        .or_else(|| v.get("rep_pen"))
        .and_then(serde_json::Value::as_f64)
    {
        cfg.repetition_penalty = n as f32
    }
    if let Some(n) = v
        .get("openai_max_tokens")
        .or_else(|| v.get("max_tokens"))
        .and_then(serde_json::Value::as_u64)
    {
        cfg.max_tokens = n.clamp(1, 32768) as u32
    }
    if let Some(m) = v
        .get("openai_model")
        .or_else(|| v.get("custom_model"))
        .and_then(serde_json::Value::as_str)
        .filter(|s| !s.is_empty())
    {
        cfg.model = m.into()
    }
    let result = super::connection::set_connection(State(st.clone()), Json(cfg)).await;
    if result.0.get("error").is_some() {
        return result;
    }
    let Json(mut response) = super::connection::get_connection(State(st)).await;
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
    let value: serde_json::Value = match serde_json::from_slice(&bytes) {
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
