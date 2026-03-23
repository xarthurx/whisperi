use super::ResultExt;

#[tauri::command]
pub fn read_changelog(app: tauri::AppHandle) -> Result<String, String> {
    use tauri::Manager;
    let resource_path = app
        .path()
        .resource_dir()
        .str_err()?
        .join("CHANGELOG.md");
    std::fs::read_to_string(&resource_path).or_else(|_| {
        // Dev fallback: bundle resources aren't copied to target/debug/
        let dev_path =
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../docs/CHANGELOG.md");
        std::fs::read_to_string(&dev_path)
            .map_err(|e| format!("Failed to read changelog: {} (path: {})", e, dev_path.display()))
    })
}
