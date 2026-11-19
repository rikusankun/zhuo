//! character_state 表：角色状态机 CRUD。

use anyhow::{Context, Result};
use rusqlite::Connection;
use serde_json::Value;

pub struct StateRepo;

impl StateRepo {
    pub fn get_state(conn: &Connection, card_key: &str) -> Result<Option<Value>> {
        let raw: Option<String> = conn
            .query_row(
                "SELECT state_json FROM character_state WHERE card_name = ?1",
                [card_key],
                |r| r.get(0),
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })?;
        match raw {
            Some(s) => Ok(Some(
                serde_json::from_str(&s).context("parse character_state json")?,
            )),
            None => Ok(None),
        }
    }

    pub fn set_state(conn: &Connection, card_key: &str, state: &Value) -> Result<()> {
        if !state.is_object() {
            anyhow::bail!("character_state must be a JSON object");
        }
        let raw = serde_json::to_string(state).context("serialize state")?;
        conn.execute(
            "INSERT INTO character_state (card_name, state_json, updated_at)
             VALUES (?1, ?2, datetime('now','localtime'))
             ON CONFLICT(card_name) DO UPDATE SET
                 state_json = excluded.state_json,
                 updated_at = excluded.updated_at",
            rusqlite::params![card_key, raw],
        )?;
        Ok(())
    }
}
