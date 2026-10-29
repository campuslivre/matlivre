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
}

pub use imp::*;
