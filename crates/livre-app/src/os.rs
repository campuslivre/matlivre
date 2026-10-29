//! The few native window calls tab dragging needs: where the cursor is, whether the left
//! button is still down (the drag can end outside our window, where egui stops seeing the
//! pointer), and which matlivre instance, if any, owns the window under the cursor. Each
//! instance marks its top-level window with the port it accepts tabs on. Also the Windows 11
//! frame of the borderless window (rounded corners, border, shadow) and where it opens.

#[cfg(windows)]
mod imp {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows_sys::Win32::Foundation::{HWND, POINT};
    use windows_sys::Win32::Graphics::Dwm::{DwmExtendFrameIntoClientArea, DwmSetWindowAttribute, DWMWA_BORDER_COLOR, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND};
    use windows_sys::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST};
    use windows_sys::Win32::UI::Controls::MARGINS;
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::HiDpi::{GetDpiForMonitor, GetDpiForWindow, GetSystemMetricsForDpi, MDT_EFFECTIVE_DPI};
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetAncestor, GetCursorPos, GetPropW, LoadImageW, SendMessageW, SetPropW, SetWindowPos, WindowFromPoint, GA_ROOT, ICON_BIG, ICON_SMALL, IMAGE_ICON,
        SM_CXICON, SM_CXSMICON, SWP_NOACTIVATE, SWP_NOZORDER, WM_SETICON,
    };

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

        /// The Windows 11 frame: rounded corners, a hairline border in `border` and the drop
        /// shadow (a one-pixel frame extended into the client area, which the app paints over).
        /// False where DWM does not round windows (Windows 10), so the app draws its own border.
        pub fn round_corners(&self, border: [u8; 3]) -> bool {
            let pref = DWMWCP_ROUND;
            let ok = unsafe { DwmSetWindowAttribute(self.0, DWMWA_WINDOW_CORNER_PREFERENCE as u32, &pref as *const i32 as _, 4) } == 0;
            if ok {
                self.set_border(border);
                let m = MARGINS { cxLeftWidth: 1, cxRightWidth: 1, cyTopHeight: 1, cyBottomHeight: 1 };
                unsafe { DwmExtendFrameIntoClientArea(self.0, &m) };
            }
            ok
        }

        /// Uses the icon embedded in the exe (resource 1) at the sizes Windows wants at this
        /// window's DPI, each drawn for its size rather than one image scaled.
        pub fn use_exe_icon(&self) {
            unsafe {
                let module = GetModuleHandleW(std::ptr::null());
                let dpi = GetDpiForWindow(self.0);
                for (which, metric) in [(ICON_SMALL, SM_CXSMICON), (ICON_BIG, SM_CXICON)] {
                    let px = GetSystemMetricsForDpi(metric, dpi);
                    let icon = LoadImageW(module, 1 as _, IMAGE_ICON, px, px, 0);
                    if !icon.is_null() {
                        SendMessageW(self.0, WM_SETICON, which as usize, icon as isize);
                    }
                }
            }
        }

        pub fn set_border(&self, [r, g, b]: [u8; 3]) {
            // COLORREF is 0x00BBGGRR.
            let c = r as u32 | (g as u32) << 8 | (b as u32) << 16;
            unsafe { DwmSetWindowAttribute(self.0, DWMWA_BORDER_COLOR as u32, &c as *const u32 as _, 4) };
        }

        /// Centres the window on the work area of the monitor under the cursor, `size` points
        /// large or as much of that as fits with a margin around it.
        pub fn place(&self, size: (f32, f32), min: (f32, f32)) {
            unsafe {
                let mut cursor = POINT { x: 0, y: 0 };
                GetCursorPos(&mut cursor);
                let monitor = MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST);
                let mut info: MONITORINFO = std::mem::zeroed();
                info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
                if GetMonitorInfoW(monitor, &mut info) == 0 {
                    return;
                }
                let (mut dpi_x, mut dpi_y) = (96, 96);
                GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y);
                let s = dpi_x as f32 / 96.0;
                let wa = info.rcWork;
                let (aw, ah) = ((wa.right - wa.left) as f32, (wa.bottom - wa.top) as f32);
                let margin = 24.0 * s;
                let w = (size.0 * s).min(aw - 2.0 * margin).max((min.0 * s).min(aw));
                let h = (size.1 * s).min(ah - 2.0 * margin).max((min.1 * s).min(ah));
                let x = wa.left + ((aw - w) / 2.0).round() as i32;
                let y = wa.top + ((ah - h) / 2.0).round() as i32;
                SetWindowPos(self.0, std::ptr::null_mut(), x, y, w.round() as i32, h.round() as i32, SWP_NOZORDER | SWP_NOACTIVATE);
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
        pub fn round_corners(&self, _: [u8; 3]) -> bool {
            false
        }
        pub fn set_border(&self, _: [u8; 3]) {}
        pub fn use_exe_icon(&self) {}
        pub fn place(&self, _: (f32, f32), _: (f32, f32)) {}
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
