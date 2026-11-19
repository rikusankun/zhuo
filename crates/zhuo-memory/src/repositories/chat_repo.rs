//! Chat 会话表 CRUD。

use anyhow::{ensure, Context, Result};
use rusqlite::Connection;

use crate::types::ChatRecord;

pub struct ChatRepo;

impl ChatRepo {
    pub fn create_chat(conn: &Connection, id: &str, card: &str, title: &str) -> Result<ChatRecord> {
        conn.execute(
            "INSERT INTO chats (id, card_name, title) VALUES (?1, ?2, ?3)",
            rusqlite::params![id, card, title],
        )?;
        Self::get_chat(conn, id)?.context("created chat missing")
    }

    pub fn get_chat(conn: &Connection, id: &str) -> Result<Option<ChatRecord>> {
        let row = conn.query_row(
            "SELECT id, card_name, title, created_at, updated_at FROM chats WHERE id = ?1",
            [id],
            |r| {
                Ok(ChatRecord {
                    id: r.get(0)?,
                    card: r.get(1)?,
                    title: r.get(2)?,
                    created_at: r.get(3)?,
                    updated_at: r.get(4)?,
                })
            },
        );
        match row {
            Ok(r) => Ok(Some(r)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn list_chats(conn: &Connection, card: Option<&str>) -> Result<Vec<ChatRecord>> {
        let sql = if card.is_some() {
            "SELECT id, card_name, title, created_at, updated_at FROM chats WHERE card_name = ?1 ORDER BY updated_at DESC"
        } else {
            "SELECT id, card_name, title, created_at, updated_at FROM chats ORDER BY updated_at DESC"
        };
        let mut stmt = conn.prepare(sql)?;
        let make = |r: &rusqlite::Row<'_>| {
            Ok(ChatRecord {
                id: r.get(0)?,
                card: r.get(1)?,
                title: r.get(2)?,
                created_at: r.get(3)?,
                updated_at: r.get(4)?,
            })
        };
        let rows = if let Some(card) = card {
            stmt.query_map([card], make)?
        } else {
            stmt.query_map([], make)?
        };
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn rename_chat(conn: &Connection, id: &str, title: &str) -> Result<usize> {
        Ok(conn.execute(
            "UPDATE chats SET title = ?2, updated_at = datetime('now','localtime') WHERE id = ?1",
            rusqlite::params![id, title],
        )?)
    }

    pub fn delete_chat(conn: &Connection, id: &str) -> Result<usize> {
        let tx = conn.unchecked_transaction()?;
        tx.execute("DELETE FROM messages WHERE chat_id = ?1", [id])?;
        let deleted = tx.execute("DELETE FROM chats WHERE id = ?1", [id])?;
        tx.commit()?;
        Ok(deleted)
    }

    pub fn add_chat_message(
        conn: &Connection,
        chat_id: &str,
        card: &str,
        role: &str,
        content: &str,
    ) -> Result<()> {
        let chat = Self::get_chat(conn, chat_id)?.context("chat not found")?;
        ensure!(chat.card == card, "chat belongs to a different card");
        let tx = conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO messages (card_name, chat_id, role, content) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![card, chat_id, role, content],
        )?;
        tx.execute(
            "UPDATE chats SET updated_at = datetime('now','localtime') WHERE id = ?1",
            [chat_id],
        )?;
        tx.commit()?;
        Ok(())
    }
}
