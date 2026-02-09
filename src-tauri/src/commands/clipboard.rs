#[tauri::command]
pub fn paste_text(text: String) -> Result<(), String> {
    crate::clipboard::paste_text(&text).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn read_clipboard() -> Result<String, String> {
    crate::clipboard::read_clipboard().map_err(|e| e.to_string())
}
