//! Only one copy runs (SPEC.md, Window and tray): a named mutex in the
//! user's session. Starting RAMlord again brings the first window forward.

use windows::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError, LPARAM, WPARAM};
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::WindowsAndMessaging::{
    AllowSetForegroundWindow, FindWindowW, GetWindowThreadProcessId, PostMessageW, WM_APP,
};
use windows::core::{PCWSTR, w};

/// The message that asks the first copy to show its window.
pub const WM_SHOW_FIRST: u32 = WM_APP + 3;

/// The window class both copies use.
pub const CLASS: PCWSTR = w!("RAMlordWindow");

/// True for the first copy. A later one asks the first to come forward
/// and gets false.
pub fn first() -> bool {
    // SAFETY: creates or opens a named mutex. Its handle is kept open for
    // the life of the process on purpose: that is what marks the first
    // copy.
    let Ok(_mutex) = (unsafe { CreateMutexW(None, false, w!("Local\\RAMlord.Instance")) }) else {
        return true;
    };
    // SAFETY: reads this thread's last error, set by CreateMutexW.
    if unsafe { GetLastError() } != ERROR_ALREADY_EXISTS {
        return true;
    }
    // SAFETY: finds the first copy's window and posts it a message; the
    // first copy may then take the foreground.
    unsafe {
        if let Ok(hwnd) = FindWindowW(CLASS, PCWSTR::null()) {
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            let _ = AllowSetForegroundWindow(pid);
            let _ = PostMessageW(Some(hwnd), WM_SHOW_FIRST, WPARAM(0), LPARAM(0));
        }
    }
    false
}
