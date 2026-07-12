use anyhow::Result;

/// Result of a [`swap_typed_text`] call.
#[derive(Debug, serde::Serialize)]
pub enum SwapResult {
    /// Backspaces were sent and new text was typed.
    Swapped,
    /// Foreground HWND no longer matches `expected_hwnd`; nothing was sent.
    SkippedFocusDrift,
    /// Nothing to do: zero backspaces and empty new text.
    SkippedNoChange,
}

/// Return the current foreground window HWND as a portable isize.
/// Caller passes this back when starting a Live session so the swap can
/// verify the user hasn't switched windows mid-dictation.
pub fn current_foreground_hwnd() -> isize {
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
        unsafe { GetForegroundWindow().0 as isize }
    }

    #[cfg(not(target_os = "windows"))]
    {
        0
    }
}

/// Class name of an arbitrary window (`None` for an invalid handle). Shared by
/// the notification label and the focus classifier.
#[cfg(target_os = "windows")]
fn window_class(hwnd: windows::Win32::Foundation::HWND) -> Option<String> {
    use windows::Win32::UI::WindowsAndMessaging::GetClassNameW;
    if hwnd.is_invalid() {
        return None;
    }
    let mut buf = [0u16; 256];
    let len = unsafe { GetClassNameW(hwnd, &mut buf) };
    if len <= 0 {
        return None;
    }
    Some(String::from_utf16_lossy(&buf[..len as usize]))
}

/// Return the class name of the current foreground window (for UI display
/// in the OS notification: "Live: typing into Notepad").
pub fn current_foreground_window_class() -> Option<String> {
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
        window_class(unsafe { GetForegroundWindow() })
    }

    #[cfg(not(target_os = "windows"))]
    {
        None
    }
}

/// Window classes whose "focused control" is a single shared render surface
/// hiding many independent text boxes (Chromium/Electron and Gecko). On these
/// we cannot tell one text box from another by HWND, so a scoped destructive
/// backspace swap is unsafe — callers fall back to the clipboard instead.
const WEB_RENDER_CLASSES: &[&str] = &[
    "Chrome_RenderWidgetHostHWND", // Chrome/Edge/Brave/Opera/Electron/CEF content
    "Chrome_WidgetWin_1",          // Chromium top-level
    "Chrome_WidgetWin_0",
    "Intermediate D3D Window", // Chromium GPU/compositor surface
    "MozillaWindowClass",      // Firefox
    "MozillaContentWindowClass",
    "MozillaCompositorWindowClass",
];

/// True if `class` is a known web/Electron render surface (see
/// [`WEB_RENDER_CLASSES`]).
fn is_web_render_class(class: &str) -> bool {
    WEB_RENDER_CLASSES
        .iter()
        .any(|c| class.eq_ignore_ascii_case(c))
}

/// The window + focused control the next keystrokes would land in. `control`
/// falls back to `window` when no distinct focused control is exposed.
/// `scopable` is true only when `control` is a real, distinguishable control a
/// backspace swap can be safely scoped to — false for web/Electron render
/// surfaces (many text boxes behind one HWND; see [`is_web_render_class`]).
#[derive(Debug, Clone, serde::Serialize)]
pub struct FocusTarget {
    pub window: isize,
    pub control: isize,
    pub scopable: bool,
}

