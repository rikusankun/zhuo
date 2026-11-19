//! 记忆系统类型定义：MemoryHit、PromptHit、LoreEntry、ChatRecord。

use serde::Serialize;

/// 长期记忆检索命中结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryHit {
    pub id: i64,
    pub summary: String,
    pub keywords: String,
    pub source_round: i64,
    pub created_at: String,
}

/// Prompt 注入命中：记忆或设定条目。
#[derive(Debug, Clone, PartialEq)]
pub struct PromptHit {
    pub kind: &'static str,
    pub title: String,
    pub content: String,
    pub score: f64,
}

/// 世界书 / lore 条目：SillyTavern 兼容的触发规则字段。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoreEntry {
    pub source_key: String,
    pub title: String,
    pub triggers: String,
    pub secondary_triggers: String,
    pub content: String,
    pub constant: bool,
    pub enabled: bool,
    pub selective: bool,
    pub case_sensitive: bool,
    pub match_whole_words: bool,
    pub order: i64,
    pub scan_depth: Option<usize>,
}

/// 聊天会话记录。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ChatRecord {
    pub id: String,
    pub card: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
}
