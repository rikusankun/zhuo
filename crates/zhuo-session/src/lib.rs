//! 会话历史落盘：CLI 与 Web 共用。
//! 每个角色一个 JSON 文件，只存对话轮次；system 每次由卡重建。
//! 同时支持 SillyTavern 兼容的 JSONL 格式导入/导出。

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use zhuo_llm::Msg;

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct Session {
    pub card_name: String,
    pub messages: Vec<Msg>,
}

/// 会话目录：优先 ZHUO_SESSIONS_DIR，否则 ~/.zhuo/sessions
pub fn sessions_dir() -> PathBuf {
    if let Ok(d) = std::env::var("ZHUO_SESSIONS_DIR") {
        return PathBuf::from(d);
    }
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".zhuo").join("sessions")
}

/// 角色名转安全文件名（保留中文/字母数字，其余替换为 _）
pub fn safe_name(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if s.is_empty() {
        "unnamed".into()
    } else {
        s
    }
}

/// SillyTavern JSONL 格式的单条记录
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JsonlMessage {
    pub role: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub char: Option<String>,
}

impl JsonlMessage {
    /// 转换为内部 Msg 格式
    pub fn to_msg(&self) -> Msg {
        Msg::new(&self.role, &self.content)
    }

    /// 从内部 Msg 创建（role 为 user/assistant/system）
    pub fn from_msg(msg: &Msg, name: Option<&str>) -> Self {
        Self {
            role: msg.role.clone(),
            content: msg.content.clone(),
            name: name.map(|s| s.to_string()),
            user: None,
            char: None,
        }
    }
}

impl Session {
    pub fn path(card_name: &str, dir: &Path) -> PathBuf {
        dir.join(format!("{}.json", safe_name(card_name)))
    }