/// Resolve the current [`FocusTarget`]. Read immediately before typing (to tag
/// each chunk with the box it lands in) and again at swap time (to confirm
/// focus hasn't drifted to a different box/window).
pub fn current_focus_target() -> FocusTarget {
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::UI::WindowsAndMessaging::{
            GUITHREADINFO, GetForegroundWindow, GetGUIThreadInfo, GetWindowThreadProcessId,
        };
        let fg = unsafe { GetForegroundWindow() };
        if fg.is_invalid() {
            return FocusTarget {
                window: 0,
                control: 0,
                scopable: false,
            };
        }
        let window = fg.0 as isize;
        // hwndFocus is the control with keyboard focus in the foreground thread.
        // For native controls this is the actual edit box; for Chromium/Gecko it
        // is the shared render surface (one HWND for every field on the page).
        let tid = unsafe { GetWindowThreadProcessId(fg, None) };
        let mut gti: GUITHREADINFO = unsafe { std::mem::zeroed() };
        gti.cbSize = std::mem::size_of::<GUITHREADINFO>() as u32;
        let control_hwnd = if tid != 0
            && unsafe { GetGUIThreadInfo(tid, &mut gti) }.is_ok()
            && !gti.hwndFocus.is_invalid()
        {
            gti.hwndFocus
        } else {
            fg
        };
        let control = control_hwnd.0 as isize;
        // Classify by the control's class first (most specific), then the window's.
        let class = window_class(control_hwnd).or_else(|| window_class(fg));
        let scopable = control != 0 && !class.as_deref().is_some_and(is_web_render_class);
        FocusTarget {
            window,
            control,
            scopable,
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        FocusTarget {
            window: 0,
            control: 0,
            scopable: false,
        }
    }
}

/// Window classes that must NEVER receive simulated keystrokes — credential
/// prompts and the lock screen. Live mode types each utterance into whatever
/// window currently has focus ("type where you look"), so if focus lands on one
/// of these while dictating, the transcript (attacker-influenceable audio) must
/// not be injected into it. Best-effort: this catches dedicated secure
/// top-level windows, NOT in-app password fields (child controls with no
/// distinguishing top-level class). Pairs with the deferred secure-window
/// auto-pause work.
const SECURE_WINDOW_CLASSES: &[&str] = &[
    "Credential Dialog Xaml Host", // Windows credential / UAC-style credential prompt
    "LockScreenControlsWindow",    // Windows lock screen
];

/// True if `class` is a known secure/credential window class.
fn is_secure_window_class(class: &str) -> bool {
    SECURE_WINDOW_CLASSES
        .iter()
        .any(|c| class.eq_ignore_ascii_case(c))
}

/// True if the current foreground window is a secure/credential window that
/// must not receive simulated keystrokes.
#[cfg(target_os = "windows")]
fn is_secure_foreground_window() -> bool {
    current_foreground_window_class()
        .as_deref()
        .is_some_and(is_secure_window_class)
}

/// Write text to clipboard, simulate paste into the focused application,
/// then restore the original clipboard contents.
pub fn paste_text(text: &str) -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        // Set new text on clipboard (it stays there for the user)
        windows_clipboard::set_text(text)?;
        std::thread::sleep(std::time::Duration::from_millis(10));

        // Detect terminal and paste accordingly
        let is_terminal = windows_terminal::is_foreground_terminal();
        if is_terminal {
            windows_paste::send_ctrl_shift_v()?;
        } else {
            windows_paste::send_ctrl_v()?;
        }

        Ok(())
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = text;
        anyhow::bail!("Clipboard paste not yet implemented for this platform");
    }
}

/// Read the current clipboard text.
pub fn read_clipboard() -> Result<String> {
    #[cfg(target_os = "windows")]
    {
        windows_clipboard::get_text()
    }

    #[cfg(not(target_os = "windows"))]
    {
        anyhow::bail!("Clipboard read not yet implemented for this platform");
    }
}

/// Set the clipboard to `text` WITHOUT pasting. Used by the Live polish
/// fallback: when an in-place swap can't be done safely (web/Electron field, or
/// a dictation split across boxes), we leave the polished text on the clipboard
/// for the user to paste themselves.
pub fn set_clipboard_text(text: &str) -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        windows_clipboard::set_text(text)
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = text;
        anyhow::bail!("Clipboard write not yet implemented for this platform");
    }
}

