use super::ResultExt;

#[tauri::command]
pub fn read_changelog(app: tauri::AppHandle) -> Result<String, String> {
    use tauri::Manager;
    let resource_path = app
        .path()
        .resource_dir()
        .str_err()?
        .join("CHANGELOG.md");
    std::fs::read_to_string(&resource_path)
        .map_err(|e| format!("Failed to read changelog: {} (path: {})", e, resource_path.display()))
}
