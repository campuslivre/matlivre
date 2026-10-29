//! The few native window calls tab dragging needs: where the cursor is, whether the left
//! button is still down (the drag can end outside our window, where egui stops seeing the
//! pointer), and which matlivre instance, if any, owns the window under the cursor. Each
//! instance marks its top-level window with the port it accepts tabs on.

#[cfg(windows)]
mod imp {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows_sys::Win32::Foundation::{HWND, POINT};
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON};
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetAncestor, GetCursorPos, GetPropW, SetPropW, WindowFromPoint, GA_ROOT};

    /// "matlivre.port", NUL-terminated UTF-16.
    fn prop_name() -> Vec<u16> {
        "matlivre.port".encode_utf16().chain(Some(0)).collect()
    }

    #[derive(Clone, Copy)]
    pub struct Window(HWND);

    // The handle is only an identifier passed back to Win32; it is never dereferenced.
    unsafe impl Send for Window {}

    pub enum Under {
        /// Our own window.
        Own,
        /// Another matlivre instance, reachable on this port.
        Instance(u16),
        /// Anything else: the desktop, another program, nothing.
        Other,
    }

    impl Window {
        pub fn from_handle(h: &impl HasWindowHandle) -> Option<Self> {
            match h.window_handle().ok()?.as_raw() {
                RawWindowHandle::Win32(w) => Some(Window(w.hwnd.get() as HWND)),
                _ => None,
            }
        }

        /// Marks the window as a matlivre instance that accepts tabs on `port`.
        pub fn advertise(&self, port: u16) {
            unsafe { SetPropW(self.0, prop_name().as_ptr(), port as usize as _) };
        }

        pub fn under_cursor(&self, (x, y): (i32, i32)) -> Under {
            unsafe {
                let hit = WindowFromPoint(POINT { x, y });
                if hit.is_null() {
                    return Under::Other;
                }
                let root = GetAncestor(hit, GA_ROOT);
                if root == self.0 {
                    return Under::Own;
                }
                match GetPropW(root, prop_name().as_ptr()) as usize {
                    0 => Under::Other,
                    port => Under::Instance(port as u16),
                }
            }
        }
    }

    /// The cursor in physical screen pixels.
    pub fn cursor_pos() -> Option<(i32, i32)> {
        let mut p = POINT { x: 0, y: 0 };
        (unsafe { GetCursorPos(&mut p) } != 0).then_some((p.x, p.y))
    }

    pub fn left_button_down() -> Option<bool> {
        Some(unsafe { GetAsyncKeyState(VK_LBUTTON as i32) } as u16 & 0x8000 != 0)
    }

    /// The drive roots with their Explorer names: "Local Disk (C:)", "Data (D:)".
    pub fn drives() -> Vec<(std::path::PathBuf, String)> {
        use windows_sys::Win32::Storage::FileSystem::{GetDriveTypeW, GetLogicalDrives, GetVolumeInformationW};
        const DRIVE_REMOVABLE: u32 = 2;
        const DRIVE_REMOTE: u32 = 4;
        const DRIVE_CDROM: u32 = 5;
        let mask = unsafe { GetLogicalDrives() };
        (0..26u8)
            .filter(|i| mask & (1 << i) != 0)
            .map(|i| {
                let letter = (b'A' + i) as char;
                let root: Vec<u16> = format!("{letter}:\\").encode_utf16().chain(Some(0)).collect();
                let kind = unsafe { GetDriveTypeW(root.as_ptr()) };
                let mut label = [0u16; 261];
                let ok = unsafe {
                    GetVolumeInformationW(root.as_ptr(), label.as_mut_ptr(), label.len() as u32, std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut(), 0)
                } != 0;
                let len = label.iter().position(|&c| c == 0).unwrap_or(0);
                let mut name = if ok { String::from_utf16_lossy(&label[..len]) } else { String::new() };
                if name.is_empty() {
                    name = match kind {
                        DRIVE_REMOVABLE => "USB Drive",
                        DRIVE_REMOTE => "Network Drive",
                        DRIVE_CDROM => "DVD Drive",
                        _ => "Local Disk",
                    }
                    .into();
                }
                (std::path::PathBuf::from(format!("{letter}:\\")), format!("{name} ({letter}:)"))
            })
            .collect()
    }
}

#[cfg(not(windows))]
mod imp {
    use raw_window_handle::HasWindowHandle;

    #[derive(Clone, Copy)]
    pub struct Window;

    pub enum Under {
        Own,
        #[allow(dead_code)]
        Instance(u16),
        Other,
    }

    impl Window {
        pub fn from_handle(_: &impl HasWindowHandle) -> Option<Self> {
            None
        }
        pub fn advertise(&self, _: u16) {}
        pub fn under_cursor(&self, _: (i32, i32)) -> Under {
            Under::Other
        }
    }

    pub fn cursor_pos() -> Option<(i32, i32)> {
        None
    }

    pub fn left_button_down() -> Option<bool> {
        None
    }

    pub fn drives() -> Vec<(std::path::PathBuf, String)> {
        vec![(std::path::PathBuf::from("/"), "File System".into())]
    }
}

pub use imp::*;
