//! Tauri commands for Live dictation mode.

use serde::Serialize;

use crate::clipboard::{
    ClipError, SwapResult, current_foreground_hwnd, current_foreground_window_class,
    send_text_keystrokes, swap_typed_text,
};

#[derive(Debug, Serialize)]
#[serde(tag = "kind", content = "data")]
pub enum TypeChunkResult {
    Typed(usize),
    SkippedTerminalFocus,
}

#[tauri::command]
pub async fn type_text_chunk(text: String) -> Result<TypeChunkResult, String> {
    match send_text_keystrokes(&text) {
        Ok(n) => Ok(TypeChunkResult::Typed(n)),
        Err(ClipError::TerminalFocusGuard) => Ok(TypeChunkResult::SkippedTerminalFocus),
    }
}

#[tauri::command]
pub async fn swap_typed_text_cmd(
    backspace_count: usize,
    new_text: String,
    expected_hwnd: Option<isize>,
) -> Result<SwapResult, String> {
    swap_typed_text(backspace_count, &new_text, expected_hwnd).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_foreground_window() -> Result<isize, String> {
    Ok(current_foreground_hwnd())
}

#[tauri::command]
pub fn get_foreground_window_class() -> Result<Option<String>, String> {
    Ok(current_foreground_window_class())
}
