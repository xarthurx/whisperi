use anyhow::Result;

/// Write text to clipboard and simulate paste (Ctrl+V)
pub fn paste_text(text: &str) -> Result<()> {
    set_clipboard(text)?;
    std::thread::sleep(std::time::Duration::from_millis(30));
    simulate_paste()?;
    Ok(())
}

fn set_clipboard(text: &str) -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        windows_clipboard::set_text(text)?;
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = text;
        anyhow::bail!("Clipboard not yet implemented for this platform");
    }

    Ok(())
}

fn simulate_paste() -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        windows_paste::send_ctrl_v()?;
    }

    #[cfg(not(target_os = "windows"))]
    {
        anyhow::bail!("Paste simulation not yet implemented for this platform");
    }

    Ok(())
}

#[cfg(target_os = "windows")]
mod windows_clipboard {
    use anyhow::Result;
    use windows::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
    };
    use windows::Win32::System::Memory::{
        GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE,
    };
    use windows::Win32::Foundation::HWND;

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
}

#[cfg(target_os = "windows")]
mod windows_paste {
    use anyhow::Result;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP,
        KEYBD_EVENT_FLAGS, VIRTUAL_KEY,
    };

    const VK_CONTROL: u16 = 0x11;
    const VK_V: u16 = 0x56;

    pub fn send_ctrl_v() -> Result<()> {
        let inputs = [
            make_key_input(VK_CONTROL, false),
            make_key_input(VK_V, false),
            make_key_input(VK_V, true),
            make_key_input(VK_CONTROL, true),
        ];

        unsafe {
            let sent = SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
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
