//! MemoryStore：SQLite 连接池管理器与事务协调器。
//! 持有 Connection，对外委派给 repositories/ 下的各表仓储。

use anyhow::{Context, ensure, Result};
use rusqlite::Connection;
use std::path::Path;
use zhuo_llm::Msg;

use crate::config::{INDEX_MIGRATIONS, MIGRATIONS, SCHEMA_SQL};
use crate::repositories::{
    chat_repo::ChatRepo, memory_repo::MemoryRepo, message_repo::MessageRepo,
    lore_repo::LoreRepo, settings_repo::SettingsRepo, state_repo::StateRepo,
};
use crate::types::ChatRecord;

pub struct MemoryStore {
    pub(crate) conn: Connection,
}

impl MemoryStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("create db dir: {}", parent.display()))?;
            }
        }
        let conn = Connection::open(path)
            .with_context(|| format!("open sqlite: {}", path.display()))?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;")
            .context("configure sqlite concurrency")?;
        let me = Self { conn };
        me.init_schema()?;
        Ok(me)
    }

    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory().context("open in-memory sqlite")?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;")
            .context("configure in-memory sqlite")?;
        let me = Self { conn };
        me.init_schema()?;
        Ok(me)
    }

    pub fn init_schema(&self) -> Result<()> {
        self.conn
            .execute_batch(SCHEMA_SQL)
            .context("init zhuo schema (需 FTS5 支持)")?;
        for m in MIGRATIONS {
            let toks: Vec<&str> = m.split_whitespace().collect();
            if toks.len() < 6 || !toks[0].eq_ignore_ascii_case("ALTER") {
                continue;
            }
            let table = toks[2];
            let col = toks[5];
            let have: i64 = self.conn.query_row(
                &format!("SELECT COUNT(*) FROM pragma_table_info('{table}') WHERE name = ?1"),
                [col],
                |r| r.get(0),
            )?;
            if have == 0 {
                self.conn
                    .execute(m, [])
                    .with_context(|| format!("migration: {m}"))?;
            }
        }
        self.conn
            .execute_batch(INDEX_MIGRATIONS)
            .context("create message and memory indexes")?;
        self.conn.execute_batch(
            "INSERT OR IGNORE INTO chats (id, card_name, title, created_at, updated_at)
             SELECT 'legacy-' || card_name, card_name, '迁移的对话', MIN(created_at), MAX(created_at)
             FROM messages WHERE chat_id = '' GROUP BY card_name;
             UPDATE messages SET chat_id = 'legacy-' || card_name WHERE chat_id = '';",
        )?;
        Ok(())
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    // ---------- Message 表（委派给 MessageRepo） ----------

    pub fn add_message(&self, card_name: &str, role: &str, content: &str) -> Result<()> {
        MessageRepo::add_message(&self.conn, card_name, role, content)
    }

    pub fn history(&self, card_name: &str, limit: usize) -> Result<Vec<Msg>> {
        MessageRepo::history(&self.conn, card_name, limit)
    }

    pub fn clear_messages(&self, card_name: &str) -> Result<usize> {
        MessageRepo::clear_messages(&self.conn, card_name)
    }

    pub fn chat_history(&self, chat_id: &str, limit: usize) -> Result<Vec<Msg>> {
        MessageRepo::chat_history(&self.conn, chat_id, limit)
    }

    pub fn clear_chat_messages(&self, chat_id: &str) -> Result<usize> {
        MessageRepo::clear_chat_messages(&self.conn, chat_id)
    }

    pub fn count_unprocessed(&self, card_name: &str) -> Result<usize> {
        MessageRepo::count_unprocessed(&self.conn, card_name)
    }

    pub fn unprocessed(&self, card_name: &str, limit: usize) -> Result<Vec<(i64, Msg)>> {
        MessageRepo::unprocessed(&self.conn, card_name, limit)
    }

    pub fn mark_processed(&self, card_name: &str, upto_id: i64) -> Result<usize> {
        MessageRepo::mark_processed(&self.conn, card_name, upto_id)
    }

    pub fn last_chat_message_id(&self, chat_id: &str) -> Result<i64> {
        MessageRepo::last_chat_message_id(&self.conn, chat_id)
    }

    // ---------- Chat 会话表（委派给 ChatRepo） ----------

    pub fn create_chat(&self, id: &str, card: &str, title: &str) -> Result<ChatRecord> {
        ChatRepo::create_chat(&self.conn, id, card, title)
    }

    pub fn get_chat(&self, id: &str) -> Result<Option<ChatRecord>> {
        ChatRepo::get_chat(&self.conn, id)
    }

    pub fn list_chats(&self, card: Option<&str>) -> Result<Vec<ChatRecord>> {
        ChatRepo::list_chats(&self.conn, card)
    }

    pub fn rename_chat(&self, id: &str, title: &str) -> Result<usize> {
        ChatRepo::rename_chat(&self.conn, id, title)
    }

    pub fn delete_chat(&self, id: &str) -> Result<usize> {
        ChatRepo::delete_chat(&self.conn, id)
    }

    pub fn add_chat_message(
        &self,
        chat_id: &str,
        card: &str,
        role: &str,
        content: &str,
    ) -> Result<()> {
        ChatRepo::add_chat_message(&self.conn, chat_id, card, role, content)
    }

    // ---------- Memory 表（委派给 MemoryRepo） ----------

    pub fn add_memory(
        &self,
        card_name: &str,
        summary: &str,
        keywords: &str,
        source_round: i64,
    ) -> Result<i64> {
        MemoryRepo::add_memory(&self.conn, card_name, summary, keywords, source_round)
    }

    pub fn commit_memory_batch(
        &self,
        card_name: &str,
        summary: &str,
        upto_message_id: i64,
    ) -> Result<bool> {
        MemoryRepo::commit_memory_batch(&self.conn, card_name, summary, upto_message_id)
    }

    pub fn search(&self, card_name: &str, query: &str, limit: usize) -> Result<Vec<crate::types::MemoryHit>> {
        MemoryRepo::search(&self.conn, card_name, query, limit)
    }

    pub fn count_memories(&self, card_name: &str) -> Result<usize> {
        MemoryRepo::count_memories(&self.conn, card_name)
    }

    pub fn clear_memories(&self, card_name: &str) -> Result<usize> {
        MemoryRepo::clear_memories(&self.conn, card_name)
    }

    pub fn list_memories(&self, card_name: &str, limit: usize) -> Result<Vec<crate::types::MemoryHit>> {
        MemoryRepo::list_memories(&self.conn, card_name, limit)
    }

    // ---------- Settings 表（委派给 SettingsRepo） ----------

    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        SettingsRepo::get_setting(&self.conn, key)
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        SettingsRepo::set_setting(&self.conn, key, value)
    }

    // ---------- character_state 表（委派给 StateRepo） ----------

    pub fn get_state(&self, card_key: &str) -> Result<Option<serde_json::Value>> {
        StateRepo::get_state(&self.conn, card_key)
    }

    pub fn set_state(&self, card_key: &str, state: &serde_json::Value) -> Result<()> {
        StateRepo::set_state(&self.conn, card_key, state)
    }

    // ---------- Lore 表（委派给 LoreRepo） ----------

    pub fn replace_lore_entries(&self, card_name: &str, entries: &[crate::types::LoreEntry]) -> Result<()> {
        LoreRepo::replace_lore_entries(&self.conn, card_name, entries)
    }

    pub fn trigger_lore_entries(&self, card_name: &str, history: &[Msg]) -> Result<Vec<crate::types::LoreEntry>> {
        LoreRepo::trigger_lore_entries(&self.conn, card_name, history)
    }

    pub fn search_prompt_context(&self, card_name: &str, query: &str) -> Result<Vec<crate::types::PromptHit>> {
        LoreRepo::search_prompt_context(&self.conn, card_name, query)
    }

    pub fn memory_block(&self, card_name: &str, query: &str, max_chars: usize) -> Result<Option<String>> {
        LoreRepo::memory_block(&self.conn, card_name, query, max_chars)
    }

    // ---------- 跨表事务协调器 ----------

    pub fn commit_chat_turn(
        &self,
        chat_id: &str,
        card: &str,
        user: &str,
        assistant: &str,
        expected_last_id: i64,
        regenerate: bool,
    ) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        let chat = ChatRepo::get_chat(&tx, chat_id)?.context("chat was deleted")?;
        ensure!(chat.card == card, "chat belongs to a different card");
        ensure!(
            MessageRepo::last_chat_message_id(&tx, chat_id)? == expected_last_id,
            "conversation changed during generation; please reload"
        );
        if regenerate {
            let history = MessageRepo::chat_history(&tx, chat_id, 2)?;
            ensure!(
                history.len() == 2
                    && history[0].role == "user"
                    && history[0].content == user
                    && history[1].role == "assistant",
                "only the latest completed turn can be regenerated"
            );
            let checkpoint: i64 = tx.query_row(
                "SELECT COALESCE(MAX(source_message_id), 0) FROM memories
                 WHERE card_name = ?1 AND source_message_id < ?2",
                rusqlite::params![card, expected_last_id],
                |r| r.get(0),
            )?;
            tx.execute(
                "DELETE FROM memories_fts WHERE mem_id IN (
                SELECT id FROM memories WHERE card_name = ?1 AND source_message_id >= ?2)",
                rusqlite::params![card, expected_last_id],
            )?;
            tx.execute(
                "DELETE FROM memories WHERE card_name = ?1 AND source_message_id >= ?2",
                rusqlite::params![card, expected_last_id],
            )?;
            tx.execute(
                "UPDATE messages SET processed = 0 WHERE card_name = ?1 AND id > ?2",
                rusqlite::params![card, checkpoint],
            )?;
            tx.execute("DELETE FROM messages WHERE id = ?1", [expected_last_id])?;
        } else {
            tx.execute(
                "INSERT INTO messages (card_name, chat_id, role, content) VALUES (?1, ?2, 'user', ?3)",
                rusqlite::params![card, chat_id, user],
            )?;
        }
        tx.execute(
            "INSERT INTO messages (card_name, chat_id, role, content) VALUES (?1, ?2, 'assistant', ?3)",
            rusqlite::params![card, chat_id, assistant],
        )?;
        tx.execute(
            "UPDATE chats SET updated_at = datetime('now','localtime') WHERE id = ?1",
            [chat_id],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn delete_card_data(&self, card_name: &str, legacy_name: Option<&str>) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        let legacy_name = legacy_name.unwrap_or(card_name);
        tx.execute(
            "DELETE FROM memories_fts WHERE mem_id IN (SELECT id FROM memories WHERE card_name IN (?1, ?2))",
            rusqlite::params![card_name, legacy_name],
        )?;
        tx.execute(
            "DELETE FROM lore_entries_fts WHERE lore_id IN (SELECT id FROM lore_entries WHERE card_name IN (?1, ?2))",
            rusqlite::params![card_name, legacy_name],
        )?;
        tx.execute(
            "DELETE FROM memories WHERE card_name IN (?1, ?2)",
            rusqlite::params![card_name, legacy_name],
        )?;
        tx.execute(
            "DELETE FROM lore_entries WHERE card_name IN (?1, ?2)",
            rusqlite::params![card_name, legacy_name],
        )?;
        tx.execute(
            "DELETE FROM messages WHERE card_name IN (?1, ?2)",
            rusqlite::params![card_name, legacy_name],
        )?;
        tx.execute(
            "DELETE FROM chats WHERE card_name IN (?1, ?2)",
            rusqlite::params![card_name, legacy_name],
        )?;
        tx.execute(
            "DELETE FROM character_state WHERE card_name IN (?1, ?2)",
            rusqlite::params![card_name, legacy_name],
        )?;
        tx.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;
    use zhuo_card::{activated_entries, WorldBook, WorldEntry};
    use zhuo_llm::Msg;

    #[test]
    fn turn_commit_is_atomic_and_rejects_foreign_deleted_or_changed_chats() {
        let s = MemoryStore::open_in_memory().unwrap();
        s.create_chat("chat", "card", "test").unwrap();
        assert!(s.add_chat_message("chat", "other", "user", "bad").is_err());
        assert!(s
            .commit_chat_turn("chat", "other", "q", "a", 0, false)
            .is_err());
        s.conn
            .execute_batch(
                "CREATE TRIGGER fail_reply BEFORE INSERT ON messages
            WHEN NEW.role = 'assistant' BEGIN SELECT RAISE(ABORT, 'disk failure'); END;",
            )
            .unwrap();
        assert!(s
            .commit_chat_turn("chat", "card", "q", "a", 0, false)
            .is_err());
        assert!(
            s.chat_history("chat", 10).unwrap().is_empty(),
            "user insert must roll back"
        );
        s.conn.execute_batch("DROP TRIGGER fail_reply;").unwrap();
        s.commit_chat_turn("chat", "card", "q", "a", 0, false)
            .unwrap();
        assert!(s
            .commit_chat_turn("chat", "card", "stale", "bad", 0, false)
            .is_err());
        let last = s.last_chat_message_id("chat").unwrap();
        s.delete_chat("chat").unwrap();
        assert!(s
            .commit_chat_turn("chat", "card", "q", "a", last, false)
            .is_err());
        assert!(s.chat_history("chat", 10).unwrap().is_empty());
    }

    #[test]
    fn regeneration_replaces_reply_and_invalidates_derived_memory() {
        let s = MemoryStore::open_in_memory().unwrap();
        s.create_chat("chat", "card", "test").unwrap();
        s.commit_chat_turn("chat", "card", "question", "old answer", 0, false)
            .unwrap();
        let last = s.last_chat_message_id("chat").unwrap();
        s.commit_memory_batch("card", "old answer summary", last)
            .unwrap();
        s.add_memory("card", "manual memory", "", 0).unwrap();
        s.commit_chat_turn("chat", "card", "question", "new answer", last, true)
            .unwrap();
        let history = s.chat_history("chat", 10).unwrap();
        assert_eq!(
            history,
            vec![Msg::user("question"), Msg::assistant("new answer")]
        );
        assert_eq!(s.count_memories("card").unwrap(), 1);
        assert_eq!(s.count_unprocessed("card").unwrap(), 2);
        assert!(s.search("card", "summary", 10).unwrap().is_empty());
    }

    #[test]
    fn delete_chat_rolls_back_messages_if_chat_delete_fails() {
        let s = MemoryStore::open_in_memory().unwrap();
        s.create_chat("chat", "card", "test").unwrap();
        s.add_chat_message("chat", "card", "user", "keep").unwrap();
        s.conn
            .execute_batch(
                "CREATE TRIGGER fail_delete BEFORE DELETE ON chats
            BEGIN SELECT RAISE(ABORT, 'disk failure'); END;",
            )
            .unwrap();
        assert!(s.delete_chat("chat").is_err());
        assert_eq!(s.chat_history("chat", 10).unwrap()[0].content, "keep");
    }

    #[test]
    fn lore_sync_avoids_reindexing_unchanged_text_and_keeps_constants() {
        let s = MemoryStore::open_in_memory().unwrap();
        let mut entries: Vec<_> = (0..4)
            .map(|i| LoreEntry {
                source_key: i.to_string(),
                title: format!("entry{i}"),
                triggers: "dragon".into(),
                secondary_triggers: String::new(),
                content: format!("dragon lore {i}"),
                constant: false,
                enabled: true,
                selective: false,
                case_sensitive: false,
                match_whole_words: true,
                order: 0,
                scan_depth: None,
            })
            .collect();
        entries.push(LoreEntry {
            source_key: "constant".into(),
            title: "always".into(),
            triggers: String::new(),
            secondary_triggers: String::new(),
            content: "keep this rule".into(),
            constant: true,
            enabled: true,
            selective: false,
            case_sensitive: false,
            match_whole_words: true,
            order: 0,
            scan_depth: None,
        });
        s.replace_lore_entries("card", &entries).unwrap();
        let before: i64 = s
            .conn
            .query_row("SELECT total_changes()", [], |r| r.get(0))
            .unwrap();
        s.replace_lore_entries("card", &entries).unwrap();
        let after: i64 = s
            .conn
            .query_row("SELECT total_changes()", [], |r| r.get(0))
            .unwrap();
        assert_eq!(after, before);
        let hits = s.search_prompt_context("card", "dragon").unwrap();
        assert_eq!(hits.len(), 3);
        assert_eq!(hits[0].content, "keep this rule");
    }

    fn seed(store: &MemoryStore) {
        store
            .add_memory(
                "苏菲",
                "苏菲最喜欢吃山楂味的糖葫芦，冬天常在冬堡集市买。",
                "糖葫芦,零食",
                3,
            )
            .unwrap();
        store
            .add_memory(
                "苏菲",
                "冬堡常年下雪，冬天屋檐会挂冰凌。",
                "冬堡,天气,雪",
                5,
            )
            .unwrap();
        store.add_memory("苏菲", "冬堡学院图书馆的顶层藏着禁书区。", "学院,图书馆", 7)
            .unwrap();
    }

    #[test]
    fn seg_splits_chinese_and_drops_noise() {
        let toks = crate::jieba::seg("你喜欢什么呢？糖葫芦！");
        assert!(toks.contains("糖葫芦"));
        assert!(toks.contains("喜欢"));
        assert!(!toks.contains("的"));
        assert!(!toks.contains("？"));
    }

    #[test]
    fn fts5_query_or_joins_quoted_tokens() {
        let q = crate::jieba::fts5_query("冬堡下雪");
        assert!(q.contains('"'));
        assert!(q.contains(" OR "));
    }

    #[test]
    fn search_hits_relevant_memory() {
        let s = MemoryStore::open_in_memory().unwrap();
        seed(&s);
        let hits = s.search("苏菲", "苏菲喜欢吃什么零食", 5).unwrap();
        assert!(!hits.is_empty());
        assert!(hits[0].summary.contains("糖葫芦"));
        let hits = s.search("苏菲", "冬堡的天气怎么样", 5).unwrap();
        assert!(hits.iter().any(|h| h.summary.contains("下雪")));
        let hits = s.search("苏菲", "股票行情涨跌", 5).unwrap();
        assert!(hits.is_empty(), "无关查询不应命中: {:?}", hits);
    }

    #[test]
    fn memory_block_injects_and_falls_back() {
        let s = MemoryStore::open_in_memory().unwrap();
        seed(&s);
        let block = s.memory_block("苏菲", "苏菲喜欢吃什么零食", 600).unwrap();
        assert!(block.is_some());
        let b = block.unwrap();
        assert!(b.starts_with("[相关记忆]"));
        assert!(b.contains("糖葫芦"));

        assert!(s
            .memory_block("苏菲", "股票行情涨跌", 600)
            .unwrap()
            .is_none());
    }

    #[test]
    fn lore_entries_are_indexed_and_replaced_per_card() {
        let store = MemoryStore::open_in_memory().unwrap();
        let lore = vec![
            LoreEntry {
                source_key: "book:castle".into(),
                title: "冬堡城堡".into(),
                triggers: "冬堡 城堡".into(),
                secondary_triggers: String::new(),
                content: "冬堡城堡位于北方山脉，藏有古老魔法书。".into(),
                constant: false,
                enabled: true,
                selective: false,
                case_sensitive: false,
                match_whole_words: true,
                order: 0,
                scan_depth: None,
            },
            LoreEntry {
                source_key: "card:constant".into(),
                title: "苏菲身份".into(),
                triggers: "苏菲".into(),
                secondary_triggers: String::new(),
                content: "苏菲是一名法师。".into(),
                constant: true,
                enabled: true,
                selective: false,
                case_sensitive: false,
                match_whole_words: true,
                order: 0,
                scan_depth: None,
            },
        ];
        store.replace_lore_entries("card-a", &lore).unwrap();
        store.replace_lore_entries("card-b", &lore[..1]).unwrap();

        let hits = store.search_prompt_context("card-a", "冬堡魔法书").unwrap();
        assert!(hits.iter().any(|hit| hit.content.contains("古老魔法书")));
        assert!(hits.iter().any(|hit| hit.content.contains("法师")));
        assert!(store
            .search_prompt_context("card-b", "冬堡魔法书")
            .unwrap()
            .iter()
            .all(|hit| !hit.content.contains("法师")));

        store.replace_lore_entries("card-a", &[]).unwrap();
        assert!(store
            .search_prompt_context("card-a", "冬堡魔法书")
            .unwrap()
            .is_empty());
    }

    #[test]
    fn prompt_context_returns_at_most_three_hits_per_source() {
        let store = MemoryStore::open_in_memory().unwrap();
        for i in 0..5 {
            store
                .add_memory("card", &format!("共同关键词记忆事实{i}"), "共同关键词", i)
                .unwrap();
        }
        let lore = (0..5)
            .map(|i| LoreEntry {
                source_key: format!("book:{i}"),
                title: format!("共同关键词设定{i}"),
                triggers: "共同关键词".into(),
                secondary_triggers: String::new(),
                content: format!("共同关键词世界事实{i}"),
                constant: false,
                enabled: true,
                selective: false,
                case_sensitive: false,
                match_whole_words: true,
                order: 0,
                scan_depth: None,
            })
            .collect::<Vec<_>>();
        store.replace_lore_entries("card", &lore).unwrap();

        let hits = store.search_prompt_context("card", "共同关键词").unwrap();
        assert_eq!(hits.iter().filter(|hit| hit.kind == "记忆").count(), 3);
        assert_eq!(hits.iter().filter(|hit| hit.kind == "设定").count(), 3);
    }

    #[test]
    fn memory_block_respects_max_chars() {
        let s = MemoryStore::open_in_memory().unwrap();
        for i in 0..10 {
            s.add_memory(
                "苏菲",
                &format!("记忆条目{i:04}：这是一条比较长的记忆内容用来撑字符数。"),
                "",
                i,
            )
            .unwrap();
        }
        let b = s.memory_block("苏菲", "记忆条目", 80).unwrap().unwrap();
        assert!(b.chars().count() <= 80 + 40);
    }

    #[test]
    fn messages_roundtrip_and_clear() {
        let s = MemoryStore::open_in_memory().unwrap();
        s.add_message("苏菲", "user", "你好").unwrap();
        s.add_message("苏菲", "assistant", "要来串糖葫芦吗？")
            .unwrap();
        s.add_message("冬堡", "user", "另一张卡").unwrap();
        let h = s.history("苏菲", 50).unwrap();
        assert_eq!(h.len(), 2);
        assert_eq!(h[0].role, "user");
        assert_eq!(h[1].content, "要来串糖葫芦吗？");
        let h = s.history("苏菲", 1).unwrap();
        assert_eq!(h.len(), 1);
        assert_eq!(h[0].content, "要来串糖葫芦吗？");
        assert_eq!(s.clear_messages("苏菲").unwrap(), 2);
        assert!(s.history("苏菲", 50).unwrap().is_empty());
    }

    #[test]
    fn clear_memories_also_clears_fts() {
        let s = MemoryStore::open_in_memory().unwrap();
        seed(&s);
        assert_eq!(s.count_memories("苏菲").unwrap(), 3);
        s.clear_memories("苏菲").unwrap();
        assert_eq!(s.count_memories("苏菲").unwrap(), 0);
        assert!(s.search("苏菲", "糖葫芦", 5).unwrap().is_empty());
    }

    #[test]
    fn delete_card_data_removes_card_records_and_search_indexes() {
        let s = MemoryStore::open_in_memory().unwrap();
        s.add_memory("card-a", "冬堡学院在北方", "冬堡 学院", 1)
            .unwrap();
        s.add_memory("card-b", "冬堡学院在南方", "冬堡 学院", 1)
            .unwrap();
        s.add_memory("legacy-name", "旧版角色记忆", "旧版 角色", 1)
            .unwrap();
        s.replace_lore_entries(
            "card-a",
            &[LoreEntry {
                source_key: "book:1".into(),
                title: "冬堡".into(),
                triggers: "冬堡".into(),
                secondary_triggers: String::new(),
                content: "冬堡学院".into(),
                constant: false,
                enabled: true,
                selective: false,
                case_sensitive: false,
                match_whole_words: true,
                order: 0,
                scan_depth: None,
            }],
        )
        .unwrap();
        s.create_chat("chat-a", "card-a", "会话").unwrap();
        s.add_chat_message("chat-a", "card-a", "user", "你好")
            .unwrap();
        s.set_state("card-a", &serde_json::json!({"位置": "冬堡"}))
            .unwrap();

        s.delete_card_data("card-a", Some("legacy-name")).unwrap();

        assert_eq!(s.count_memories("card-a").unwrap(), 0);
        assert_eq!(s.count_memories("card-b").unwrap(), 1);
        assert_eq!(s.count_memories("legacy-name").unwrap(), 0);
        assert!(s.search("card-a", "冬堡学院", 10).unwrap().is_empty());
        assert!(s
            .search_prompt_context("card-a", "冬堡学院")
            .unwrap()
            .is_empty());
        assert!(s.list_chats(Some("card-a")).unwrap().is_empty());
        assert!(s.get_state("card-a").unwrap().is_none());
    }

    #[test]
    fn open_file_db_persists_across_connections() {
        let mut p = std::env::temp_dir();
        p.push(format!("zhuo_mem_test_{}.db", std::process::id()));
        let _ = std::fs::remove_file(&p);
        {
            let s = MemoryStore::open(&p).unwrap();
            s.add_memory("苏菲", "苏菲的猫叫雪球，只吃蜂蜜。", "雪球,猫", 2)
                .unwrap();
        }
        let s2 = MemoryStore::open(&p).unwrap();
        let hits = s2.search("苏菲", "苏菲的猫吃什么", 5).unwrap();
        assert!(hits.iter().any(|h| h.summary.contains("雪球")));
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn file_database_uses_wal_and_busy_timeout() {
        let mut p = std::env::temp_dir();
        p.push(format!("zhuo_wal_test_{}.db", std::process::id()));
        let _ = std::fs::remove_file(&p);

        let store = MemoryStore::open(&p).unwrap();
        let journal_mode: String = store
            .conn
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .unwrap();
        let busy_timeout: i64 = store
            .conn
            .query_row("PRAGMA busy_timeout", [], |row| row.get(0))
            .unwrap();

        assert_eq!(journal_mode.to_ascii_lowercase(), "wal");
        assert_eq!(busy_timeout, 5000);
        drop(store);
        let mut wal = p.as_os_str().to_os_string();
        wal.push("-wal");
        let _ = std::fs::remove_file(std::path::PathBuf::from(wal));
        let mut shm = p.as_os_str().to_os_string();
        shm.push("-shm");
        let _ = std::fs::remove_file(std::path::PathBuf::from(shm));
    }

    #[test]
    fn unprocessed_cursor_counts_and_marks() {
        let s = MemoryStore::open_in_memory().unwrap();
        s.add_message("苏菲", "user", "一").unwrap();
        s.add_message("苏菲", "assistant", "二").unwrap();
        s.add_message("冬堡", "user", "别的卡").unwrap();
        assert_eq!(s.count_unprocessed("苏菲").unwrap(), 2);

        let rows = s.unprocessed("苏菲", 10).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].1.content, "一");

        let rows = s.unprocessed("苏菲", 1).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].1.content, "一");

        let max_id = rows[0].0;
        assert_eq!(s.mark_processed("苏菲", max_id).unwrap(), 1);
        assert_eq!(s.count_unprocessed("苏菲").unwrap(), 1);
        assert_eq!(s.unprocessed("苏菲", 1).unwrap()[0].1.content, "二");
        assert_eq!(s.count_unprocessed("冬堡").unwrap(), 1);
        assert_eq!(s.mark_processed("苏菲", max_id).unwrap(), 0);
    }

    #[test]
    fn list_memories_returns_recent_desc() {
        let s = MemoryStore::open_in_memory().unwrap();
        for i in 0..3 {
            s.add_memory("苏菲", &format!("记忆{i}"), "", i).unwrap();
        }
        let list = s.list_memories("苏菲", 2).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].summary, "记忆2");
        assert_eq!(list[1].summary, "记忆1");
        assert!(!list[0].created_at.is_empty());
    }

    #[test]
    fn migration_adds_processed_to_legacy_db() {
        let mut p = std::env::temp_dir();
        p.push(format!("zhuo_mem_legacy_{}.db", std::process::id()));
        let _ = std::fs::remove_file(&p);
        {
            let conn = rusqlite::Connection::open(&p).unwrap();
            conn.execute_batch(
                "CREATE TABLE messages (
                     id INTEGER PRIMARY KEY AUTOINCREMENT,
                     card_name TEXT NOT NULL,
                     role TEXT NOT NULL,
                     content TEXT NOT NULL,
                     created_at TEXT NOT NULL DEFAULT (datetime('now'))
                 );
                 CREATE TABLE memories (
                     id INTEGER PRIMARY KEY AUTOINCREMENT,
                     card_name TEXT NOT NULL,
                     summary TEXT NOT NULL,
                     keywords TEXT NOT NULL DEFAULT '',
                     source_round INTEGER NOT NULL DEFAULT 0,
                     created_at TEXT NOT NULL DEFAULT (datetime('now'))
                 );
                 INSERT INTO messages (card_name, role, content) VALUES ('苏菲','user','旧消息');",
            )
            .unwrap();
        }
        let s = MemoryStore::open(&p).unwrap();
        assert_eq!(s.count_unprocessed("苏菲").unwrap(), 1);
        let source_column: i64 = s
            .conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('memories')
                 WHERE name = 'source_message_id'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(source_column, 1);
        let rows = s.unprocessed("苏菲", 10).unwrap();
        assert_eq!(rows[0].1.content, "旧消息");
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn memory_batch_rolls_back_when_cursor_update_fails_and_can_retry() {
        let store = MemoryStore::open_in_memory().unwrap();
        store.add_message("苏菲", "user", "重要事件").unwrap();
        let upto_message_id = store
            .conn
            .query_row("SELECT MAX(id) FROM messages", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap();
        store
            .conn
            .execute_batch(
                "CREATE TRIGGER block_processed_cursor BEFORE UPDATE OF processed ON messages
                 WHEN NEW.processed = 1
                 BEGIN SELECT RAISE(ABORT, 'blocked'); END;",
            )
            .unwrap();

        assert!(store
            .commit_memory_batch("苏菲", "重要事件已记录", upto_message_id)
            .is_err());
        assert_eq!(store.count_memories("苏菲").unwrap(), 0);
        assert_eq!(store.count_unprocessed("苏菲").unwrap(), 1);
        let fts_count: i64 = store
            .conn
            .query_row("SELECT COUNT(*) FROM memories_fts", [], |row| row.get(0))
            .unwrap();
        assert_eq!(fts_count, 0);

        store
            .conn
            .execute_batch("DROP TRIGGER block_processed_cursor;")
            .unwrap();
        assert!(store
            .commit_memory_batch("苏菲", "重要事件已记录", upto_message_id)
            .unwrap());
        assert_eq!(store.count_memories("苏菲").unwrap(), 1);
        assert_eq!(store.count_unprocessed("苏菲").unwrap(), 0);
    }

    #[test]
    fn memory_batch_retry_does_not_duplicate_memory_or_fts() {
        let store = MemoryStore::open_in_memory().unwrap();
        store.add_message("苏菲", "user", "重要事件").unwrap();
        let upto_message_id = store
            .conn
            .query_row("SELECT MAX(id) FROM messages", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap();

        assert!(store
            .commit_memory_batch("苏菲", "重要事件已记录", upto_message_id)
            .unwrap());
        assert!(!store
            .commit_memory_batch("苏菲", "重要事件已记录", upto_message_id)
            .unwrap());

        let fts_count: i64 = store
            .conn
            .query_row(
                "SELECT COUNT(*) FROM memories_fts WHERE card_name = '苏菲'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(store.count_memories("苏菲").unwrap(), 1);
        assert_eq!(fts_count, 1);
        assert_eq!(store.count_unprocessed("苏菲").unwrap(), 0);
    }

    #[test]
    fn message_and_memory_batch_indexes_are_created() {
        let store = MemoryStore::open_in_memory().unwrap();
        for index in [
            "idx_messages_chat_id",
            "idx_messages_unprocessed",
            "idx_memories_card_source_message",
        ] {
            let count: i64 = store
                .conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = ?1",
                    [index],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(count, 1, "missing index {index}");
        }
    }

    #[test]
    fn character_state_roundtrip_upsert_and_reject() {
        let s = MemoryStore::open_in_memory().unwrap();
        assert!(s.get_state("card_a").unwrap().is_none());
        s.set_state(
            "card_a",
            &serde_json::json!({"背包": ["木剑", "药水"], "位置": "冬堡学院", "好感度": 5}),
        )
        .unwrap();
        let st = s.get_state("card_a").unwrap().unwrap();
        assert_eq!(st["位置"], "冬堡学院");
        assert_eq!(st["背包"][0], "木剑");
        s.set_state("card_a", &serde_json::json!({"位置": "集市"}))
            .unwrap();
        let st = s.get_state("card_a").unwrap().unwrap();
        assert_eq!(st["位置"], "集市");
        assert!(st.get("背包").is_none(), "整对象替换，不是合并");
        assert!(s.get_state("card_b").unwrap().is_none());
        assert!(s.set_state("card_a", &serde_json::json!([1, 2])).is_err());
    }

    #[test]
    fn chats_and_settings_roundtrip() {
        let s = MemoryStore::open_in_memory().unwrap();
        let chat = s.create_chat("chat-1", "card-a", "第一幕").unwrap();
        assert_eq!(chat.title, "第一幕");
        s.add_chat_message("chat-1", "card-a", "user", "你好")
            .unwrap();
        s.add_chat_message("chat-1", "card-a", "assistant", "欢迎")
            .unwrap();
        assert_eq!(s.chat_history("chat-1", 20).unwrap().len(), 2);
        s.rename_chat("chat-1", "重逢").unwrap();
        assert_eq!(s.list_chats(Some("card-a")).unwrap()[0].title, "重逢");
        s.set_setting("connection", "{\"model\":\"test\"}").unwrap();
        assert_eq!(
            s.get_setting("connection").unwrap().unwrap(),
            "{\"model\":\"test\"}"
        );
        assert_eq!(s.delete_chat("chat-1").unwrap(), 1);
    }

    #[test]
    fn lore_trigger_matches_sillytavern_standard() {
        let store = MemoryStore::open_in_memory().unwrap();

        let world_book = WorldBook {
            name: "冬堡世界书".into(),
            description: "测试用世界书".into(),
            scan_depth: Some(4),
            entries: vec![
                WorldEntry {
                    id: "1".into(),
                    keys: vec!["冬堡".into(), "雪祭典".into()],
                    secondary_keys: vec!["集市".into()],
                    content: "冬堡是北方山城，一年中八个月下雪。每年入冬第一场雪会举办雪祭典。"
                        .into(),
                    constant: false,
                    selective: true,
                    case_sensitive: false,
                    match_whole_words: true,
                    enabled: true,
                    order: 10,
                    scan_depth: Some(4),
                },
                WorldEntry {
                    id: "2".into(),
                    keys: vec!["糖葫芦".into()],
                    secondary_keys: vec![],
                    content: "冰糖葫芦是冬堡集市最常见的小吃，山楂裹糖，冷天糖壳格外脆。".into(),
                    constant: false,
                    selective: false,
                    case_sensitive: false,
                    match_whole_words: true,
                    enabled: true,
                    order: 20,
                    scan_depth: None,
                },
                WorldEntry {
                    id: "3".into(),
                    keys: vec!["苏菲".into()],
                    secondary_keys: vec!["雪球".into()],
                    content: "苏菲是冬堡学院的学生，养了一只叫雪球的猫，只吃蜂蜜。".into(),
                    constant: true,
                    selective: true,
                    case_sensitive: false,
                    match_whole_words: true,
                    enabled: true,
                    order: 5,
                    scan_depth: None,
                },
                WorldEntry {
                    id: "4".into(),
                    keys: vec!["禁书".into()],
                    secondary_keys: vec!["图书馆".into()],
                    content: "冬堡学院图书馆顶层藏有禁书区，记录着古老魔法。".into(),
                    constant: false,
                    selective: true,
                    case_sensitive: true,
                    match_whole_words: true,
                    enabled: true,
                    order: 30,
                    scan_depth: Some(3),
                },
                WorldEntry {
                    id: "5".into(),
                    keys: vec!["老约翰".into()],
                    secondary_keys: vec!["蜂蜜".into()],
                    content: "集市上老约翰的铺子卖雪季前野花酿的蜂蜜。".into(),
                    constant: false,
                    selective: true,
                    case_sensitive: false,
                    match_whole_words: true,
                    enabled: false,
                    order: 40,
                    scan_depth: None,
                },
            ],
        };

        let lore_entries: Vec<_> = world_book
            .entries
            .iter()
            .map(|e| LoreEntry {
                source_key: e.id.clone(),
                title: format!("条目{}", e.id),
                triggers: e.keys.join(","),
                secondary_triggers: e.secondary_keys.join(","),
                content: e.content.clone(),
                constant: e.constant,
                enabled: e.enabled,
                selective: e.selective,
                case_sensitive: e.case_sensitive,
                match_whole_words: e.match_whole_words,
                order: e.order,
                scan_depth: e.scan_depth,
            })
            .collect();
        store.replace_lore_entries("测试卡", &lore_entries).unwrap();

        let history = vec![
            Msg::user("苏菲，冬堡的雪祭典是什么时候？"),
            Msg::assistant("每年入冬第一场雪落下的那天，全城会在中央广场点起篝火。"),
            Msg::user("对了，苏菲你店里那只猫叫什么名字？"),
            Msg::assistant("它叫雪球，是三年前我在集市门口捡到的小家伙。"),
            Msg::user("听说冬堡学院图书馆有禁书区？"),
            Msg::assistant("那是传说，顶层藏着古老魔法书，普通学生进不去。"),
        ];

        let history_text: Vec<String> = history.iter().map(|m| m.content.clone()).collect();
        let expected = activated_entries(&world_book, &history_text);

        let actual = store.trigger_lore_entries("测试卡", &history).unwrap();

        let expected_keys: Vec<_> = expected.iter().map(|e| e.id.clone()).collect();
        let actual_keys: Vec<_> = actual.iter().map(|e| e.source_key.clone()).collect();

        println!("zhuo-card (标准) 触发: {:?}", expected_keys);
        println!("zhuo-memory 触发: {:?}", actual_keys);

        assert_eq!(
            actual_keys, expected_keys,
            "触发条目集合必须与 SillyTavern 标准一致"
        );

        assert!(
            actual.iter().any(|e| e.source_key == "3"),
            "常量条目必须触发"
        );
        assert!(
            !actual.iter().any(|e| e.source_key == "1"),
            "全词匹配下冬堡条目不应触发"
        );
        assert!(
            !actual.iter().any(|e| e.source_key == "2"),
            "未提及关键词的条目不应触发"
        );
        assert!(
            !actual.iter().any(|e| e.source_key == "4"),
            "全词匹配下禁书条目不应触发"
        );
        assert!(
            !actual.iter().any(|e| e.source_key == "5"),
            "禁用条目不应触发"
        );

        println!("✅ 世界书触发引擎测试通过：zhuo-memory 与 SillyTavern 标准行为一致");
    }
}