#[cfg(target_os = "windows")]
mod windows_clipboard {
    use anyhow::Result;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, GetClipboardData, OpenClipboard, SetClipboardData,
    };
    use windows::Win32::System::Memory::{
        GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock,
    };

    const CF_UNICODETEXT: u32 = 13;

    pub fn set_text(text: &str) -> Result<()> {
        let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        let byte_len = wide.len() * 2;

        unsafe {
            let hmem = GlobalAlloc(GMEM_MOVEABLE, byte_len)?;
            let ptr = GlobalLock(hmem);
            if ptr.is_null() {
                anyhow::bail!("GlobalLock failed");
            }
            std::ptr::copy_nonoverlapping(wide.as_ptr() as *const u8, ptr as *mut u8, byte_len);
            let _ = GlobalUnlock(hmem);

            OpenClipboard(HWND::default())?;
            let _ = EmptyClipboard();
            SetClipboardData(
                CF_UNICODETEXT,
                windows::Win32::Foundation::HANDLE(hmem.0 as _),
            )?;
            CloseClipboard()?;
        }

        Ok(())
    }

    pub fn get_text() -> Result<String> {
        unsafe {
            OpenClipboard(HWND::default())?;

            let handle = GetClipboardData(CF_UNICODETEXT);
            if handle.is_err() {
                CloseClipboard()?;
                return Ok(String::new());
            }
            let handle = handle.unwrap();

            let hmem = windows::Win32::Foundation::HGLOBAL(handle.0 as _);
            let ptr = GlobalLock(hmem);
            if ptr.is_null() {
                CloseClipboard()?;
                return Ok(String::new());
            }

            let size = GlobalSize(hmem);
            let num_u16 = size / 2;
            let slice = std::slice::from_raw_parts(ptr as *const u16, num_u16);

            // Find null terminator
            let len = slice.iter().position(|&c| c == 0).unwrap_or(num_u16);
            let text = String::from_utf16_lossy(&slice[..len]);

            let _ = GlobalUnlock(hmem);
            CloseClipboard()?;

            Ok(text)
        }
    }
}

#[cfg(target_os = "windows")]
mod windows_terminal {
    use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

    /// Known terminal window class names on Windows.
    const TERMINAL_CLASSES: &[&str] = &[
        "ConsoleWindowClass",            // cmd.exe, legacy console
        "CASCADIA_HOSTING_WINDOW_CLASS", // Windows Terminal
        "mintty",                        // Git Bash, MSYS2, Cygwin
        "VirtualConsoleClass",           // ConEmu
        "PuTTY",                         // PuTTY
        "Alacritty",                     // Alacritty
        "org.wezfurlong.wezterm",        // WezTerm
        "Hyper",                         // Hyper terminal
        "TMobaXterm",                    // MobaXterm
    ];

    pub fn is_foreground_terminal() -> bool {
        unsafe {
            let hwnd = GetForegroundWindow();
            if hwnd.is_invalid() {
                return false;
            }

            let mut class_name = [0u8; 256];
            let len = windows::Win32::UI::WindowsAndMessaging::GetClassNameA(hwnd, &mut class_name);
            if len == 0 {
                return false;
            }

            let class_str = std::str::from_utf8(&class_name[..len as usize]).unwrap_or("");

            TERMINAL_CLASSES
                .iter()
                .any(|tc| class_str.eq_ignore_ascii_case(tc))
        }
    }
}

#[cfg(target_os = "windows")]
mod windows_paste {
    use anyhow::Result;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_KEYUP, SendInput,
        VIRTUAL_KEY,
    };

    const VK_CONTROL: u16 = 0x11;
    const VK_SHIFT: u16 = 0x10;
    const VK_V: u16 = 0x56;

    pub fn send_ctrl_v() -> Result<()> {
        let inputs = [
            make_key_input(VK_CONTROL, false),
            make_key_input(VK_V, false),
            make_key_input(VK_V, true),
            make_key_input(VK_CONTROL, true),
        ];
        send_inputs(&inputs)
    }

    pub fn send_ctrl_shift_v() -> Result<()> {
        let inputs = [
            make_key_input(VK_CONTROL, false),
            make_key_input(VK_SHIFT, false),
            make_key_input(VK_V, false),
            make_key_input(VK_V, true),
            make_key_input(VK_SHIFT, true),
            make_key_input(VK_CONTROL, true),
        ];
        send_inputs(&inputs)
    }

    fn send_inputs(inputs: &[INPUT]) -> Result<()> {
        unsafe {
            let sent = SendInput(inputs, std::mem::size_of::<INPUT>() as i32);
            if sent != inputs.len() as u32 {
                anyhow::bail!("SendInput failed, sent {} of {}", sent, inputs.len());
            }
        }
        Ok(())
    }

    fn make_key_input(vk: u16, key_up: bool) -> INPUT {
        let flags = if key_up {
            KEYEVENTF_KEYUP
        } else {
            KEYBD_EVENT_FLAGS(0)
        };

        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(vk),
                    wScan: 0,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    }
}

