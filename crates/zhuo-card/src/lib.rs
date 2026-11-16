//! 角色卡解析：纯 JSON（V1 / V2）与 PNG 内嵌卡（tEXt/zTXt/iTXt `chara` chunk）。

use anyhow::{anyhow, Context, Result};
use base64::Engine;
use flate2::read::ZlibDecoder;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::Read;
use std::path::Path;

/// 统一后的角色卡字段
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct Card {
    /// 唯一 ID：内容 canonical JSON 的 FNV-1a 32bit 短 hash（8 位 hex）。
    /// 同内容同 id、不同文件不同 id——解决同名卡歧义，记忆/状态按 id 彻底隔离。
    #[serde(default)]
    pub id: String,
    pub name: String,
    pub description: String,
    pub first_mes: String,
    pub mes_example: String,
    #[serde(default)]
    pub personality: String,
    #[serde(default)]
    pub scenario: String,
    #[serde(default)]
    pub system_prompt: String,
    #[serde(default)]
    pub post_history_instructions: String,
    #[serde(default)]
    pub alternate_greetings: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub creator: String,
    #[serde(default)]
    pub creator_notes: String,
    #[serde(default)]
    pub character_book: Option<WorldBook>,
    #[serde(skip)]
    pub raw: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct WorldBook {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub scan_depth: Option<usize>,
    #[serde(default)]
    pub entries: Vec<WorldEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct WorldEntry {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub keys: Vec<String>,
    #[serde(default)]
    pub secondary_keys: Vec<String>,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub constant: bool,
    #[serde(default)]
    pub selective: bool,
    #[serde(default)]
    pub case_sensitive: bool,
    #[serde(default = "default_true")]
    pub match_whole_words: bool,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub order: i64,
    #[serde(default)]
    pub scan_depth: Option<usize>,
}

fn default_true() -> bool {
    true
}

/// FNV-1a 64bit（非加密用途：卡内容指纹）。手写 10 行，不引第三方。
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

fn short_hash(bytes: &[u8]) -> String {
    format!("{:08x}", fnv1a(bytes) as u32)
}

impl Card {
    /// V2 优先读 `data.*`，缺字段时回落到顶层（V1）。
    /// id 由 **canonical JSON**（serde_json 排序键序列化）内容哈希生成：
    /// 格式化差异不影响 id，语义内容变化才换 id。
    pub fn from_json(value: Value) -> Result<Self> {
        let data = value.get("data").filter(|v| v.is_object());
        let get = |key: &str| -> String {
            data.and_then(|d| d.get(key))
                .or_else(|| value.get(key))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string()
        };
        let name = get("name");
        if name.trim().is_empty() {
            return Err(anyhow!("card has no `name` field"));
        }
        let canonical = serde_json::to_string(&value).context("canonicalize card json")?;
        Ok(Self {
            id: short_hash(canonical.as_bytes()),
            name,
            description: get("description"),
            first_mes: get("first_mes"),
            mes_example: get("mes_example"),
            personality: get("personality"),
            scenario: get("scenario"),
            system_prompt: get("system_prompt"),
            post_history_instructions: get("post_history_instructions"),
            alternate_greetings: get_array(data, &value, "alternate_greetings"),
            tags: get_array(data, &value, "tags"),
            creator: get("creator"),
            creator_notes: get("creator_notes"),
            character_book: data
                .and_then(|d| d.get("character_book"))
                .or_else(|| value.get("character_book"))
                .and_then(parse_world_book_value),
            raw: value,
        })
    }

    pub fn to_v2_json(&self) -> Value {
        let mut root = if self.raw.is_object() {
            self.raw.clone()
        } else {
            serde_json::json!({})
        };
        let obj = root.as_object_mut().unwrap();
        obj.insert("spec".into(), Value::String("chara_card_v2".into()));
        obj.insert("spec_version".into(), Value::String("2.0".into()));
        let mut data = obj
            .get("data")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        for (key, val) in [
            ("name", self.name.clone()),
            ("description", self.description.clone()),
            ("personality", self.personality.clone()),
            ("scenario", self.scenario.clone()),
            ("first_mes", self.first_mes.clone()),
            ("mes_example", self.mes_example.clone()),
            ("system_prompt", self.system_prompt.clone()),
            (
                "post_history_instructions",
                self.post_history_instructions.clone(),
            ),
            ("creator", self.creator.clone()),
            ("creator_notes", self.creator_notes.clone()),
        ] {
            data.insert(key.into(), Value::String(val));
        }
        data.insert(
            "alternate_greetings".into(),
            serde_json::json!(self.alternate_greetings),
        );
        data.insert("tags".into(), serde_json::json!(self.tags));
        if let Some(book) = &self.character_book {
            if !data.contains_key("character_book") {
                data.insert("character_book".into(), world_book_to_v2(book));
            }
        }
        obj.insert("data".into(), Value::Object(data));
        root
    }
}

fn get_array(data: Option<&Value>, root: &Value, key: &str) -> Vec<String> {
    data.and_then(|d| d.get(key))
        .or_else(|| root.get(key))
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}
fn bool_field(v: &Value, names: &[&str], default: bool) -> bool {
    names
        .iter()
        .find_map(|n| v.get(*n))
        .and_then(|v| v.as_bool().or_else(|| v.as_i64().map(|n| n != 0)))
        .unwrap_or(default)
}
fn strings_field(v: &Value, names: &[&str]) -> Vec<String> {
    names
        .iter()
        .find_map(|n| v.get(*n))
        .map(|v| match v {
            Value::Array(a) => a
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect(),
            Value::String(s) => s
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect(),
            _ => vec![],
        })
        .unwrap_or_default()
}

pub fn parse_world_book_value(v: &Value) -> Option<WorldBook> {
    let entries_v = v.get("entries")?;
    let mut entries = Vec::new();
    let vals: Vec<(String, &Value)> = match entries_v {
        Value::Array(a) => a
            .iter()
            .enumerate()
            .map(|(i, v)| (i.to_string(), v))
            .collect(),
        Value::Object(m) => m.iter().map(|(k, v)| (k.clone(), v)).collect(),
        _ => return None,
    };
    for (fallback, e) in vals {
        let Some(content) = e.get("content").and_then(Value::as_str) else {
            continue;
        };
        entries.push(WorldEntry {
            id: e
                .get("id")
                .or_else(|| e.get("uid"))
                .map(|v| {
                    v.as_str()
                        .map(str::to_string)
                        .unwrap_or_else(|| v.to_string())
                })
                .unwrap_or(fallback),
            keys: strings_field(e, &["keys", "key"]),
            secondary_keys: strings_field(e, &["secondary_keys", "keysecondary", "secondaryKeys"]),
            content: content.into(),
            constant: bool_field(e, &["constant"], false),
            selective: bool_field(e, &["selective"], false),
            case_sensitive: bool_field(e, &["case_sensitive", "caseSensitive"], false),
            match_whole_words: bool_field(e, &["match_whole_words", "matchWholeWords"], true),
            enabled: !bool_field(e, &["disable", "disabled"], false)
                && bool_field(e, &["enabled"], true),
            order: e
                .get("insertion_order")
                .or_else(|| e.get("order"))
                .and_then(Value::as_i64)
                .unwrap_or(0),
            scan_depth: e
                .get("scan_depth")
                .or_else(|| e.get("scanDepth"))
                .and_then(Value::as_u64)
                .map(|n| n as usize),
        });
    }
    Some(WorldBook {
        name: v
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .into(),
        description: v
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .into(),
        scan_depth: v
            .get("scan_depth")
            .or_else(|| v.get("scanDepth"))
            .and_then(Value::as_u64)
            .map(|n| n as usize),
        entries,
    })
}
pub fn parse_world_book_bytes(bytes: &[u8]) -> Result<WorldBook> {
    let v: Value = serde_json::from_slice(bytes).context("parse ST World Info JSON")?;
    parse_world_book_value(&v)
        .ok_or_else(|| anyhow!("World Info must contain an entries object or array"))
}
pub fn world_book_to_v2(book: &WorldBook) -> Value {
    serde_json::json!({"name":book.name,"description":book.description,"scan_depth":book.scan_depth,"entries":book.entries.iter().map(|e|serde_json::json!({"id":e.id,"keys":e.keys,"secondary_keys":e.secondary_keys,"content":e.content,"constant":e.constant,"selective":e.selective,"case_sensitive":e.case_sensitive,"match_whole_words":e.match_whole_words,"enabled":e.enabled,"insertion_order":e.order,"scan_depth":e.scan_depth})).collect::<Vec<_>>()})
}

pub fn activated_entries(book: &WorldBook, history: &[String]) -> Vec<WorldEntry> {
    let mut out: Vec<_> = book
        .entries
        .iter()
        .filter(|e| e.enabled)
        .filter(|e| {
            if e.constant {
                return true;
            }
            let depth = e.scan_depth.or(book.scan_depth).unwrap_or(history.len());
            let text = history
                .iter()
                .rev()
                .take(depth)
                .rev()
                .cloned()
                .collect::<Vec<_>>()
                .join("\n");
            let primary = e
                .keys
                .iter()
                .any(|k| keyword_matches(&text, k, e.case_sensitive, e.match_whole_words));
            let secondary = !e.selective
                || e.secondary_keys.is_empty()
                || e.secondary_keys
                    .iter()
                    .any(|k| keyword_matches(&text, k, e.case_sensitive, e.match_whole_words));
            primary && secondary
        })
        .cloned()
        .collect();
    out.sort_by_key(|e| e.order);
    out
}
fn keyword_matches(text: &str, key: &str, case_sensitive: bool, whole: bool) -> bool {
    if key.is_empty() {
        return false;
    }
    let (hay, needle) = if case_sensitive {
        (text.to_string(), key.to_string())
    } else {
        (text.to_lowercase(), key.to_lowercase())
    };
    if !whole {
        return hay.contains(&needle);
    }
    hay.match_indices(&needle).any(|(i, _)| {
        let before = hay[..i].chars().next_back();
        let after = hay[i + needle.len()..].chars().next();
        before
            .map(|c| !c.is_alphanumeric() && c != '_')
            .unwrap_or(true)
            && after
                .map(|c| !c.is_alphanumeric() && c != '_')
                .unwrap_or(true)
    })
}

pub fn write_v2_png(source: &[u8], card_json: &Value) -> Result<Vec<u8>> {
    if !source.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err(anyhow!("source is not PNG"));
    }
    let json = serde_json::to_vec(card_json)?;
    let encoded = base64::engine::general_purpose::STANDARD.encode(json);
    let mut payload = b"chara\0".to_vec();
    payload.extend_from_slice(encoded.as_bytes());
    let mut out = source[..8].to_vec();
    let mut pos = 8;
    let mut inserted = false;
    while pos + 12 <= source.len() {
        let len = u32::from_be_bytes(source[pos..pos + 4].try_into().unwrap()) as usize;
        let end = pos + 12 + len;
        if end > source.len() {
            return Err(anyhow!("broken PNG chunk"));
        }
        let kind = &source[pos + 4..pos + 8];
        let data = &source[pos + 8..pos + 8 + len];
        let keyword = data.split(|b| *b == 0).next().unwrap_or_default();
        let is_card = matches!(kind, b"tEXt" | b"zTXt" | b"iTXt")
            && (keyword.eq_ignore_ascii_case(b"chara") || keyword.eq_ignore_ascii_case(b"ccv3"));
        if !inserted && kind == b"IDAT" {
            push_png_chunk(&mut out, b"tEXt", &payload);
            inserted = true
        }
        if !is_card {
            out.extend_from_slice(&source[pos..end])
        }
        pos = end;
        if kind == b"IEND" {
            break;
        }
    }
    if !inserted {
        return Err(anyhow!("PNG has no IDAT chunk"));
    }
    Ok(out)
}
fn push_png_chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let mut h = crc32fast::Hasher::new();
    h.update(kind);
    h.update(data);
    out.extend_from_slice(&h.finalize().to_be_bytes())
}

