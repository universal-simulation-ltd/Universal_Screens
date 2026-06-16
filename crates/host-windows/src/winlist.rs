//! Enumerate and focus top-level windows, so the clicker can choose which window
//! its keystrokes land in (`SendInput` always targets the foreground window).

use windows::core::BOOL;
use windows::Win32::Foundation::{HWND, LPARAM};
use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, EnumWindows, GetForegroundWindow, GetWindowLongW, GetWindowTextLengthW,
    GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible, SetForegroundWindow, ShowWindow,
    GWL_EXSTYLE, SW_RESTORE, WS_EX_TOOLWINDOW,
};

/// List visible top-level windows that have a title, as `(id, title)` where `id`
/// is the window handle (echoed back to [`focus_window`]). Tool windows are
/// skipped, and tabs/newlines in titles are flattened to spaces.
pub fn list_windows() -> Vec<(i64, String)> {
    let mut windows: Vec<(i64, String)> = Vec::new();
    unsafe {
        let _ = EnumWindows(Some(enum_proc), LPARAM(&mut windows as *mut _ as isize));
    }
    windows
}

unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let windows = unsafe { &mut *(lparam.0 as *mut Vec<(i64, String)>) };
    unsafe {
        if !IsWindowVisible(hwnd).as_bool() {
            return BOOL(1); // continue enumeration
        }
        let len = GetWindowTextLengthW(hwnd);
        if len <= 0 {
            return BOOL(1); // continue enumeration
        }
        // Skip tool windows (palettes, overlays, etc.).
        if GetWindowLongW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TOOLWINDOW.0 != 0 {
            return BOOL(1); // continue enumeration
        }
        let mut buf = vec![0u16; (len + 1) as usize];
        let n = GetWindowTextW(hwnd, &mut buf);
        if n > 0 {
            let title = String::from_utf16_lossy(&buf[..n as usize]).replace(['\t', '\n', '\r'], " ");
            windows.push((hwnd.0 as i64, title));
        }
    }
    BOOL(1)
}

/// Bring the window with handle `id` to the foreground so subsequent keystrokes
/// land in it. Uses the standard `AttachThreadInput` dance, since Windows
/// otherwise only lets the current foreground process call `SetForegroundWindow`
/// (a background caller just gets a taskbar flash).
pub fn focus_window(id: i64) {
    let hwnd = HWND(id as *mut core::ffi::c_void);
    unsafe {
        let fg_thread = GetWindowThreadProcessId(GetForegroundWindow(), None);
        let this_thread = GetCurrentThreadId();
        let attach = fg_thread != 0 && fg_thread != this_thread;
        if attach {
            let _ = AttachThreadInput(this_thread, fg_thread, true);
        }
        let _ = ShowWindow(hwnd, SW_RESTORE); // un-minimize if needed
        let _ = BringWindowToTop(hwnd);
        let _ = SetForegroundWindow(hwnd);
        if attach {
            let _ = AttachThreadInput(this_thread, fg_thread, false);
        }
    }
}
