//! 低配轨记忆系统：SQLite + FTS5 + jieba-rs，零显存占用。
//!
//! 设计（对应《酌》架构蓝图 3.1）：
//! - **Message 表**：对话历史持久化（替代旧 JSON 落盘，单一事实来源）。
//! - **Memory / Lore 表 + FTS5 虚拟表**：分别保存长期记忆和角色/世界设定条目。
//!   写入前用 jieba 分词，检索时同样分词后拼 FTS5 MATCH 语法并按 `bm25()` 排序。
//! - 检索结果作为**独立文本块**插入 System Prompt 与聊天历史之间，生成后即丢弃，
//!   不污染永久聊天记录。
//!
//! 全部在进程内 SQLite 完成，纯 CPU，毫秒级检索数万条记忆。

pub mod config;
pub mod store;
pub mod types;

pub use config::{DEFAULT_DB, INDEX_MIGRATIONS, MIGRATIONS, SCHEMA_SQL, STOPWORDS};
pub use store::MemoryStore;
pub use types::{ChatRecord, LoreEntry, MemoryHit, PromptHit};