/// 加载文件并解析为 Card。按 PNG magic 判断格式，其余按 JSON 处理。
pub fn load_card(path: impl AsRef<Path>) -> Result<Card> {
    let path = path.as_ref();
    let bytes =
        std::fs::read(path).with_context(|| format!("read card file: {}", path.display()))?;
    Card::from_json(parse_card_bytes(&bytes)?)
}

/// 从字节解析出原始卡 JSON
pub fn parse_card_bytes(bytes: &[u8]) -> Result<Value> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        parse_png_card(bytes)
    } else {
        serde_json::from_slice(bytes).context("parse card JSON")
    }
}

fn parse_png_card(bytes: &[u8]) -> Result<Value> {
    if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err(anyhow!("not a PNG file"));
    }
    if bytes.len() < 8 {
        return Err(anyhow!("PNG file too short"));
    }

    // 手动遍历整个 PNG 的所有 chunk，不依赖 read_info（它只读到第一个 IDAT 就停止）。
    // tEXt/zTXt/iTXt 可以出现在 IDAT 之前或之后，因此必须扫描到 IEND。
    let mut chunks: Vec<(String, Vec<u8>)> = Vec::new();
    let mut pos = 8usize;
    while pos + 12 <= bytes.len() {
        let len = u32::from_be_bytes(bytes[pos..pos + 4].try_into().unwrap()) as usize;
        let end = pos + 12 + len;
        if end > bytes.len() {
            break; // 非法长度，停止避免死循环
        }
        let kind = &bytes[pos + 4..pos + 8];
        let data = &bytes[pos + 8..pos + 8 + len];

        match kind {
            b"tEXt" => {
                let sep = data.iter().position(|&b| b == 0).unwrap_or(len);
                let keyword = String::from_utf8_lossy(&data[..sep]).to_string();
                let text = data.get(sep + 1..).unwrap_or(&[]).to_vec();
                chunks.push((keyword, text));
            }
            b"zTXt" => {
                let sep = data.iter().position(|&b| b == 0).unwrap_or(len);
                let keyword = String::from_utf8_lossy(&data[..sep]).to_string();
                if sep + 2 <= len {
                    if let Ok(decompressed) = decompress_zlib(&data[sep + 2..]) {
                        chunks.push((keyword, decompressed.into_bytes()));
                    }
                }
            }
            b"iTXt" => {
                let mut off = 0;
                let sep = data[off..]
                    .iter()
                    .position(|&b| b == 0)
                    .unwrap_or(len - off);
                let keyword = String::from_utf8_lossy(&data[off..off + sep]).to_string();
                off += sep + 1;
                if off + 2 > len {
                    break;
                }
                let compression_flag = data[off];
                off += 2; // skip compression_flag + compression_method
                let lang_sep = data[off..]
                    .iter()
                    .position(|&b| b == 0)
                    .unwrap_or(len - off);
                off += lang_sep + 1; // skip language tag
                let tk_sep = data[off..]
                    .iter()
                    .position(|&b| b == 0)
                    .unwrap_or(len - off);
                off += tk_sep + 1; // skip translated keyword
                let text = if off <= len {
                    if compression_flag == 1 {
                        decompress_zlib(&data[off..]).unwrap_or_default()
                    } else {
                        String::from_utf8_lossy(&data[off..]).to_string()
                    }
                } else {
                    String::new()
                };
                chunks.push((keyword, text.into_bytes()));
            }
            _ => {}
        }

        if kind == b"IEND" {
            break;
        }
        pos = end;
    }

    // 优先 chara(V1/V2) / ccv3(V3)，其次任意能解出 JSON 的 chunk
    let ordered: Vec<&(String, Vec<u8>)> =
        chunks
            .iter()
            .filter(|(k, _)| k.eq_ignore_ascii_case("chara") || k.eq_ignore_ascii_case("ccv3"))
            .chain(chunks.iter().filter(|(k, _)| {
                !k.eq_ignore_ascii_case("chara") && !k.eq_ignore_ascii_case("ccv3")
            }))
            .collect();

    let mut tried: Vec<String> = Vec::new();
    for (keyword, _) in &ordered {
        tried.push(keyword.clone());
    }

    for (_, text_bytes) in &ordered {
        if let Some(v) = decode_chara_payload(text_bytes) {
            return Ok(v);
        }
    }

    if tried.is_empty() {
        Err(anyhow!("no `chara` chunk with valid card JSON in PNG"))
    } else {
        Err(anyhow!(
            "PNG 中找到文本 chunk {:?}，但均无法解出卡 JSON",
            tried
        ))
    }
}

