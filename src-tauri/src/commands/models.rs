use serde_json::Value;

#[tauri::command]
pub fn get_model_registry() -> Result<Value, String> {
    // Load the model registry JSON that's embedded from the frontend
    // This will be populated with the actual registry data in Phase 6
    let registry = serde_json::json!({
        "whisperModels": {},
        "cloudProviders": [],
        "transcriptionProviders": []
    });

    Ok(registry)
}
