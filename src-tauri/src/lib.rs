mod audio;
mod clipboard;
mod commands;
mod database;
mod models;
mod reasoning;
mod transcription;

pub(crate) static HTTP_CLIENT: std::sync::LazyLock<reqwest::Client> =
    std::sync::LazyLock::new(|| {
        reqwest::Client::builder()
            .user_agent("Whisperi")
            .build()
            .expect("Failed to build HTTP client")
    });

use tauri::Manager;
use tauri::menu::{CheckMenuItemBuilder, MenuBuilder, MenuItemBuilder};
use tauri::tray::TrayIconBuilder;

/// Override the Windows minimum window size constraint for a given window.
/// Windows enforces a minimum width (~136px at 100% DPI) even for undecorated windows.
/// This subclasses the window proc to intercept WM_GETMINMAXINFO and set our own minimum.
/// Automatically adjusts for DPI changes.
#[cfg(windows)]
fn override_min_window_size(window: &tauri::WebviewWindow, logical_w: i32, logical_h: i32) {
    use std::sync::atomic::{AtomicI32, AtomicIsize, Ordering};
    use windows::Win32::Foundation::*;
    use windows::Win32::UI::HiDpi::GetDpiForWindow;
    use windows::Win32::UI::WindowsAndMessaging::*;

    static OLD_WNDPROC: AtomicIsize = AtomicIsize::new(0);
    static MIN_W: AtomicI32 = AtomicI32::new(0);
    static MIN_H: AtomicI32 = AtomicI32::new(0);

    MIN_W.store(logical_w, Ordering::Relaxed);
    MIN_H.store(logical_h, Ordering::Relaxed);

    unsafe extern "system" fn wnd_proc(
        hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM,
    ) -> LRESULT {
        if msg == WM_GETMINMAXINFO && lparam.0 != 0 {
            let info = &mut *(lparam.0 as *mut MINMAXINFO);
            let dpi = unsafe { GetDpiForWindow(hwnd) };
            let scale = dpi as f64 / 96.0;
            info.ptMinTrackSize.x = (MIN_W.load(Ordering::Relaxed) as f64 * scale) as i32;
            info.ptMinTrackSize.y = (MIN_H.load(Ordering::Relaxed) as f64 * scale) as i32;
            return LRESULT(0);
        }
        let old: WNDPROC = unsafe { std::mem::transmute(OLD_WNDPROC.load(Ordering::Relaxed)) };
        unsafe { CallWindowProcW(old, hwnd, msg, wparam, lparam) }
    }

    let hwnd = HWND(window.hwnd().unwrap().0 as *mut _);
    unsafe {
        let old = SetWindowLongPtrW(hwnd, GWLP_WNDPROC, wnd_proc as *const () as isize);
        OLD_WNDPROC.store(old, Ordering::Relaxed);
    }
}

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
        .plugin(tauri_plugin_window_state::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .setup(|app| {
            // Initialize audio recording state
            app.manage(audio::RecordingState::new());

            // Initialize database
            let app_handle = app.handle().clone();
            database::init(&app_handle)?;

            // Override Windows minimum window size for the overlay
            #[cfg(windows)]
            if let Some(main_window) = app.get_webview_window("main") {
                override_min_window_size(&main_window, 100, 100);
            }

            // Settings window starts hidden — opened via tray
            if let Some(settings_window) = app.get_webview_window("settings") {
                let _ = settings_window.hide();
            }

            // System tray
            let show = CheckMenuItemBuilder::with_id("show", "Show Whisperi")
                .checked(true)
                .build(app)?;
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
                            if let Some(window) = app.get_webview_window("main") {
                                let visible = window.is_visible().unwrap_or(false);
                                if visible {
                                    let _ = window.hide();
                                    let _ = show.set_checked(false);
                                } else {
                                    let _ = window.show();
                                    let _ = window.set_focus();
                                    let _ = show.set_checked(true);
                                }
                            }
                        }
                        "settings" => {
                            if let Some(window) = app.get_webview_window("settings") {
                                let _ = window.show();
                                let _ = window.set_focus();
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
            commands::app::quit_app,
            commands::app::show_settings,
        ])
        .run(tauri::generate_context!())
        .expect("error while running whisperi");
}