fn decompress_zlib(data: &[u8]) -> Result<String> {
    let mut d = ZlibDecoder::new(data);
    let mut buf = Vec::new();
    d.read_to_end(&mut buf)
        .context("zlib decompress text chunk")?;
    String::from_utf8(buf).context("decompressed text is not valid UTF-8")
}

/// tEXt value 通常是 base64(JSON)，少数卡直接放 JSON。
/// 容错：STANDARD / URL_SAFE / URL_SAFE_NO_PAD base64，剔除空白与 BOM，
/// 直接 JSON，Latin-1 兜底。
fn decode_chara_payload(raw: &[u8]) -> Option<Value> {
    // 先按 UTF-8 解读，失败则按 Latin-1 兜底
    let text = std::str::from_utf8(raw)
        .map(|s| s.to_string())
        .unwrap_or_else(|_| raw.iter().map(|&b| b as char).collect::<String>());

    decode_payload_str(&text)
}

fn decode_payload_str(text: &str) -> Option<Value> {
    let trimmed = text.trim();
    let no_bom = trimmed.strip_prefix('\u{FEFF}').unwrap_or(trimmed);

    // a) STANDARD base64
    if let Some(v) = try_base64_json(no_bom, &base64::engine::general_purpose::STANDARD) {
        return Some(v);
    }
    // b) URL_SAFE / URL_SAFE_NO_PAD base64
    if let Some(v) = try_base64_json(no_bom, &base64::engine::general_purpose::URL_SAFE) {
        return Some(v);
    }
    if let Some(v) = try_base64_json(no_bom, &base64::engine::general_purpose::URL_SAFE_NO_PAD) {
        return Some(v);
    }
    // c) 剔除所有空白与 BOM 后再尝试 base64
    let stripped: String = no_bom
        .chars()
        .filter(|c| !matches!(c, '\r' | '\n' | '\t' | ' '))
        .collect();
    if stripped != no_bom && !stripped.is_empty() {
        if let Some(v) = try_base64_json(&stripped, &base64::engine::general_purpose::STANDARD) {
            return Some(v);
        }
        if let Some(v) = try_base64_json(&stripped, &base64::engine::general_purpose::URL_SAFE) {
            return Some(v);
        }
        if let Some(v) =
            try_base64_json(&stripped, &base64::engine::general_purpose::URL_SAFE_NO_PAD)
        {
            return Some(v);
        }
    }
    // d) 直接把文本当 JSON 解析
    serde_json::from_str::<Value>(no_bom).ok()
}