#[cfg(target_os = "windows")]
mod windows_keystrokes {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_KEYUP,
        KEYEVENTF_UNICODE, SendInput, VIRTUAL_KEY,
    };

    pub(super) fn ensure_all_inputs_sent(
        sent: u32,
        expected: usize,
        operation: &str,
    ) -> anyhow::Result<()> {
        if sent != expected as u32 {
            anyhow::bail!(
                "SendInput {} failed, sent {} of {} events",
                operation,
                sent,
                expected
            );
        }
        Ok(())
    }

    pub fn send_text_keystrokes_inner(text: &str) -> anyhow::Result<usize> {
        let inputs = build_unicode_input_events(text);
        let sent = unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
        ensure_all_inputs_sent(sent, inputs.len(), "typing")?;
        // Return UTF-16 code-unit count, NOT codepoint count: each KEYEVENTF_UNICODE
        // event we sent corresponds to one UTF-16 code unit (surrogate pairs are
        // two events). swap_typed_text issues one VK_BACK per backspace, matching
        // the per-code-unit grain. For non-BMP chars (emoji, rare CJK extension)
        // returning chars().count() would under-count and leave half-deleted
        // surrogates after the post-stop swap.
        Ok(text.encode_utf16().count())
    }

    /// Build a vector of INPUT events (KEYDOWN+KEYUP pair per character) for a
    /// string. Uses KEYEVENTF_UNICODE so the chars are injected directly without
    /// going through the IME composition queue. Surrogate pairs are sent as two
    /// separate events.
    pub(super) fn build_unicode_input_events(text: &str) -> Vec<INPUT> {
        let mut inputs = Vec::with_capacity(text.encode_utf16().count() * 2);
        for code_unit in text.encode_utf16() {
            let down = INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VIRTUAL_KEY(0),
                        wScan: code_unit,
                        dwFlags: KEYEVENTF_UNICODE,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            };
            let mut up = down;
            // SAFETY: ki union variant was set in `down` above; we only update dwFlags
            up.Anonymous.ki.dwFlags = KEYEVENTF_UNICODE | KEYEVENTF_KEYUP;
            inputs.push(down);
            inputs.push(up);
        }
        inputs
    }

    /// Build a single VK-based key event (KEYDOWN or KEYUP).
    pub(super) fn make_vk_event(vk: u16, key_up: bool) -> INPUT {
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(vk),
                    wScan: 0,
                    dwFlags: if key_up {
                        KEYEVENTF_KEYUP
                    } else {
                        KEYBD_EVENT_FLAGS(0)
                    },
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    }
}

/// Type `text` into the current foreground window via SendInput with
/// KEYEVENTF_UNICODE. Returns the number of UTF-16 code units actually typed
/// (post-sanitization) — NOT codepoints. SendInput's KEYEVENTF_UNICODE
/// generates one input event per UTF-16 unit, so each non-BMP codepoint
/// (emoji, some CJK extensions) costs two units. Callers that need to undo
/// the typed text via VK_BACK must use this UTF-16 count, since backspace also
/// deletes one UTF-16 unit at a time (see `swap_typed_text`).
///
/// Sanitization strips control chars, ANSI escapes, and converts
/// `\n`/`\r`/`\t` to space — so a malicious transcript cannot inject a newline
/// that would auto-execute a shell command. Even in a terminal, the user must
/// press Enter themselves to run anything typed.
pub fn send_text_keystrokes(text: &str) -> Result<usize> {
    #[cfg(target_os = "windows")]
    {
        // Refuse to type if focus drifted onto a credential prompt or the lock
        // screen mid-dictation — a transcript must never be injected into a
        // password/secure field. Returning 0 keeps the frontend's typed-char
        // counter in lockstep (it bails when charsTyped <= 0).
        if is_secure_foreground_window() {
            log::warn!(
                "[Live] suppressing keystrokes: foreground window is a secure/credential dialog"
            );
            return Ok(0);
        }
        let sanitized = sanitize_for_send_input(text);
        if sanitized.is_empty() {
            return Ok(0);
        }
        windows_keystrokes::send_text_keystrokes_inner(&sanitized)
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = text;
        Ok(0)
    }
}

