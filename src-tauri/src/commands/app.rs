use super::ResultExt;

#[tauri::command]
pub fn quit_app(app: tauri::AppHandle) {
    app.exit(0);
}

#[tauri::command]
pub fn show_settings(app: tauri::AppHandle) -> Result<(), String> {
    use tauri::Manager;
    let window = app
        .get_webview_window("settings")
        .ok_or("Settings window not found")?;
    window.show().str_err()?;
    window.set_focus().str_err()?;
    Ok(())
}
