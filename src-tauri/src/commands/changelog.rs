use super::ResultExt;

// Resources bundled with a `..` path land under `_up_/` in the installed app
// (NSIS on Windows, .app on macOS, AppImage on Linux). `path().resolve()`
// with `BaseDirectory::Resource` reverses that transformation transparently,
// so we pass the original path from `bundle.resources` in tauri.conf.json.
#[tauri::command]
pub fn read_changelog(app: tauri::AppHandle) -> Result<String, String> {
    use tauri::Manager;
    use tauri::path::BaseDirectory;

    let resource_path = app
        .path()
        .resolve("../docs/CHANGELOG.md", BaseDirectory::Resource)
        .str_err()?;

    std::fs::read_to_string(&resource_path).or_else(|primary_err| {
        // Dev fallback: `tauri dev` doesn't stage bundle resources under
        // target/debug/, so resolve() points at a non-existent path.
        let dev_path =
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../docs/CHANGELOG.md");
        std::fs::read_to_string(&dev_path).map_err(|dev_err| {
            format!(
                "Failed to read changelog. Resource path {} failed ({}); dev fallback {} failed ({})",
                resource_path.display(),
                primary_err,
                dev_path.display(),
                dev_err
            )
        })
    })
}
