//! app_settings 表：应用配置键值对 CRUD。

use anyhow::Result;
use rusqlite::Connection;

pub struct SettingsRepo;

impl SettingsRepo {
    pub fn get_setting(conn: &Connection, key: &str) -> Result<Option<String>> {
        conn.query_row(
            "SELECT value FROM app_settings WHERE key = ?1",
            [key],
            |r| r.get(0),
        )
        .map(Some)
        .map_err(|e| e.into())
        .or_else(|e: anyhow::Error| match e.downcast_ref::<rusqlite::Error>() {
            Some(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            _ => Err(e),
        })
    }

    pub fn set_setting(conn: &Connection, key: &str, value: &str) -> Result<()> {
        conn.execute(
            "INSERT INTO app_settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = datetime('now','localtime')",
            rusqlite::params![key, value],
        )?;
        Ok(())
    }
}
