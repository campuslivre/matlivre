//! macOS integration: the window under the mouse (for dragging tabs between windows), the
//! cursor and mouse button, and files opened from Finder or the Dock.

use std::ffi::{c_char, c_void, CStr};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Mutex, OnceLock};

use objc2::ffi;
use objc2::runtime::{AnyClass, AnyObject, Sel};
use objc2::{class, msg_send, sel};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

#[repr(C)]
#[derive(Clone, Copy)]
struct CGPoint {
    x: f64,
    y: f64,
}

// SAFETY: matches `struct CGPoint { CGFloat x; CGFloat y; }` on 64-bit macOS.
unsafe impl objc2::Encode for CGPoint {
    const ENCODING: objc2::Encoding = objc2::Encoding::Struct("CGPoint", &[objc2::Encoding::Double, objc2::Encoding::Double]);
}

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGEventCreate(source: *const c_void) -> *mut c_void;
    fn CGEventGetLocation(event: *const c_void) -> CGPoint;
    fn CGEventSourceButtonState(state: i32, button: u32) -> bool;
}
#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFRelease(cf: *const c_void);
}
extern "C" {
    fn kill(pid: i32, sig: i32) -> i32;
}

/// The cursor in global screen points, origin top-left.
pub fn cursor_pos() -> Option<(i32, i32)> {
    unsafe {
        let e = CGEventCreate(std::ptr::null());
        if e.is_null() {
            return None;
        }
        let p = CGEventGetLocation(e);
        CFRelease(e);
        Some((p.x.round() as i32, p.y.round() as i32))
    }
}

/// kCGEventSourceStateCombinedSessionState, kCGMouseButtonLeft.
pub fn left_button_down() -> Option<bool> {
    Some(unsafe { CGEventSourceButtonState(0, 0) })
}

pub enum Under {
    Own,
    Instance(u16),
    Other,
}

/// The app's main window, identified by its NSView.
#[derive(Clone, Copy)]
pub struct Window(usize);

impl Window {
    pub fn from_handle(h: &impl HasWindowHandle) -> Option<Self> {
        match h.window_handle().ok()?.as_raw() {
            RawWindowHandle::AppKit(w) => Some(Window(w.ns_view.as_ptr() as usize)),
            _ => None,
        }
    }

    fn ns_window(&self) -> *mut AnyObject {
        unsafe {
            let view = self.0 as *mut AnyObject;
            msg_send![view, window]
        }
    }

    fn number(&self) -> isize {
        let w = self.ns_window();
        if w.is_null() {
            return -1;
        }
        unsafe { msg_send![w, windowNumber] }
    }

    pub fn round_corners(&self, _: [u8; 3]) -> bool {
        false
    }
    pub fn set_border(&self, _: [u8; 3]) {}
    pub fn use_exe_icon(&self) {}

    /// Centres the window on the screen.
    pub fn place(&self, _: (f32, f32), _: (f32, f32)) {
        let w = self.ns_window();
        if !w.is_null() {
            unsafe {
                let _: () = msg_send![w, center];
            }
        }
    }

    /// Records this window's number and the port it takes tabs on, where other instances look.
    pub fn advertise(&self, port: u16) {
        let Some(dir) = instances_dir() else { return };
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(dir.join(port.to_string()), format!("{} {}\n", std::process::id(), self.number()));
    }

    pub fn under_cursor(&self, _: (i32, i32)) -> Under {
        let number = unsafe {
            let p: CGPoint = msg_send![class!(NSEvent), mouseLocation];
            let n: isize = msg_send![class!(NSWindow), windowNumberAtPoint: p, belowWindowWithWindowNumber: 0isize];
            n
        };
        if number == self.number() {
            return Under::Own;
        }
        let Some(dir) = instances_dir() else { return Under::Other };
        let Ok(rd) = std::fs::read_dir(&dir) else { return Under::Other };
        for e in rd.flatten() {
            let Some(port) = e.file_name().to_str().and_then(|n| n.parse::<u16>().ok()) else { continue };
            let text = std::fs::read_to_string(e.path()).unwrap_or_default();
            let mut it = text.split_whitespace().filter_map(|t| t.parse::<i64>().ok());
            let (Some(pid), Some(num)) = (it.next(), it.next()) else { continue };
            if unsafe { kill(pid as i32, 0) } != 0 {
                let _ = std::fs::remove_file(e.path()); // that instance has quit
                continue;
            }
            if num as isize == number {
                return Under::Instance(port);
            }
        }
        Under::Other
    }
}

fn instances_dir() -> Option<PathBuf> {
    crate::app::app_dir().map(|d| d.join("instances"))
}

// ---- files opened from Finder or the Dock ---------------------------------------------------

static OPEN_TX: OnceLock<Mutex<(Sender<PathBuf>, eframe::egui::Context)>> = OnceLock::new();

unsafe extern "C" fn open_files(_this: *mut AnyObject, _sel: Sel, app: *mut AnyObject, files: *mut AnyObject) {
    let count: usize = msg_send![files, count];
    if let Some(m) = OPEN_TX.get() {
        let (tx, ctx) = &*m.lock().unwrap();
        for i in 0..count {
            let s: *mut AnyObject = msg_send![files, objectAtIndex: i];
            let c: *const c_char = msg_send![s, UTF8String];
            if !c.is_null() {
                let _ = tx.send(PathBuf::from(CStr::from_ptr(c).to_string_lossy().into_owned()));
            }
        }
        ctx.request_repaint();
    }
    // NSApplicationDelegateReplySuccess
    let _: () = msg_send![app, replyToOpenOrPrint: 0isize];
}

/// Makes Finder's "Open", "Open With" and files dropped on the Dock icon arrive as paths:
/// adds `application:openFiles:` to the application delegate winit installed.
pub fn receive_open_files(ctx: &eframe::egui::Context) -> Receiver<PathBuf> {
    let (tx, rx) = mpsc::channel();
    let _ = OPEN_TX.set(Mutex::new((tx, ctx.clone())));
    unsafe {
        let app: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
        let delegate: *mut AnyObject = msg_send![app, delegate];
        if !delegate.is_null() {
            let cls: &AnyClass = (*delegate).class();
            let imp: unsafe extern "C" fn() = std::mem::transmute(open_files as unsafe extern "C" fn(_, _, _, _));
            ffi::class_addMethod(cls as *const AnyClass as *mut _, sel!(application:openFiles:).as_ptr() as _, Some(imp), c"v@:@@".as_ptr());
        }
    }
    rx
}

#[allow(dead_code)]
pub fn is_matlab_file(p: &Path) -> bool {
    matches!(p.extension().and_then(|e| e.to_str()), Some("m" | "mlx"))
}
