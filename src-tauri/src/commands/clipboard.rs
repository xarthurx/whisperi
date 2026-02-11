use super::ResultExt;

#[tauri::command]
pub fn paste_text(text: String) -> Result<(), String> {
    crate::clipboard::paste_text(&text).str_err()
}

#[tauri::command]
pub fn read_clipboard() -> Result<String, String> {
    crate::clipboard::read_clipboard().str_err()
}
