mod audio;
mod clipboard;
mod commands;
mod database;
mod models;
mod reasoning;
mod transcription;

use tauri::Manager;
use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::TrayIconBuilder;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Info)
                .level_for("tao", log::LevelFilter::Error)
                .build(),
        )
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // Focus main window when a second instance is launched
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_focus();
            }
        }))
        .plugin(
            tauri_plugin_window_state::Builder::new()
                .with_denylist(&["main"])
                .build(),
        )
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

            // System tray
            let show = MenuItemBuilder::with_id("show", "Show Whisperi").build(app)?;
            let settings = MenuItemBuilder::with_id("settings", "Settings").build(app)?;
            let quit = MenuItemBuilder::with_id("quit", "Quit").build(app)?;

            let menu = MenuBuilder::new(app)
                .item(&show)
                .separator()
                .item(&settings)
                .separator()
                .item(&quit)
                .build()?;

            TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip("Whisperi")
                .menu(&menu)
                .on_menu_event(move |app, event| {
                    match event.id().as_ref() {
                        "show" => {
                            if let Some(w) = app.get_webview_window("main") {
                                let _ = w.show();
                                let _ = w.set_focus();
                            }
                        }
                        "settings" => {
                            if let Some(w) = app.get_webview_window("settings") {
                                let _ = w.show();
                                let _ = w.set_focus();
                            }
                        }
                        "quit" => {
                            app.exit(0);
                        }
                        _ => {}
                    }
                })
                .build(app)?;

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
            commands::clipboard::paste_text,
            commands::clipboard::read_clipboard,
            commands::database::save_transcription,
            commands::database::get_transcriptions,
            commands::database::delete_transcription,
            commands::database::clear_transcriptions,
        ])
        .run(tauri::generate_context!())
        .expect("error while running whisperi");
}