/// Replace the last `backspaces` typed characters in the focused control with
/// `new_text`. Refuses to act if the foreground window OR focused control has
/// drifted from `expected_hwnd` / `expected_control` — the central safety
/// invariant of the post-stop swap. Both checks happen BEFORE any keystroke is
/// sent, so a drifted focus can never have its content backspaced.
pub fn swap_typed_text(
    backspaces: usize,
    new_text: &str,
    expected_hwnd: Option<isize>,
    expected_control: Option<isize>,
) -> Result<SwapResult> {
    #[cfg(target_os = "windows")]
    {
        let target = current_focus_target();
        if let Some(want) = expected_hwnd
            && target.window != want
        {
            return Ok(SwapResult::SkippedFocusDrift);
        }
        if let Some(want) = expected_control
            && target.control != want
        {
            return Ok(SwapResult::SkippedFocusDrift);
        }

        let sanitized = sanitize_for_send_input(new_text);
        if backspaces == 0 && sanitized.is_empty() {
            return Ok(SwapResult::SkippedNoChange);
        }

        let mut inputs = Vec::with_capacity(backspaces * 2 + sanitized.encode_utf16().count() * 2);
        for _ in 0..backspaces {
            inputs.push(windows_keystrokes::make_vk_event(0x08, false));
            inputs.push(windows_keystrokes::make_vk_event(0x08, true));
        }
        inputs.extend(windows_keystrokes::build_unicode_input_events(&sanitized));

        let sent = unsafe {
            windows::Win32::UI::Input::KeyboardAndMouse::SendInput(
                &inputs,
                std::mem::size_of::<windows::Win32::UI::Input::KeyboardAndMouse::INPUT>() as i32,
            )
        };
        windows_keystrokes::ensure_all_inputs_sent(sent, inputs.len(), "swap")?;
        Ok(SwapResult::Swapped)
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = (backspaces, new_text, expected_hwnd, expected_control);
        Ok(SwapResult::SkippedNoChange)
    }
}

