use super::ResultExt;
use crate::database::{Database, StatsPayload, StatsPeriod, Transcription};
use tauri::State;

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn save_transcription(
    db: State<'_, Database>,
    original_text: String,
    processed_text: Option<String>,
    processing_method: String,
    agent_name: Option<String>,
    error: Option<String>,
    duration_ms: Option<i64>,
) -> Result<i64, String> {
    db.save_transcription(
        &original_text,
        processed_text.as_deref(),
        &processing_method,
        agent_name.as_deref(),
        error.as_deref(),
        duration_ms,
    )
    .str_err()
}

#[tauri::command]
pub fn get_transcriptions(
    db: State<'_, Database>,
    limit: u32,
    offset: u32,
) -> Result<Vec<Transcription>, String> {
    db.get_transcriptions(limit, offset).str_err()
}

#[tauri::command]
pub fn delete_transcription(db: State<'_, Database>, id: i64) -> Result<(), String> {
    db.delete_transcription(id).str_err()
}

#[tauri::command]
pub fn clear_transcriptions(db: State<'_, Database>) -> Result<(), String> {
    db.clear_transcriptions().str_err()
}

#[tauri::command]
pub fn get_stats(db: State<'_, Database>, period: String) -> Result<StatsPayload, String> {
    let p = match period.as_str() {
        "today" => StatsPeriod::Today,
        "week" => StatsPeriod::Week,
        "all" => StatsPeriod::All,
        other => return Err(format!("unknown stats period: {other}")),
    };
    db.get_stats(p).str_err()
}
