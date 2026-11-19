//! Message 表：对话历史 CRUD。

use anyhow::Result;
use rusqlite::Connection;
use zhuo_llm::Msg;

pub struct MessageRepo;

impl MessageRepo {
    pub fn add_message(conn: &Connection, card_name: &str, role: &str, content: &str) -> Result<()> {
        conn.execute(
            "INSERT INTO messages (card_name, role, content) VALUES (?1, ?2, ?3)",
            rusqlite::params![card_name, role, content],
        )?;
        Ok(())
    }

    pub fn history(conn: &Connection, card_name: &str, limit: usize) -> Result<Vec<Msg>> {
        let mut stmt = conn.prepare(
            "SELECT role, content FROM (
                 SELECT role, content, id FROM messages
                 WHERE card_name = ?1 ORDER BY id DESC LIMIT ?2
             ) ORDER BY id ASC",
        )?;
        let rows = stmt.query_map(rusqlite::params![card_name, limit as i64], |r| {
            Ok(Msg::new(r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn clear_messages(conn: &Connection, card_name: &str) -> Result<usize> {
        Ok(conn.execute("DELETE FROM messages WHERE card_name = ?1", [card_name])?)
    }

    pub fn chat_history(conn: &Connection, chat_id: &str, limit: usize) -> Result<Vec<Msg>> {
        let mut stmt = conn.prepare(
            "SELECT role, content FROM (SELECT role, content, id FROM messages WHERE chat_id = ?1 ORDER BY id DESC LIMIT ?2) ORDER BY id ASC",
        )?;
        let rows = stmt.query_map(rusqlite::params![chat_id, limit as i64], |r| {
            Ok(Msg::new(r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn clear_chat_messages(conn: &Connection, chat_id: &str) -> Result<usize> {
        Ok(conn.execute("DELETE FROM messages WHERE chat_id = ?1", [chat_id])?)
    }

    pub fn count_unprocessed(conn: &Connection, card_name: &str) -> Result<usize> {
        Ok(conn
            .query_row(
                "SELECT COUNT(*) FROM messages WHERE card_name = ?1 AND processed = 0",
                [card_name],
                |r| r.get::<_, i64>(0),
            )? as usize)
    }

    pub fn unprocessed(conn: &Connection, card_name: &str, limit: usize) -> Result<Vec<(i64, Msg)>> {
        let mut stmt = conn.prepare(
            "SELECT id, role, content FROM (
                 SELECT id, role, content FROM messages
                 WHERE card_name = ?1 AND processed = 0
                 ORDER BY id ASC LIMIT ?2
             ) ORDER BY id ASC",
        )?;
        let rows = stmt.query_map(rusqlite::params![card_name, limit as i64], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                Msg::new(r.get::<_, String>(1)?, r.get::<_, String>(2)?),
            ))
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn mark_processed(conn: &Connection, card_name: &str, upto_id: i64) -> Result<usize> {
        Ok(conn
            .execute(
                "UPDATE messages SET processed = 1
                 WHERE card_name = ?1 AND processed = 0 AND id <= ?2",
                rusqlite::params![card_name, upto_id],
            )?)
    }

    pub fn last_chat_message_id(conn: &Connection, chat_id: &str) -> Result<i64> {
        Ok(conn.query_row(
            "SELECT COALESCE(MAX(id), 0) FROM messages WHERE chat_id = ?1",
            [chat_id],
            |r| r.get(0),
        )?)
    }
}