/// Sanitize text before it goes through SendInput. Strips control characters,
/// ANSI escape sequences, and translates whitespace control chars to space.
/// This prevents a malicious transcript from injecting shell commands or
/// terminal escapes into the focused window.
pub fn sanitize_for_send_input(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            // ANSI escape: ESC [ ... <final-byte 0x40-0x7E>
            '\x1B' if chars.peek() == Some(&'[') => {
                chars.next(); // consume '['
                // Drop parameter bytes 0x30-0x3F and intermediate 0x20-0x2F until final 0x40-0x7E
                for cc in chars.by_ref() {
                    if ('\x40'..='\x7E').contains(&cc) {
                        break;
                    }
                }
            }
            // Newline / tab → space
            '\n' | '\r' | '\t' => out.push(' '),
            // C0 control chars (other than CR/LF/Tab handled above)
            c if (c as u32) < 0x20 => { /* drop */ }
            // DEL + C1 control chars
            c if (c as u32) >= 0x7F && (c as u32) <= 0x9F => { /* drop */ }
            // Everything else passes through (including all printable Unicode + emoji)
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "windows")]
    #[test]
    fn build_unicode_input_events_pair_per_char() {
        use windows::Win32::UI::Input::KeyboardAndMouse::{KEYEVENTF_KEYUP, KEYEVENTF_UNICODE};
        let events = windows_keystrokes::build_unicode_input_events("ab");
        assert_eq!(events.len(), 4); // 2 chars × (down + up)
        assert_eq!(unsafe { events[0].Anonymous.ki.wScan }, b'a' as u16);
        assert_eq!(unsafe { events[0].Anonymous.ki.dwFlags }, KEYEVENTF_UNICODE);
        assert_eq!(
            unsafe { events[1].Anonymous.ki.dwFlags },
            KEYEVENTF_UNICODE | KEYEVENTF_KEYUP
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn build_unicode_input_events_handles_surrogate_pair() {
        let events = windows_keystrokes::build_unicode_input_events("🎉"); // U+1F389, surrogate pair in UTF-16
        assert_eq!(events.len(), 4); // 2 code units × 2 events
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn send_input_count_must_match_exactly() {
        assert!(windows_keystrokes::ensure_all_inputs_sent(4, 4, "test").is_ok());
        assert!(windows_keystrokes::ensure_all_inputs_sent(0, 4, "test").is_err());
        assert!(windows_keystrokes::ensure_all_inputs_sent(3, 4, "test").is_err());
    }

    #[test]
    fn sanitize_strips_c0_control_chars() {
        let input = "hello\x00\x01\x02world\x08";
        let out = sanitize_for_send_input(input);
        assert_eq!(out, "helloworld");
    }

    #[test]
    fn sanitize_strips_c1_control_chars() {
        // U+007F (DEL) and U+009F (a C1 control char, using surrogate pair representation)
        let input = "hello\x7Fworld";
        let out = sanitize_for_send_input(input);
        assert_eq!(out, "helloworld");
    }

    #[test]
    fn sanitize_strips_ansi_escapes() {
        let input = "before\x1B[31mred\x1B[0mafter";
        let out = sanitize_for_send_input(input);
        assert_eq!(out, "beforeredafter");
    }

    #[test]
    fn sanitize_translates_newline_to_space() {
        let input = "line1\nline2\nline3";
        let out = sanitize_for_send_input(input);
        assert_eq!(out, "line1 line2 line3");
    }

    #[test]
    fn sanitize_translates_tab_to_space() {
        let input = "col1\tcol2";
        let out = sanitize_for_send_input(input);
        assert_eq!(out, "col1 col2");
    }

    #[test]
    fn sanitize_preserves_unicode() {
        let input = "café 你好 🎉";
        let out = sanitize_for_send_input(input);
        assert_eq!(out, "café 你好 🎉");
    }

    #[test]
    fn swap_returns_no_change_for_empty_inputs() {
        // No HWND/control to match against — pass None for both
        let result = swap_typed_text(0, "", None, None).unwrap();
        assert!(matches!(result, SwapResult::SkippedNoChange));
    }

    #[test]
    fn web_render_classes_are_detected() {
        // Chromium/Electron and Gecko render surfaces are NOT scopable — many
        // text boxes hide behind one HWND, so the swap must fall back to clipboard.
        assert!(is_web_render_class("Chrome_RenderWidgetHostHWND"));
        assert!(is_web_render_class("chrome_renderwidgethosthwnd")); // case-insensitive
        assert!(is_web_render_class("MozillaWindowClass"));
        // Native edit controls ARE scopable.
        assert!(!is_web_render_class("Edit"));
        assert!(!is_web_render_class("RICHEDIT50W"));
        assert!(!is_web_render_class("Notepad"));
        assert!(!is_web_render_class(""));
    }

    #[test]
    fn secure_window_classes_are_detected() {
        assert!(is_secure_window_class("Credential Dialog Xaml Host"));
        assert!(is_secure_window_class("credential dialog xaml host")); // case-insensitive
        assert!(is_secure_window_class("LockScreenControlsWindow"));
        // Ordinary app / terminal windows must NOT be treated as secure,
        // otherwise normal dictation would be silently suppressed.
        assert!(!is_secure_window_class("CASCADIA_HOSTING_WINDOW_CLASS"));
        assert!(!is_secure_window_class("Notepad"));
        assert!(!is_secure_window_class(""));
    }

    // NOTE: testing the HWND-mismatch path requires a running window manager.
    // The build itself validates the focus-drift check compiles correctly.
}