    /// 读已有历史；文件不存在或损坏时返回空会话（不阻塞启动）
    pub fn load_or_new(card_name: &str, dir: &Path) -> Result<Self> {
        let path = Self::path(card_name, dir);
        match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .with_context(|| format!("parse session: {}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self {
                card_name: card_name.into(),
                messages: Vec::new(),
            }),
            Err(e) => Err(e).with_context(|| format!("read session: {}", path.display())),
        }
    }

    pub fn save(&self, dir: &Path) -> Result<()> {
        std::fs::create_dir_all(dir)?;
        let path = Self::path(&self.card_name, dir);
        std::fs::write(&path, serde_json::to_vec_pretty(self)?)
            .with_context(|| format!("write session: {}", path.display()))?;
        Ok(())
    }

    /// 从 JSONL 文件加载会话（SillyTavern 导出格式兼容）
    /// 每行一个 JSON 对象：{"role":"user","content":"...","name":"..."}
    pub fn load_from_jsonl(card_name: &str, path: &Path) -> Result<Self> {
        let file = File::open(path).with_context(|| format!("open jsonl: {}", path.display()))?;
        let reader = BufReader::new(file);
        let mut messages = Vec::new();
        for line in reader.lines() {
            let line = line?;
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let jsonl: JsonlMessage = serde_json::from_str(line)
                .with_context(|| format!("parse jsonl line: {}", line))?;
            messages.push(jsonl.to_msg());
        }
        Ok(Self {
            card_name: card_name.into(),
            messages,
        })
    }

    /// 保存为 JSONL 格式（SillyTavern 兼容）
    /// 每行一个 JSON 对象，便于流式处理和版本控制
    pub fn save_as_jsonl(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file =
            File::create(path).with_context(|| format!("create jsonl: {}", path.display()))?;
        let mut writer = BufWriter::new(file);
        for msg in &self.messages {
            let jsonl = JsonlMessage::from_msg(msg, Some(&self.card_name));
            let line = serde_json::to_string(&jsonl)?;
            writer.write_all(line.as_bytes())?;
            writer.write_all(b"\n")?;
        }
        writer.flush()?;
        Ok(())
    }

    /// 追加消息到 JSONL 文件（增量写入，不重读全文件）
    pub fn append_to_jsonl(path: &Path, messages: &[Msg], card_name: &str) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .with_context(|| format!("open jsonl for append: {}", path.display()))?;
        let mut writer = BufWriter::new(file);
        for msg in messages {
            let jsonl = JsonlMessage::from_msg(msg, Some(card_name));
            let line = serde_json::to_string(&jsonl)?;
            writer.write_all(line.as_bytes())?;
            writer.write_all(b"\n")?;
        }
        writer.flush()?;
        Ok(())
    }

    /// 清空会话时同步删盘上文件，避免重启后"幽灵恢复"
    pub fn delete(card_name: &str, dir: &Path) -> Result<()> {
        let path = Self::path(card_name, dir);
        match std::fs::remove_file(&path) {
            Ok(_) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e).with_context(|| format!("delete session: {}", path.display())),
        }
    }

    /// 启动时扫目录恢复全部会话；跳过损坏或非 Session 的 .json
    pub fn load_all(dir: &Path) -> Vec<Self> {
        let mut out = Vec::new();
        let Ok(rd) = std::fs::read_dir(dir) else {
            return out;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            if let Ok(bytes) = std::fs::read(&p) {
                if let Ok(s) = serde_json::from_slice::<Session>(&bytes) {
                    if !s.card_name.is_empty() {
                        out.push(s);
                    }
                }
            }
        }
        out.sort_by(|a, b| a.card_name.cmp(&b.card_name));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "zhuo_shared_session_{}_{}",
            name,
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&p);
        p
    }

    #[test]
    fn save_then_load_roundtrip() {
        let dir = tmp("roundtrip");
        let mut s = Session::load_or_new("苏菲/测试", &dir).unwrap();
        assert!(s.messages.is_empty());
        s.messages.push(Msg::user("你好"));
        s.messages.push(Msg::assistant("要来串冰糖葫芦吗？"));
        s.save(&dir).unwrap();

        let loaded = Session::load_or_new("苏菲/测试", &dir).unwrap();
        assert_eq!(loaded, s);
        assert_eq!(loaded.messages.len(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_file_gives_empty_session() {
        let dir = tmp("missing");
        let s = Session::load_or_new("nobody", &dir).unwrap();
        assert_eq!(s.card_name, "nobody");
        assert!(s.messages.is_empty());
    }

    #[test]
    fn safe_name_replaces_path_separators() {
        assert_eq!(safe_name("苏菲/测试"), "苏菲_测试");
        assert_eq!(safe_name("a:b\\c"), "a_b_c");
        assert_eq!(safe_name("///"), "___");
    }

    #[test]
    fn load_all_skips_corrupt_and_non_session_files() {
        let dir = tmp("load_all");
        std::fs::create_dir_all(&dir).unwrap();
        Session {
            card_name: "苏菲".into(),
            messages: vec![Msg::user("一")],
        }
        .save(&dir)
        .unwrap();
        Session {
            card_name: "冬堡".into(),
            messages: vec![Msg::user("二")],
        }
        .save(&dir)
        .unwrap();
        std::fs::write(dir.join("broken.json"), b"not json").unwrap();
        std::fs::write(dir.join("x.tokens.jsonl"), b"{}\n").unwrap();

        let all = Session::load_all(&dir);
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].card_name, "冬堡");
        assert_eq!(all[1].card_name, "苏菲");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn delete_removes_file() {
        let dir = tmp("delete");
        let s = Session {
            card_name: "苏菲".into(),
            messages: vec![Msg::user("一")],
        };
        s.save(&dir).unwrap();
        assert!(Session::path("苏菲", &dir).exists());
        Session::delete("苏菲", &dir).unwrap();
        assert!(!Session::path("苏菲", &dir).exists());
        Session::delete("苏菲", &dir).unwrap(); // 幂等
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn jsonl_sillytavern_roundtrip() {
        let dir = tmp("jsonl_roundtrip");

        // 1. 模拟 SillyTavern 导出的 JSONL 内容
        let jsonl_content = r#"{"role":"user","content":"苏菲，你店里那只总在书架上打盹的猫叫什么名字呀？","name":"User"}
{"role":"assistant","content":"它叫雪球，是三年前我在集市门口捡到的小家伙。别看它懒，它只肯吃蜂蜜，别的什么都不碰。","name":"苏菲"}
{"role":"user","content":"蜂蜜？冬堡哪儿能弄到正宗的蜂蜜？","name":"User"}
{"role":"assistant","content":"集市上老约翰的铺子有，用雪季前的野花酿的。雪球每天傍晚都要凑过去闻半天。","name":"苏菲"}
"#;
        let jsonl_path = dir.join("chat.jsonl");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&jsonl_path, jsonl_content).unwrap();

        // 2. 从 JSONL 加载
        let mut session = Session::load_from_jsonl("苏菲", &jsonl_path).unwrap();
        assert_eq!(session.card_name, "苏菲");
        assert_eq!(session.messages.len(), 4);
        assert_eq!(session.messages[0].role, "user");
        assert_eq!(
            session.messages[0].content,
            "苏菲，你店里那只总在书架上打盹的猫叫什么名字呀？"
        );
        assert_eq!(session.messages[1].role, "assistant");
        assert!(session.messages[1].content.contains("雪球"));

        // 3. 追加新消息
        session.messages.push(Msg::user("冬堡的雪祭典是什么时候？"));
        session.messages.push(Msg::assistant(
            "每年入冬第一场雪落下的那天，全城会在中央广场点起篝火。",
        ));

        // 4. 保存回 JSONL
        let output_path = dir.join("chat_output.jsonl");
        session.save_as_jsonl(&output_path).unwrap();

        // 5. 验证保存的 JSONL 可读且内容正确
        let reloaded = Session::load_from_jsonl("苏菲", &output_path).unwrap();
        assert_eq!(reloaded.messages.len(), 6);
        assert_eq!(reloaded.messages[4].content, "冬堡的雪祭典是什么时候？");
        assert_eq!(
            reloaded.messages[5].content,
            "每年入冬第一场雪落下的那天，全城会在中央广场点起篝火。"
        );

        // 6. 验证 JSONL 格式：每行都是有效 JSON
        let saved_content = std::fs::read_to_string(&output_path).unwrap();
        let lines: Vec<&str> = saved_content.trim().split('\n').collect();
        assert_eq!(lines.len(), 6);
        for line in lines {
            let parsed: JsonlMessage = serde_json::from_str(line).unwrap();
            assert!(!parsed.role.is_empty());
            assert!(!parsed.content.is_empty());
        }

        // 7. 测试 append_to_jsonl
        let append_path = dir.join("chat_append.jsonl");
        Session::append_to_jsonl(&append_path, &[Msg::user("追加测试")], "苏菲").unwrap();
        Session::append_to_jsonl(&append_path, &[Msg::assistant("追加回复")], "苏菲").unwrap();
        let appended = Session::load_from_jsonl("苏菲", &append_path).unwrap();
        assert_eq!(appended.messages.len(), 2);
        assert_eq!(appended.messages[0].content, "追加测试");
        assert_eq!(appended.messages[1].content, "追加回复");

        println!("✅ SillyTavern JSONL 完整读写测试通过");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn jsonl_handles_sillytavern_user_char_fields() {
        // SillyTavern 有时会用 user/char 字段代替 name
        let dir = tmp("jsonl_user_char");
        std::fs::create_dir_all(&dir).unwrap();
        let jsonl_content = r#"{"role":"user","content":"你好","user":"用户","char":"苏菲"}
{"role":"assistant","content":"你好！","user":"用户","char":"苏菲"}
"#;
        let jsonl_path = dir.join("chat.jsonl");
        std::fs::write(&jsonl_path, jsonl_content).unwrap();

        let session = Session::load_from_jsonl("苏菲", &jsonl_path).unwrap();
        assert_eq!(session.messages.len(), 2);
        assert_eq!(session.messages[0].content, "你好");
        assert_eq!(session.messages[1].content, "你好！");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
