//! Lore 表 + FTS5：世界书条目 CRUD、替换、提取与注入块。

use anyhow::Result;
use rusqlite::Connection;

use crate::jieba::seg;
use crate::lore_engine::activated_entries_internal;
use crate::repositories::memory_repo::MemoryRepo;
use crate::types::{LoreEntry, PromptHit};

pub struct LoreRepo;

impl LoreRepo {
    pub fn replace_lore_entries(
        conn: &Connection,
        card_name: &str,
        entries: &[LoreEntry],
    ) -> Result<()> {
        let mut expected: Vec<_> = entries
            .iter()
            .filter(|e| !e.content.trim().is_empty())
            .cloned()
            .collect();
        expected.sort_by(|a, b| a.source_key.cmp(&b.source_key));

        let mut stmt = conn.prepare(
            "SELECT source_key, title, triggers, secondary_triggers, content, constant, enabled, selective, case_sensitive, match_whole_words, order_val, scan_depth
             FROM lore_entries WHERE card_name = ?1 ORDER BY source_key",
        )?;
        let rows = stmt.query_map([card_name], |r| {
            Ok(LoreEntry {
                source_key: r.get(0)?,
                title: r.get(1)?,
                triggers: r.get(2)?,
                secondary_triggers: r.get(3)?,
                content: r.get(4)?,
                constant: r.get(5)?,
                enabled: r.get(6)?,
                selective: r.get(7)?,
                case_sensitive: r.get(8)?,
                match_whole_words: r.get(9)?,
                order: r.get(10)?,
                scan_depth: r.get(11)?,
            })
        })?;
        let current = rows.collect::<std::result::Result<Vec<_>, _>>()?;

        if current == expected {
            return Ok(());
        }

        let tx = conn.unchecked_transaction()?;
        tx.execute("DELETE FROM lore_entries_fts WHERE card_name = ?1", [card_name])?;
        tx.execute("DELETE FROM lore_entries WHERE card_name = ?1", [card_name])?;

        for entry in entries.iter().filter(|entry| !entry.content.trim().is_empty()) {
            tx.execute(
                "INSERT INTO lore_entries (card_name, source_key, title, triggers, secondary_triggers, content, constant, enabled, selective, case_sensitive, match_whole_words, order_val, scan_depth)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                rusqlite::params![
                    card_name,
                    entry.source_key,
                    entry.title,
                    entry.triggers,
                    entry.secondary_triggers,
                    entry.content,
                    entry.constant,
                    entry.enabled,
                    entry.selective,
                    entry.case_sensitive,
                    entry.match_whole_words,
                    entry.order,
                    entry.scan_depth,
                ],
            )?;
            let id = tx.last_insert_rowid();
            let body = seg(&format!(
                "{} {} {} {}",
                entry.title, entry.triggers, entry.secondary_triggers, entry.content
            ));
            tx.execute(
                "INSERT INTO lore_entries_fts (lore_id, card_name, body) VALUES (?1, ?2, ?3)",
                rusqlite::params![id, card_name, body],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn load_lore_entries(conn: &Connection, card_name: &str) -> Result<Vec<LoreEntry>> {
        let mut stmt = conn.prepare(
            "SELECT source_key, title, triggers, secondary_triggers, content, constant, enabled, selective, case_sensitive, match_whole_words, order_val, scan_depth
             FROM lore_entries WHERE card_name = ?1 AND enabled = 1 ORDER BY source_key",
        )?;
        let rows = stmt.query_map([card_name], |r| {
            Ok(LoreEntry {
                source_key: r.get(0)?,
                title: r.get(1)?,
                triggers: r.get(2)?,
                secondary_triggers: r.get(3)?,
                content: r.get(4)?,
                constant: r.get(5)?,
                enabled: r.get(6)?,
                selective: r.get(7)?,
                case_sensitive: r.get(8)?,
                match_whole_words: r.get(9)?,
                order: r.get(10)?,
                scan_depth: r.get(11)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn trigger_lore_entries(conn: &Connection, card_name: &str, history: &[zhuo_llm::Msg]) -> Result<Vec<LoreEntry>> {
        let entries = Self::load_lore_entries(conn, card_name)?;
        let history_text: Vec<String> = history.iter().map(|m| m.content.clone()).collect();
        Ok(activated_entries_internal(&entries, &history_text))
    }

    pub fn search_prompt_context(
        conn: &Connection,
        card_name: &str,
        query: &str,
    ) -> Result<Vec<PromptHit>> {
        let q = crate::jieba::fts5_query(query);
        let mut hits = Vec::new();

        if !q.is_empty() {
            let mut stmt = conn.prepare(
                "SELECT m.summary, bm25(memories_fts)
                 FROM memories_fts
                 JOIN memories m ON m.id = memories_fts.mem_id
                 WHERE memories_fts.card_name = ?2 AND memories_fts MATCH ?1
                 ORDER BY bm25(memories_fts)
                 LIMIT 3",
            )?;
            let rows = stmt.query_map(rusqlite::params![q, card_name], |row| {
                Ok(PromptHit {
                    kind: "记忆",
                    title: "长期记忆".to_string(),
                    content: row.get(0)?,
                    score: row.get(1)?,
                })
            })?;
            hits.extend(rows.collect::<std::result::Result<Vec<_>, _>>()?);

            let mut stmt = conn.prepare(
                "SELECT l.title, l.content, bm25(lore_entries_fts)
                 FROM lore_entries_fts
                 JOIN lore_entries l ON l.id = lore_entries_fts.lore_id
                 WHERE lore_entries_fts.card_name = ?2 AND lore_entries_fts MATCH ?1
                 ORDER BY bm25(lore_entries_fts)
                 LIMIT 3",
            )?;
            let rows = stmt.query_map(rusqlite::params![q, card_name], |row| {
                Ok(PromptHit {
                    kind: "设定",
                    title: row.get(0)?,
                    content: row.get(1)?,
                    score: row.get(2)?,
                })
            })?;
            hits.extend(rows.collect::<std::result::Result<Vec<_>, _>>()?);
        }

        let mut stmt = conn.prepare(
            "SELECT title, content
             FROM lore_entries
             WHERE card_name = ?1 AND constant = 1
             ORDER BY id
             LIMIT 3",
        )?;
        let rows = stmt.query_map([card_name], |row| {
            Ok(PromptHit {
                kind: "设定",
                title: row.get(0)?,
                content: row.get(1)?,
                score: f64::NEG_INFINITY,
            })
        })?;
        for constant in rows.collect::<std::result::Result<Vec<_>, _>>()? {
            if let Some(existing) = hits.iter_mut().find(|hit| {
                hit.kind == "设定" && hit.title == constant.title && hit.content == constant.content
            }) {
                existing.score = f64::NEG_INFINITY;
            } else {
                hits.push(constant);
            }
        }

        hits.sort_by(|a, b| a.score.total_cmp(&b.score));
        let mut memory_count = 0;
        let mut lore_count = 0;
        hits.retain(|hit| {
            let count = if hit.kind == "记忆" {
                &mut memory_count
            } else {
                &mut lore_count
            };
            if *count >= 3 {
                return false;
            }
            *count += 1;
            true
        });
        Ok(hits)
    }

    pub fn memory_block(
        conn: &Connection,
        card_name: &str,
        query: &str,
        max_chars: usize,
    ) -> Result<Option<String>> {
        let hits = MemoryRepo::search(conn, card_name, query, 5)?;
        if hits.is_empty() {
            return Ok(None);
        }
        let mut block =
            String::from("[相关记忆]\n以下是从长期记忆中检索到的、与当前对话相关的事实：\n");
        for h in &hits {
            let line = format!("- {}\n", h.summary);
            if block.chars().count() + line.chars().count() > max_chars {
                break;
            }
            block.push_str(&line);
        }
        Ok(Some(block))
    }
}
