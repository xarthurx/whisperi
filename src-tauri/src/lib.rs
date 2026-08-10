mod audio;
mod clipboard;
mod commands;
mod database;
pub(crate) mod http;
mod reasoning;
pub mod transcription;

use tauri::Manager;
use tauri::menu::{CheckMenuItemBuilder, MenuBuilder, MenuItemBuilder};
use tauri::tray::TrayIconBuilder;
use tauri_plugin_window_state::{StateFlags, WindowExt};

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

    /// Force DWM to recreate the transparent layered window surface.
    /// A hide+show cycle is needed because SetWindowPos alone doesn't
    /// make WebView2 repaint on a stale compositor surface.
    unsafe fn refresh_transparent_overlay(hwnd: HWND) {
        if !unsafe { IsWindowVisible(hwnd) }.as_bool() {
            return;
        }
        unsafe {
            let _ = ShowWindow(hwnd, SW_HIDE);
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            let _ = SetWindowPos(
                hwnd,
                HWND_TOPMOST,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
    }

    unsafe extern "system" fn wnd_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if msg == WM_GETMINMAXINFO && lparam.0 != 0 {
            let info = unsafe { &mut *(lparam.0 as *mut MINMAXINFO) };
            let dpi = unsafe { GetDpiForWindow(hwnd) };
            let scale = dpi as f64 / 96.0;
            info.ptMinTrackSize.x = (MIN_W.load(Ordering::Relaxed) as f64 * scale) as i32;
            info.ptMinTrackSize.y = (MIN_H.load(Ordering::Relaxed) as f64 * scale) as i32;
            return LRESULT(0);
        }

        // After sleep/wake the compositor drops the transparent window surface.
        const PBT_APMRESUMEAUTOMATIC: usize = 0x0012;
        if msg == WM_POWERBROADCAST && wparam.0 == PBT_APMRESUMEAUTOMATIC {
            unsafe { refresh_transparent_overlay(hwnd) };
        }

        // Monitor turned back on after power-saving blanked the display.
        if msg == WM_SYSCOMMAND && (wparam.0 & 0xFFF0) == SC_MONITORPOWER as usize && lparam.0 == -1
        {
            unsafe { refresh_transparent_overlay(hwnd) };
        }

        // Session unlock (Win+L → log back in) or remote reconnect.
        const WM_WTSSESSION_CHANGE: u32 = 0x02B1;
        const WTS_SESSION_UNLOCK: usize = 0x8;
        if msg == WM_WTSSESSION_CHANGE && wparam.0 == WTS_SESSION_UNLOCK {
            unsafe { refresh_transparent_overlay(hwnd) };
        }

        // Display settings changed (resolution, monitor connected/disconnected).
        if msg == WM_DISPLAYCHANGE {
            unsafe { refresh_transparent_overlay(hwnd) };
        }

        let old: WNDPROC = unsafe { std::mem::transmute(OLD_WNDPROC.load(Ordering::Relaxed)) };
        unsafe { CallWindowProcW(old, hwnd, msg, wparam, lparam) }
    }

    let hwnd = HWND(window.hwnd().unwrap().0 as *mut _);
    unsafe {
        let old = SetWindowLongPtrW(hwnd, GWLP_WNDPROC, wnd_proc as *const () as isize);
        OLD_WNDPROC.store(old, Ordering::Relaxed);
    }

    // Register for session change notifications (lock/unlock, remote connect).
    {
        use windows::Win32::System::RemoteDesktop::*;
        let _ = unsafe { WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION) };
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Force ANSI colors even when stdout is piped (bun → cargo → app).
    colored::control::set_override(true);

    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Info)
                .level_for("tao", log::LevelFilter::Error)
                .with_colors(
                    tauri_plugin_log::fern::colors::ColoredLevelConfig::default()
                        .info(tauri_plugin_log::fern::colors::Color::Green),
                )
                .build(),
        )
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // Focus main window when a second instance is launched
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(
            tauri_plugin_window_state::Builder::new()
                // The overlay is fixed-size. Restoring its persisted physical size can
                // compound DPI scaling when it moves between monitors, so setup restores
                // only its position below. Other windows keep the default full restore.
                .skip_initial_state("main")
                .build(),
        )
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

            // Initialize live dictation session state. Wrap in Arc so the
            // audio-pump task spawned in start_live_session can clone a handle
            // for self-cleanup on exit (preventing HashMap leaks when a WS
            // dies without an explicit stop/cancel command).
            app.manage(std::sync::Arc::new(
                crate::transcription::streaming::LiveSessionState::default(),
            ));

            // Initialize database
            let app_handle = app.handle().clone();
            database::init(&app_handle)?;

            // Restore the overlay location without restoring its fixed size.
            if let Some(main_window) = app.get_webview_window("main") {
                // Keep the user's chosen monitor/location without allowing a stale or
                // mixed-DPI window-state entry to override the configured 100x100 size.
                let _ = main_window.restore_state(StateFlags::POSITION);
                #[cfg(windows)]
                {
                    // Override Windows' minimum window size for the overlay.
                    override_min_window_size(&main_window, 100, 100);
                    let _ = main_window.show();
                }
            }

            // Settings window starts hidden — opened via tray
            if let Some(settings_window) = app.get_webview_window("settings") {
                let _ = settings_window.hide();
            }

            // System tray
            let show = CheckMenuItemBuilder::with_id("show", "Show")
                .checked(true)
                .build(app)?;
            let settings = MenuItemBuilder::with_id("settings", "Preferences").build(app)?;
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
                .on_menu_event(move |app, event| match event.id().as_ref() {
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
                })
                .build(app)?;

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::audio::list_audio_devices,
            commands::audio::start_recording,
            commands::audio::stop_recording,
            commands::audio::get_audio_level,
            commands::transcription::transcribe_cloud,
            commands::reasoning::process_reasoning,
            commands::settings::get_setting,
            commands::settings::set_setting,
            commands::settings::get_all_settings,
            commands::clipboard::paste_text,
            commands::clipboard::read_clipboard,
            commands::clipboard::set_clipboard_text,
            commands::database::save_transcription,
            commands::database::get_transcriptions,
            commands::database::delete_transcription,
            commands::database::clear_transcriptions,
            commands::database::get_stats,
            commands::app::quit_app,
            commands::app::show_settings,
            commands::changelog::read_changelog,
            commands::live::start_live_session,
            commands::live::stop_live_session,
            commands::live::cancel_live_session,
            commands::live::type_text_chunk,
            commands::live::swap_typed_text_cmd,
            commands::live::get_foreground_window,
            commands::live::get_foreground_window_class,
            commands::live::get_focus_target,
        ])
        .build(tauri::generate_context!())
        .expect("error while building whisperi")
        .run(|app_handle, event| {
            // On quit (tray "Quit", the quit_app command, or any exit request),
            // flush an active Live session before the process exits: signal
            // cancel and await the soft-flush so the provider WebSocket closes
            // cleanly and the final utterance isn't lost to a detached task.
            // Bounded so quit never hangs; a no-op when no session is active.
            // `app.exit()` routes through the event loop and emits
            // ExitRequested (it does not bypass to process::exit unless the
            // request itself fails), so this handler reliably runs on quit.
            if let tauri::RunEvent::ExitRequested { .. } = event {
                let sessions = app_handle
                    .state::<std::sync::Arc<crate::transcription::streaming::LiveSessionState>>()
                    .inner()
                    .clone();
                tauri::async_runtime::block_on(
                    sessions.shutdown(std::time::Duration::from_millis(1500)),
                );
            }
        });
}
