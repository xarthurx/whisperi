use super::ResultExt;
use serde_json::Value;
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

const STORE_FILE: &str = "settings.json";

#[tauri::command]
pub fn get_setting(app: AppHandle, key: String) -> Result<Option<Value>, String> {
    let store = app.store(STORE_FILE).str_err()?;
    Ok(store.get(&key))
}

#[tauri::command]
pub fn set_setting(app: AppHandle, key: String, value: Value) -> Result<(), String> {
    let store = app.store(STORE_FILE).str_err()?;
    store.set(&key, value);
    Ok(())
}

#[tauri::command]
pub fn get_all_settings(app: AppHandle) -> Result<Value, String> {
    let store = app.store(STORE_FILE).str_err()?;
    let map: serde_json::Map<String, Value> = store
        .keys()
        .into_iter()
        .filter_map(|key| store.get(&key).map(|val| (key, val)))
        .collect();
    Ok(Value::Object(map))
}
