mod audio;
mod clipboard;
mod commands;
mod database;
mod models;
mod reasoning;
mod transcription;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_log::Builder::new().build())
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // Focus main window when a second instance is launched
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_window_state::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // Initialize audio recording state
            app.manage(audio::RecordingState::new());

            // Initialize database
            let app_handle = app.handle().clone();
            database::init(&app_handle)?;

            // Settings window starts hidden — opened via tray
            if let Some(settings_window) = app.get_webview_window("settings") {
                let _ = settings_window.hide();
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::audio::list_audio_devices,
            commands::audio::start_recording,
            commands::audio::stop_recording,
            commands::audio::get_audio_level,
            commands::transcription::transcribe_local,
            commands::transcription::transcribe_cloud,
            commands::transcription::list_whisper_models,
            commands::transcription::download_whisper_model,
            commands::transcription::delete_whisper_model,
            commands::transcription::get_whisper_status,
            commands::reasoning::process_reasoning,
            commands::settings::get_setting,
            commands::settings::set_setting,
            commands::settings::get_all_settings,
            commands::models::get_model_registry,
        ])
        .run(tauri::generate_context!())
        .expect("error while running whisperi");
}