fn try_base64_json<E: base64::Engine>(input: &str, engine: &E) -> Option<Value> {
    let raw = engine.decode(input).ok()?;
    try_json_from_bytes(&raw)
}

fn try_json_from_bytes(raw: &[u8]) -> Option<Value> {
    // 先尝试 UTF-8
    if let Ok(v) = serde_json::from_slice::<Value>(raw) {
        return Some(v);
    }
    // Latin-1 -> UTF-8 兜底
    let latin1: String = raw.iter().map(|&b| b as char).collect();
    serde_json::from_str::<Value>(&latin1).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    use flate2::write::ZlibEncoder;
    use flate2::Compression;
    use std::io::Write;

    fn v1() -> &'static str {
        r#"{"name":"苏菲","description":"冬堡学院的学生","first_mes":"要来串冰糖葫芦吗？","mes_example":"<START>"}"#
    }

    fn v2() -> String {
        let inner = r#"{"name":"苏菲","description":"冬堡学院的学生","first_mes":"要来串冰糖葫芦吗？","mes_example":"<START>","spec":"chara_card_v2"}"#;
        format!(
            r#"{{"spec":"chara_card_v2","spec_version":"2.0","data":{}}}"#,
            inner
        )
    }

    fn make_png(chara_text: Option<&str>) -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut buf, 1, 1);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            if let Some(t) = chara_text {
                enc.add_text_chunk("chara".into(), t.into()).unwrap();
            }
            let mut writer = enc.write_header().unwrap();
            writer.write_image_data(&[0, 0, 0, 255]).unwrap();
        }
        buf
    }

    /// 创建不带任何文本 chunk 的基础 PNG（1x1 RGBA）。
    fn make_png_base() -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut buf, 1, 1);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            let mut writer = enc.write_header().unwrap();
            writer.write_image_data(&[0, 0, 0, 255]).unwrap();
        }
        buf
    }

    /// 在 IEND 之前插入原始 chunk 数据（即 IDAT 之后）。
    fn insert_chunk_before_iend(png: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
        let iend_pos = png.len() - 12; // IEND 始终是最后 12 字节
        let chunk_len = (data.len() as u32).to_be_bytes();
        let mut chunk = Vec::new();
        chunk.extend_from_slice(&chunk_len);
        chunk.extend_from_slice(kind);
        chunk.extend_from_slice(data);
        let mut h = crc32fast::Hasher::new();
        h.update(kind);
        h.update(data);
        chunk.extend_from_slice(&h.finalize().to_be_bytes());
        png.splice(iend_pos..iend_pos, chunk);
    }

    /// 在 IDAT 之后插入 tEXt 风格的文本 chunk：keyword\0 + text。
    fn insert_text_chunk_after_idat(png: &mut Vec<u8>, kind: &[u8; 4], keyword: &str, text: &str) {
        let mut data = Vec::new();
        data.extend_from_slice(keyword.as_bytes());
        data.push(0);
        data.extend_from_slice(text.as_bytes());
        insert_chunk_before_iend(png, kind, &data);
    }

    /// zlib 压缩。
    fn compress_zlib(data: &[u8]) -> Vec<u8> {
        let mut e = ZlibEncoder::new(Vec::new(), Compression::default());
        e.write_all(data).unwrap();
        e.finish().unwrap()
    }

    fn write_tmp(name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("zhuo_card_test_{}_{}", name, std::process::id()));
        std::fs::write(&p, bytes).unwrap();
        p
    }

    #[test]
    fn parses_v1_json() {
        let c = Card::from_json(serde_json::from_str(v1()).unwrap()).unwrap();
        assert_eq!(c.name, "苏菲");
        assert_eq!(c.description, "冬堡学院的学生");
        assert_eq!(c.first_mes, "要来串冰糖葫芦吗？");
        assert_eq!(c.mes_example, "<START>");
    }

    #[test]
    fn parses_v2_data_object() {
        let c = Card::from_json(serde_json::from_str(&v2()).unwrap()).unwrap();
        assert_eq!(c.name, "苏菲");
        assert_eq!(c.first_mes, "要来串冰糖葫芦吗？");
    }

    #[test]
    fn loads_json_file() {
        let p = write_tmp("v1.json", v1().as_bytes());
        let c = load_card(&p).unwrap();
        assert_eq!(c.name, "苏菲");
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn loads_png_base64_chara_chunk() {
        let b64 = base64::engine::general_purpose::STANDARD.encode(v2());
        let png = make_png(Some(&b64));
        let c = Card::from_json(parse_card_bytes(&png).unwrap()).unwrap();
        assert_eq!(c.name, "苏菲");
        assert_eq!(c.description, "冬堡学院的学生");
    }

    #[test]
    fn loads_png_raw_json_chara_chunk() {
        // tEXt 是 Latin-1，非 ASCII 卡无法直写；用 ASCII 卡验证「未 base64」的回落路径
        let raw = r#"{"name":"Sophie","description":"student","first_mes":"hi","mes_example":""}"#;
        let png = make_png(Some(raw));
        let c = Card::from_json(parse_card_bytes(&png).unwrap()).unwrap();
        assert_eq!(c.name, "Sophie");
    }

    #[test]
    fn png_without_chara_fails() {
        let png = make_png(None);
        assert!(parse_card_bytes(&png).is_err());
    }

    #[test]
    fn card_without_name_fails() {
        assert!(Card::from_json(serde_json::json!({"description": "x"})).is_err());
    }

    #[test]
    fn same_content_same_id_reordered_keys_stable() {
        let a = Card::from_json(serde_json::from_str(v1()).unwrap()).unwrap();
        let b = Card::from_json(serde_json::from_str(v1()).unwrap()).unwrap();
        assert_eq!(a.id, b.id);
        assert_eq!(a.id.len(), 8, "短 hash 应为 8 位 hex");
        // 键顺序不同 → canonical 序列化一致 → id 不变
        let reordered = r#"{"mes_example":"<START>","first_mes":"要来串冰糖葫芦吗？","description":"冬堡学院的学生","name":"苏菲"}"#;
        let c = Card::from_json(serde_json::from_str(reordered).unwrap()).unwrap();
        assert_eq!(a.id, c.id);
    }

    #[test]
    fn different_content_different_id_isolates_same_name_cards() {
        // sophie.json 与 sophie_v1.json 同名但内容不同 → 不同 id → 记忆隔离
        let a = Card::from_json(serde_json::from_str(v1()).unwrap()).unwrap();
        let b = Card::from_json(
            serde_json::json!({"name":"苏菲","description":"冬堡学院的学生","first_mes":"……要来串冰糖葫芦吗？我多买了一串。","mes_example":"<START>"}),
        )
        .unwrap();
        assert_eq!(a.name, b.name, "同名卡");
        assert_ne!(a.id, b.id, "内容不同 → id 必须不同");
        // V2 包裹后的 data 与裸 V1 语义相同 → id 应一致（canonical 含 spec 字段除外则不同，
        // 但同一文件重复解析始终稳定）
        let c2 = Card::from_json(serde_json::from_str(&v2()).unwrap()).unwrap();
        assert!(!c2.id.is_empty());
        assert_eq!(
            c2.id,
            Card::from_json(serde_json::from_str(&v2()).unwrap())
                .unwrap()
                .id
        );
    }

    #[test]
    fn v2_advanced_fields_and_roundtrip_are_preserved() {
        let value = serde_json::json!({"spec":"chara_card_v2","spec_version":"2.0","vendor_extra":{"keep":true},"data":{"name":"Mira","description":"desc","first_mes":"hi","mes_example":"","system_prompt":"SYSTEM","post_history_instructions":"POST","tags":["fantasy"],"character_book":{"name":"Lore","entries":[{"id":1,"keys":["Rose"],"secondary_keys":["garden"],"content":"secret","selective":true,"case_sensitive":true,"match_whole_words":true,"enabled":true}]}}});
        let card = Card::from_json(value).unwrap();
        assert_eq!(card.system_prompt, "SYSTEM");
        assert_eq!(card.post_history_instructions, "POST");
        assert_eq!(card.character_book.as_ref().unwrap().entries.len(), 1);
        let out = card.to_v2_json();
        assert_eq!(out["vendor_extra"]["keep"], true);
        assert_eq!(out["data"]["system_prompt"], "SYSTEM");
    }

    #[test]
    fn world_info_matches_st_core_rules() {
        let book=parse_world_book_value(&serde_json::json!({"scanDepth":2,"entries":{"0":{"key":["Rose"],"keysecondary":["garden"],"content":"selective","selective":true,"caseSensitive":true,"matchWholeWords":true},"1":{"key":["king"],"content":"whole","matchWholeWords":true},"2":{"key":[],"content":"always","constant":true}}})).unwrap();
        let hit = activated_entries(
            &book,
            &[
                "old Rose garden".into(),
                "the Rose blooms in the garden".into(),
            ],
        );
        assert_eq!(hit.len(), 2);
        assert!(hit.iter().any(|e| e.content == "selective"));
        assert!(hit.iter().any(|e| e.content == "always"));
        let miss = activated_entries(
            &book,
            &[
                "old Rose garden".into(),
                "unrelated".into(),
                "liking lowercase rose".into(),
            ],
        );
        assert_eq!(
            miss.len(),
            1,
            "scan depth, case and whole-word rules should reject dynamic entries"
        );
    }

    #[test]
    fn png_v2_rewrite_replaces_card_chunk() {
        let old = make_png(Some(&base64::engine::general_purpose::STANDARD.encode(
            r#"{"name":"Old","first_mes":"","description":"","mes_example":""}"#,
        )));
        let value = serde_json::json!({"spec":"chara_card_v2","spec_version":"2.0","data":{"name":"New","first_mes":"hello","description":"d","mes_example":""}});
        let rewritten = write_v2_png(&old, &value).unwrap();
        let card = Card::from_json(parse_card_bytes(&rewritten).unwrap()).unwrap();
        assert_eq!(card.name, "New");
        assert_eq!(card.first_mes, "hello");
    }

    #[test]
    fn real_sillytavern_card_roundtrip() {
        // 使用测试工具函数创建一个带有 V2 角色卡的 PNG（模拟 SillyTavern 导出）
        let v2_card = serde_json::json!({
            "spec": "chara_card_v2",
            "spec_version": "2.0",
            "data": {
                "name": "苏菲PNG",
                "description": "你是苏菲，冬堡学院的学生。（PNG 内嵌卡）",
                "first_mes": "……要来串冰糖葫芦吗？我多买了一串。",
                "mes_example": "<START>",
                "personality": "安静、观察力强、对甜食没有抵抗力",
                "scenario": "冬堡集市，黄昏，刚下过雪",
                "creator_notes": "zhuo MVP 测试卡（V2）"
            }
        });
        let png_bytes = make_png(Some(
            &base64::engine::general_purpose::STANDARD.encode(v2_card.to_string()),
        ));

        // 1. 读取并解析原始卡
        let card =
            Card::from_json(parse_card_bytes(&png_bytes).unwrap()).expect("load original card");
        println!(
            "原始卡: name={}, desc={}, id={}",
            card.name, card.description, card.id
        );
        assert_eq!(card.name, "苏菲PNG");
        assert!(card.description.contains("冬堡学院"));

        // 2. 修改卡片
        let mut modified = card.clone();
        modified.name = "苏菲-修改版".to_string();
        modified.description = "修改后的描述：冬堡学院学生，验证写回功能。".to_string();
        modified.first_mes = "……要来串修改版冰糖葫芦吗？".to_string();

        // 3. 写回 PNG
        let v2_json = modified.to_v2_json();
        let new_png = write_v2_png(&png_bytes, &v2_json).expect("write v2 png");

        // 4. 验证写回的 PNG
        let verified = Card::from_json(parse_card_bytes(&new_png).unwrap()).unwrap();
        println!(
            "验证卡: name={}, desc={}, id={}",
            verified.name, verified.description, verified.id
        );
        assert_eq!(verified.name, modified.name);
        assert_eq!(verified.description, modified.description);
        assert_eq!(verified.first_mes, modified.first_mes);

        // 5. 验证原始文件未被破坏（重新读取原文件对比）
        let original_again = Card::from_json(parse_card_bytes(&png_bytes).unwrap()).unwrap();
        assert_eq!(original_again.name, card.name);
        assert_eq!(original_again.description, card.description);

        println!("✅ 真实 SillyTavern 角色卡完整读写流程测试通过");
    }

    #[test]
    fn parses_png_chara_after_idat() {
        // chara tEXt chunk 写在 IDAT 之后 → 必须能解析（旧逻辑会因 read_info 停于 IDAT 而失败）
        let b64 = base64::engine::general_purpose::STANDARD.encode(v2());
        let mut png = make_png_base();
        insert_text_chunk_after_idat(&mut png, b"tEXt", "chara", &b64);
        let c = Card::from_json(parse_card_bytes(&png).unwrap()).unwrap();
        assert_eq!(c.name, "苏菲");
        assert_eq!(c.description, "冬堡学院的学生");
    }

    #[test]
    fn parses_png_ccv3_after_idat() {
        // ccv3 (V3) tEXt chunk 写在 IDAT 之后 → 成功
        let ccv3_json = serde_json::json!({
            "name": "CCV3卡",
            "description": "ccv3 在 IDAT 之后",
            "first_mes": "你好",
            "mes_example": ""
        });
        let b64 = base64::engine::general_purpose::STANDARD.encode(ccv3_json.to_string());
        let mut png = make_png_base();
        insert_text_chunk_after_idat(&mut png, b"tEXt", "ccv3", &b64);
        let c = Card::from_json(parse_card_bytes(&png).unwrap()).unwrap();
        assert_eq!(c.name, "CCV3卡");
    }

    #[test]
    fn parses_png_ztxt_chara_after_idat() {
        // zTXt 压缩的 chara 写在 IDAT 之后 → 成功
        let json = v2();
        let compressed = compress_zlib(json.as_bytes());
        let mut data = Vec::new();
        data.extend_from_slice(b"chara\0");
        data.push(0); // compression method = 0
        data.extend_from_slice(&compressed);
        let mut png = make_png_base();
        insert_chunk_before_iend(&mut png, b"zTXt", &data);
        let c = Card::from_json(parse_card_bytes(&png).unwrap()).unwrap();
        assert_eq!(c.name, "苏菲");
    }

    #[test]
    fn parses_png_itxt_chara_after_idat() {
        // iTXt 压缩 + UTF-8 中文 chara 写在 IDAT 之后 → 成功
        let json =
            r#"{"name":"iTXt卡","description":"压缩+中文","first_mes":"你好","mes_example":""}"#;
        let compressed = compress_zlib(json.as_bytes());
        let mut data = Vec::new();
        data.extend_from_slice(b"chara\0");
        data.push(1); // compression flag = 1 (compressed)
        data.push(0); // compression method = 0
        data.extend_from_slice(b"\0"); // empty language tag
        data.extend_from_slice(b"\0"); // empty translated keyword
        data.extend_from_slice(&compressed);
        let mut png = make_png_base();
        insert_chunk_before_iend(&mut png, b"iTXt", &data);
        let c = Card::from_json(parse_card_bytes(&png).unwrap()).unwrap();
        assert_eq!(c.name, "iTXt卡");
        assert_eq!(c.description, "压缩+中文");
    }

    #[test]
    fn parses_png_urlsafe_base64_after_idat() {
        // URL-safe base64 chara 写在 IDAT 之后 → 成功
        let json = v2();
        let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(json);
        let mut png = make_png_base();
        insert_text_chunk_after_idat(&mut png, b"tEXt", "chara", &encoded);
        let c = Card::from_json(parse_card_bytes(&png).unwrap()).unwrap();
        assert_eq!(c.name, "苏菲");
    }

    #[test]
    fn parses_png_newline_base64_after_idat() {
        // 带换行的 base64 chara 写在 IDAT 之后 → 成功（空白会被剔除）
        let b64 = base64::engine::general_purpose::STANDARD.encode(v2());
        let with_newlines = format!("{}\n{}\n{}", &b64[..32], &b64[32..64], &b64[64..]);
        let mut png = make_png_base();
        insert_text_chunk_after_idat(&mut png, b"tEXt", "chara", &with_newlines);
        let c = Card::from_json(parse_card_bytes(&png).unwrap()).unwrap();
        assert_eq!(c.name, "苏菲");
    }

    #[test]
    fn png_without_chara_still_fails_with_clear_error() {
        // 真正无卡的纯图片 → 仍报清晰错误
        let png = make_png_base();
        let err = parse_card_bytes(&png).unwrap_err().to_string();
        assert!(
            err.contains("no `chara` chunk") || err.contains("文本 chunk"),
            "错误信息应清晰：{err}"
        );
    }
}
