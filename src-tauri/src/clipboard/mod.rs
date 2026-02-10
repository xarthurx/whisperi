use anyhow::Result;

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
    use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

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
