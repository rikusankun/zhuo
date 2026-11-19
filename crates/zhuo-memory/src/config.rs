//! 记忆系统常量：数据库路径、建表 SQL、迁移语句、停用词表。

/// 默认数据库路径：蓝图约定的 zhuo.db（可用 ZHUO_DB 覆盖）
pub const DEFAULT_DB: &str = "zhuo.db";

/// 建表 SQL：Message、Memory、Lore、角色状态和会话表及其 FTS5 虚拟表。
pub const SCHEMA_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS messages (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    card_name  TEXT    NOT NULL,
    role       TEXT    NOT NULL CHECK (role IN ('user','assistant','system')),
    content    TEXT    NOT NULL,
    processed  INTEGER NOT NULL DEFAULT 0,
    created_at TEXT    NOT NULL DEFAULT (datetime('now','localtime'))
);
CREATE INDEX IF NOT EXISTS idx_messages_card ON messages(card_name, id);

CREATE TABLE IF NOT EXISTS memories (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    card_name    TEXT    NOT NULL,
    summary      TEXT    NOT NULL,
    keywords     TEXT    NOT NULL DEFAULT '',
    source_round INTEGER NOT NULL DEFAULT 0,
    source_message_id INTEGER,
    created_at   TEXT    NOT NULL DEFAULT (datetime('now','localtime'))
);
CREATE INDEX IF NOT EXISTS idx_memories_card ON memories(card_name, id);

CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts USING fts5(
    mem_id    UNINDEXED,
    card_name UNINDEXED,
    body,
    tokenize = 'unicode61'
);

CREATE TABLE IF NOT EXISTS lore_entries (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    card_name  TEXT    NOT NULL,
    source_key TEXT    NOT NULL,
    title      TEXT    NOT NULL,
    triggers   TEXT    NOT NULL DEFAULT '',
    secondary_triggers TEXT    NOT NULL DEFAULT '',
    content    TEXT    NOT NULL,
    constant   INTEGER NOT NULL DEFAULT 0,
    enabled    INTEGER NOT NULL DEFAULT 1,
    selective  INTEGER NOT NULL DEFAULT 0,
    case_sensitive INTEGER NOT NULL DEFAULT 0,
    match_whole_words INTEGER NOT NULL DEFAULT 1,
    order_val  INTEGER NOT NULL DEFAULT 0,
    scan_depth INTEGER,
    UNIQUE(card_name, source_key)
);
CREATE INDEX IF NOT EXISTS idx_lore_entries_card ON lore_entries(card_name);

CREATE VIRTUAL TABLE IF NOT EXISTS lore_entries_fts USING fts5(
    lore_id   UNINDEXED,
    card_name UNINDEXED,
    body,
    tokenize = 'unicode61'
);

CREATE TABLE IF NOT EXISTS character_state (
    card_name  TEXT PRIMARY KEY,
    state_json TEXT NOT NULL DEFAULT '{}',
    updated_at TEXT NOT NULL DEFAULT (datetime('now','localtime'))
);

CREATE TABLE IF NOT EXISTS chats (
    id          TEXT PRIMARY KEY,
    card_name   TEXT NOT NULL,
    title       TEXT    NOT NULL,
    created_at  TEXT    NOT NULL DEFAULT (datetime('now','localtime')),
    updated_at  TEXT    NOT NULL DEFAULT (datetime('now','localtime'))
);
CREATE INDEX IF NOT EXISTS idx_chats_card ON chats(card_name, updated_at DESC);

CREATE TABLE IF NOT EXISTS app_settings (
    key        TEXT PRIMARY KEY,
    value      TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (datetime('now','localtime'))
);
"#;

/// 旧库迁移：给已存在的 messages 表补 processed 列（幂等）。
pub const MIGRATIONS: &[&str] = &[
    "ALTER TABLE messages ADD COLUMN processed INTEGER NOT NULL DEFAULT 0",
    "ALTER TABLE messages ADD COLUMN chat_id TEXT NOT NULL DEFAULT ''",
    "ALTER TABLE memories ADD COLUMN source_message_id INTEGER",
    "ALTER TABLE lore_entries ADD COLUMN enabled INTEGER NOT NULL DEFAULT 1",
    "ALTER TABLE lore_entries ADD COLUMN selective INTEGER NOT NULL DEFAULT 0",
    "ALTER TABLE lore_entries ADD COLUMN case_sensitive INTEGER NOT NULL DEFAULT 0",
    "ALTER TABLE lore_entries ADD COLUMN match_whole_words INTEGER NOT NULL DEFAULT 1",
    "ALTER TABLE lore_entries ADD COLUMN order_val INTEGER NOT NULL DEFAULT 0",
    "ALTER TABLE lore_entries ADD COLUMN scan_depth INTEGER",
    "ALTER TABLE lore_entries ADD COLUMN secondary_triggers TEXT NOT NULL DEFAULT ''",
];

pub const INDEX_MIGRATIONS: &str = r#"
CREATE INDEX IF NOT EXISTS idx_messages_chat_id ON messages(chat_id, id);
CREATE INDEX IF NOT EXISTS idx_messages_unprocessed ON messages(card_name, processed, id);
CREATE UNIQUE INDEX IF NOT EXISTS idx_memories_card_source_message
    ON memories(card_name, source_message_id);
"#;

/// 中文/无意义虚词停用表：分词后过滤，避免 FTS5 查询被噪音 token 稀释。
pub const STOPWORDS: &[&str] = &[
    "的", "了", "吗", "呢", "吧", "啊", "呀", "哦", "嗯", "是",
    "在", "和", "与", "就", "都", "也", "很", "要", "有", "不",
    "这", "那", "我", "你", "他", "她", "它",
    "什么", "怎么", "怎么样", "如何", "请问", "一下",
    "些", "个",
];
