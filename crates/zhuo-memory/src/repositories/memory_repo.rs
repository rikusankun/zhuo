//! Memory 表 + FTS5：长期记忆卡片 CRUD 与检索。

use anyhow::Result;
use rusqlite::Connection;

use crate::jieba::seg;
use crate::types::MemoryHit;

pub struct MemoryRepo;

impl MemoryRepo {
    pub fn add_memory(
        conn: &Connection,
        card_name: &str,
        summary: &str,
        keywords: &str,
        source_round: i64,
    ) -> Result<i64> {
        if summary.trim().is_empty() {
            anyhow::bail!("memory summary is empty");
        }
        let tx = conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO memories (card_name, summary, keywords, source_round)
                 VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![card_name, summary.trim(), keywords.trim(), source_round],
        )?;
        let id = tx.last_insert_rowid();
        let body = seg(&format!("{} {}", summary, keywords));
        tx.execute(
            "INSERT INTO memories_fts (mem_id, card_name, body) VALUES (?1, ?2, ?3)",
            rusqlite::params![id, card_name, body],
        )?;
        tx.commit()?;
        Ok(id)
    }

    pub fn commit_memory_batch(
        conn: &Connection,
        card_name: &str,
        summary: &str,
        upto_message_id: i64,
    ) -> Result<bool> {
        if summary.trim().is_empty() {
            anyhow::bail!("memory summary is empty");
        }

        let tx = conn.unchecked_transaction()?;
        let inserted = tx
            .execute(
                "INSERT INTO memories
                    (card_name, summary, source_round, source_message_id)
                 VALUES (?1, ?2, ?3, ?3)
                 ON CONFLICT(card_name, source_message_id) DO NOTHING",
                rusqlite::params![card_name, summary.trim(), upto_message_id],
            )?;
        if inserted > 0 {
            let memory_id = tx.last_insert_rowid();
            let body = seg(summary);
            tx.execute(
                "INSERT INTO memories_fts (mem_id, card_name, body) VALUES (?1, ?2, ?3)",
                rusqlite::params![memory_id, card_name, body],
            )?;
        }
        tx.execute(
            "UPDATE messages SET processed = 1
             WHERE card_name = ?1 AND id <= ?2 AND processed = 0",
            rusqlite::params![card_name, upto_message_id],
        )?;
        tx.commit()?;
        Ok(inserted > 0)
    }

    pub fn search(
        conn: &Connection,
        card_name: &str,
        query: &str,
        limit: usize,
    ) -> Result<Vec<MemoryHit>> {
        let q = crate::jieba::fts5_query(query);
        if q.is_empty() {
            return Ok(Vec::new());
        }
        let mut stmt = conn.prepare(
            "SELECT m.id, m.summary, m.keywords, m.source_round, m.created_at
             FROM memories_fts f
             JOIN memories m ON m.id = f.mem_id
             WHERE f.card_name = ?2 AND memories_fts MATCH ?1
             ORDER BY bm25(memories_fts)
             LIMIT ?3",
        )?;
        let rows = stmt.query_map(rusqlite::params![q, card_name, limit as i64], |r| {
            Ok(MemoryHit {
                id: r.get(0)?,
                summary: r.get(1)?,
                keywords: r.get(2)?,
                source_round: r.get(3)?,
                created_at: r.get(4)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn count_memories(conn: &Connection, card_name: &str) -> Result<usize> {
        Ok(conn
            .query_row(
                "SELECT COUNT(*) FROM memories WHERE card_name = ?1",
                [card_name],
                |r| r.get::<_, i64>(0),
            )? as usize)
    }

    pub fn clear_memories(conn: &Connection, card_name: &str) -> Result<usize> {
        let tx = conn.unchecked_transaction()?;
        let n = tx
            .execute("DELETE FROM memories WHERE card_name = ?1", [card_name])?;
        tx.execute("DELETE FROM memories_fts WHERE card_name = ?1", [card_name])?;
        tx.commit()?;
        Ok(n)
    }

    pub fn list_memories(conn: &Connection, card_name: &str, limit: usize) -> Result<Vec<MemoryHit>> {
        let mut stmt = conn.prepare(
            "SELECT id, summary, keywords, source_round, created_at
             FROM memories WHERE card_name = ?1 ORDER BY id DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(rusqlite::params![card_name, limit as i64], |r| {
            Ok(MemoryHit {
                id: r.get(0)?,
                summary: r.get(1)?,
                keywords: r.get(2)?,
                source_round: r.get(3)?,
                created_at: r.get(4)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }
}
