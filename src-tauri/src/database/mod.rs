pub mod migrations;
pub mod word_count;

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

#[derive(Debug, Clone, Copy)]
pub enum StatsPeriod {
    Today,
    Week,
    All,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatsPayload {
    pub total_seconds: i64,
    pub total_words: i64,
    pub total_recordings: i64,
    pub avg_seconds: f64,
    pub avg_words: f64,
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
    /// Create an in-memory database with migrations applied. Test-only.
    #[cfg(test)]
    fn new_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        migrations::run(&conn)?;
        Ok(Database {
            conn: Mutex::new(conn),
        })
    }

    pub fn save_transcription(
        &self,
        original_text: &str,
        processed_text: Option<&str>,
        processing_method: &str,
        agent_name: Option<&str>,
        error: Option<&str>,
        duration_ms: Option<i64>,
    ) -> Result<i64> {
        // Count words on the final user-visible text — processed_text when AI
        // enhancement is on, otherwise the raw transcription.
        let counted_text = processed_text.unwrap_or(original_text);
        let word_count = word_count::count_words(counted_text);

        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO transcriptions
               (original_text, processed_text, is_processed, processing_method,
                agent_name, error, duration_ms, word_count)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                original_text,
                processed_text,
                processed_text.is_some(),
                processing_method,
                agent_name,
                error,
                duration_ms,
                word_count,
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

    pub fn get_stats(&self, period: StatsPeriod) -> Result<StatsPayload> {
        // All queries:
        //   - filter `duration_ms IS NOT NULL` so pre-feature rows (NULL after
        //     the v2 migration on an existing DB) are excluded from both totals.
        //   - use SQLite's 'localtime' modifier on `timestamp` so "today" and
        //     "last 7 days" reflect the user's local timezone, not UTC.
        // Each is one indexed scan + SUM; cheap.
        let (sum_ms, sum_words, total_recordings): (i64, i64, i64) = {
            let conn = self.conn.lock().unwrap();
            match period {
                StatsPeriod::Today => conn.query_row(
                    "SELECT COALESCE(SUM(duration_ms), 0),
                            COALESCE(SUM(word_count), 0),
                            COUNT(*)
                     FROM transcriptions
                     WHERE duration_ms IS NOT NULL
                       AND datetime(timestamp, 'localtime') >= date('now', 'localtime')",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )?,
                StatsPeriod::Week => conn.query_row(
                    "SELECT COALESCE(SUM(duration_ms), 0),
                            COALESCE(SUM(word_count), 0),
                            COUNT(*)
                     FROM transcriptions
                     WHERE duration_ms IS NOT NULL
                       AND datetime(timestamp, 'localtime') >= datetime('now', 'localtime', '-7 days')",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )?,
                StatsPeriod::All => conn.query_row(
                    "SELECT COALESCE(SUM(duration_ms), 0),
                            COALESCE(SUM(word_count), 0),
                            COUNT(*)
                     FROM transcriptions
                     WHERE duration_ms IS NOT NULL",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )?,
            }
        };

        let total_seconds = sum_ms / 1000;
        let avg_seconds = if total_recordings > 0 {
            (total_seconds as f64) / (total_recordings as f64)
        } else {
            0.0
        };
        let avg_words = if total_recordings > 0 {
            (sum_words as f64) / (total_recordings as f64)
        } else {
            0.0
        };

        Ok(StatsPayload {
            total_seconds,
            total_words: sum_words,
            total_recordings,
            avg_seconds,
            avg_words,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn insert_raw(
        db: &Database,
        original: &str,
        duration_ms: Option<i64>,
        word_count: Option<i64>,
        timestamp_sql: &str,
    ) {
        let conn = db.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO transcriptions (timestamp, original_text, duration_ms, word_count)
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![timestamp_sql, original, duration_ms, word_count],
        )
        .unwrap();
    }

    #[test]
    fn save_transcription_persists_word_count_and_duration() {
        let db = Database::new_in_memory().unwrap();
        let id = db
            .save_transcription("hello world", None, "none", None, None, Some(2500))
            .unwrap();
        assert!(id > 0);

        let conn = db.conn.lock().unwrap();
        let (dur, wc): (Option<i64>, Option<i64>) = conn
            .query_row(
                "SELECT duration_ms, word_count FROM transcriptions WHERE id = ?1",
                [id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(dur, Some(2500));
        assert_eq!(wc, Some(2));
    }

    #[test]
    fn save_transcription_counts_processed_text_when_present() {
        let db = Database::new_in_memory().unwrap();
        let id = db
            .save_transcription(
                "um hello",
                Some("hello world"),
                "ai",
                None,
                None,
                Some(1000),
            )
            .unwrap();
        let conn = db.conn.lock().unwrap();
        let wc: i64 = conn
            .query_row(
                "SELECT word_count FROM transcriptions WHERE id = ?1",
                [id],
                |row| row.get(0),
            )
            .unwrap();
        // Counts processed_text ("hello world") = 2, not original ("um hello") = 2.
        // Same value here by coincidence — assert nonzero to prove it ran.
        assert_eq!(wc, 2);
    }

    #[test]
    fn get_stats_all_excludes_null_duration_rows() {
        let db = Database::new_in_memory().unwrap();
        // Pre-feature row (no duration recorded).
        insert_raw(&db, "old", None, None, "2026-01-01 12:00:00");
        // Two new-feature rows.
        insert_raw(&db, "new1", Some(2000), Some(3), "2026-05-19 09:00:00");
        insert_raw(&db, "new2", Some(4000), Some(5), "2026-05-19 09:01:00");

        let s = db.get_stats(StatsPeriod::All).unwrap();
        assert_eq!(s.total_recordings, 2);
        assert_eq!(s.total_seconds, 6);
        assert_eq!(s.total_words, 8);
        assert!((s.avg_seconds - 3.0).abs() < 1e-9);
        assert!((s.avg_words - 4.0).abs() < 1e-9);
    }

    #[test]
    fn get_stats_empty_db_returns_zero_averages() {
        let db = Database::new_in_memory().unwrap();
        let s = db.get_stats(StatsPeriod::All).unwrap();
        assert_eq!(s.total_recordings, 0);
        assert_eq!(s.total_seconds, 0);
        assert_eq!(s.total_words, 0);
        assert_eq!(s.avg_seconds, 0.0);
        assert_eq!(s.avg_words, 0.0);
    }

    #[test]
    fn get_stats_today_uses_localtime_midnight() {
        let db = Database::new_in_memory().unwrap();
        // datetime('now') is UTC; we just want a row that is unambiguously "today" locally.
        let conn = db.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO transcriptions (timestamp, original_text, duration_ms, word_count)
             VALUES (datetime('now'), 'just now', 1500, 2)",
            [],
        )
        .unwrap();
        // Old row from years ago.
        conn.execute(
            "INSERT INTO transcriptions (timestamp, original_text, duration_ms, word_count)
             VALUES ('2020-01-01 12:00:00', 'long ago', 9999, 999)",
            [],
        )
        .unwrap();
        drop(conn);

        let today = db.get_stats(StatsPeriod::Today).unwrap();
        assert_eq!(today.total_recordings, 1);
        assert_eq!(today.total_seconds, 1);
        assert_eq!(today.total_words, 2);
    }

    #[test]
    fn get_stats_week_excludes_rows_older_than_seven_days() {
        let db = Database::new_in_memory().unwrap();
        let conn = db.conn.lock().unwrap();
        // 3 days ago — should be included.
        conn.execute(
            "INSERT INTO transcriptions (timestamp, original_text, duration_ms, word_count)
             VALUES (datetime('now', '-3 days'), 'recent', 2000, 4)",
            [],
        )
        .unwrap();
        // 30 days ago — should be excluded.
        conn.execute(
            "INSERT INTO transcriptions (timestamp, original_text, duration_ms, word_count)
             VALUES (datetime('now', '-30 days'), 'old', 9000, 100)",
            [],
        )
        .unwrap();
        drop(conn);

        let week = db.get_stats(StatsPeriod::Week).unwrap();
        assert_eq!(week.total_recordings, 1);
        assert_eq!(week.total_seconds, 2);
        assert_eq!(week.total_words, 4);
    }
}
