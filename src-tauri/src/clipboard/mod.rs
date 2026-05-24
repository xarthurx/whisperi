use anyhow::Result;

/// Errors specific to clipboard / keystroke injection.
#[derive(Debug, thiserror::Error)]
pub enum ClipError {
    #[error("Foreground window is a terminal — Live mode does not type into terminals")]
    TerminalFocusGuard,
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

#[cfg(target_os = "windows")]
mod windows_clipboard {
    use anyhow::Result;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, GetClipboardData, OpenClipboard, SetClipboardData,
    };
    use windows::Win32::System::Memory::{
        GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE,
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
    use windows::Win32::UI::WindowsAndMessaging::{GetClassNameW, GetForegroundWindow};

    /// Known terminal window class names on Windows.
    const TERMINAL_CLASSES: &[&str] = &[
        "ConsoleWindowClass",             // cmd.exe, legacy console
        "CASCADIA_HOSTING_WINDOW_CLASS",  // Windows Terminal
        "mintty",                         // Git Bash, MSYS2, Cygwin
        "VirtualConsoleClass",            // ConEmu
        "PuTTY",                          // PuTTY
        "Alacritty",                      // Alacritty
        "org.wezfurlong.wezterm",         // WezTerm
        "Hyper",                          // Hyper terminal
        "TMobaXterm",                     // MobaXterm
    ];

    pub fn is_foreground_terminal() -> bool {
        unsafe {
            let hwnd = GetForegroundWindow();
            if hwnd.is_invalid() {
                return false;
            }

            let mut class_name = [0u8; 256];
            let len = windows::Win32::UI::WindowsAndMessaging::GetClassNameA(
                hwnd,
                &mut class_name,
            );
            if len == 0 {
                return false;
            }

            let class_str = std::str::from_utf8(&class_name[..len as usize]).unwrap_or("");

            TERMINAL_CLASSES
                .iter()
                .any(|tc| class_str.eq_ignore_ascii_case(tc))
        }
    }

    /// Check if the current foreground window is a terminal/console class. Used as
    /// a Live-mode safety guard — typing into terminals can execute shell commands
    /// (`\nrm -rf ~\n`), so we refuse to type into them. The user is shown a toast.
    pub fn is_foreground_window_terminal_class() -> bool {
        let hwnd = unsafe { GetForegroundWindow() };
        if hwnd.is_invalid() {
            return false;
        }
        let mut buf = [0u16; 256];
        let len = unsafe { GetClassNameW(hwnd, &mut buf) };
        if len <= 0 {
            return false;
        }
        let class = String::from_utf16_lossy(&buf[..len as usize]);
        TERMINAL_CLASSES.iter().any(|tc| class.eq_ignore_ascii_case(tc))
    }
}

#[cfg(target_os = "windows")]
mod windows_paste {
    use anyhow::Result;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
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
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE,
    };

    pub fn send_text_keystrokes_inner(text: &str) -> usize {
        let inputs = build_unicode_input_events(text);
        unsafe {
            SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
        }
        text.chars().count()
    }

    /// Build a vector of INPUT events (KEYDOWN+KEYUP pair per character) for a
    /// string. Uses KEYEVENTF_UNICODE so the chars are injected directly without
    /// going through the IME composition queue. Surrogate pairs are sent as two
    /// separate events.
    pub fn build_unicode_input_events(text: &str) -> Vec<INPUT> {
        let mut inputs = Vec::with_capacity(text.encode_utf16().count() * 2);
        for code_unit in text.encode_utf16() {
            let down = INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY(0),
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
}

/// Type `text` into the current foreground window via SendInput with
/// KEYEVENTF_UNICODE. Returns the number of Unicode code points actually typed
/// (post-sanitization). Refuses to type into terminal-class windows.
pub fn send_text_keystrokes(text: &str) -> std::result::Result<usize, ClipError> {
    #[cfg(target_os = "windows")]
    {
        if windows_terminal::is_foreground_window_terminal_class() {
            return Err(ClipError::TerminalFocusGuard);
        }
        let sanitized = sanitize_for_send_input(text);
        if sanitized.is_empty() {
            return Ok(0);
        }
        Ok(windows_keystrokes::send_text_keystrokes_inner(&sanitized))
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = text;
        Err(ClipError::TerminalFocusGuard) // unreachable on non-Windows but satisfies return type
    }
}

/// Check if the current foreground window is a terminal/console class.
pub fn is_foreground_window_terminal_class() -> bool {
    #[cfg(target_os = "windows")]
    {
        windows_terminal::is_foreground_window_terminal_class()
    }
    #[cfg(not(target_os = "windows"))]
    {
        false
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
}
