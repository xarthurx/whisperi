use crate::database::{Database, Transcription};
use tauri::State;

#[tauri::command]
pub fn save_transcription(
    db: State<'_, Database>,
    original_text: String,
    processed_text: Option<String>,
    processing_method: String,
    agent_name: Option<String>,
    error: Option<String>,
) -> Result<i64, String> {
    db.save_transcription(
        &original_text,
        processed_text.as_deref(),
        &processing_method,
        agent_name.as_deref(),
        error.as_deref(),
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_transcriptions(
    db: State<'_, Database>,
    limit: u32,
    offset: u32,
) -> Result<Vec<Transcription>, String> {
    db.get_transcriptions(limit, offset)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_transcription(db: State<'_, Database>, id: i64) -> Result<(), String> {
    db.delete_transcription(id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn clear_transcriptions(db: State<'_, Database>) -> Result<(), String> {
    db.clear_transcriptions().map_err(|e| e.to_string())
}
