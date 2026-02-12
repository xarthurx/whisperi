pub mod migrations;

use anyhow::{Context, Result};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::AppHandle;
use tauri::Manager;

pub struct Database {
    conn: Mutex<Connection>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transcription {
    pub id: i64,
    pub timestamp: String,
    pub original_text: String,
    pub processed_text: Option<String>,
    pub is_processed: bool,
    pub processing_method: String,
    pub agent_name: Option<String>,
    pub error: Option<String>,
}

/// Initialize the database and store it in Tauri's managed state
pub fn init(app: &AppHandle) -> Result<()> {
    let db_path = get_db_path(app)?;
    let conn = Connection::open(&db_path)
        .with_context(|| format!("Failed to open database at {}", db_path.display()))?;

    migrations::run(&conn)?;

    app.manage(Database {
        conn: Mutex::new(conn),
    });

    Ok(())
}

fn get_db_path(app: &AppHandle) -> Result<PathBuf> {
    let app_data = app
        .path()
        .app_data_dir()
        .context("Failed to resolve app data directory")?;
    std::fs::create_dir_all(&app_data)?;
    Ok(app_data.join("whisperi.db"))
}

impl Database {
    pub fn save_transcription(
        &self,
        original_text: &str,
        processed_text: Option<&str>,
        processing_method: &str,
        agent_name: Option<&str>,
        error: Option<&str>,
    ) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO transcriptions (original_text, processed_text, is_processed, processing_method, agent_name, error)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                original_text,
                processed_text,
                processed_text.is_some(),
                processing_method,
                agent_name,
                error,
            ],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn get_transcriptions(&self, limit: u32, offset: u32) -> Result<Vec<Transcription>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, timestamp, original_text, processed_text, is_processed, processing_method, agent_name, error
             FROM transcriptions ORDER BY id DESC LIMIT ?1 OFFSET ?2",
        )?;

        let rows = stmt.query_map(rusqlite::params![limit, offset], |row| {
            Ok(Transcription {
                id: row.get(0)?,
                timestamp: row.get(1)?,
                original_text: row.get(2)?,
                processed_text: row.get(3)?,
                is_processed: row.get(4)?,
                processing_method: row.get(5)?,
                agent_name: row.get(6)?,
                error: row.get(7)?,
            })
        })?;

        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn delete_transcription(&self, id: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM transcriptions WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn clear_transcriptions(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM transcriptions", [])?;
        Ok(())
    }
}
