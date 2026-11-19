//! 低配轨记忆系统：SQLite + FTS5 + jieba-rs，零显存占用。

pub mod config;
pub mod jieba;
pub mod lore_engine;
pub mod repositories;
pub mod store;
pub mod types;

pub use config::{DEFAULT_DB, INDEX_MIGRATIONS, MIGRATIONS, SCHEMA_SQL, STOPWORDS};
pub use jieba::{fts5_query, seg};
pub use lore_engine::activated_entries_internal;
pub use repositories::{
    chat_repo::ChatRepo, lore_repo::LoreRepo, memory_repo::MemoryRepo,
    message_repo::MessageRepo, settings_repo::SettingsRepo, state_repo::StateRepo,
};
pub use store::MemoryStore;
pub use types::{ChatRecord, LoreEntry, MemoryHit, PromptHit};
