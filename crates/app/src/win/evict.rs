//! Eviction on Windows (SPEC.md, Evicting a tenant): WM_CLOSE to every
//! visible top-level window of the tenant, then TerminateProcess if the
//! user asks. The core decides; this only acts and reports. No elevation,
//! no bulk eviction, nothing automatic.

use std::collections::BTreeSet;

use ramlord_core::ui::Report;
use windows::Win32::Foundation::{
    ERROR_ACCESS_DENIED, ERROR_INVALID_PARAMETER, LPARAM, WAIT_OBJECT_0, WPARAM,
};
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_ACCESS_RIGHTS, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
    PROCESS_TERMINATE, TerminateProcess, WaitForSingleObject,
};
use windows::Win32::UI::WindowsAndMessaging::{IsWindowVisible, PostMessageW, WM_CLOSE};

use super::provider::{Handle, window_pid, windows_of};

/// How long a refused TerminateProcess may take to show the process was
/// already on its way out (Windows refuses a process that is exiting).
const EXITING_MS: u32 = 200;

/// One evicted process, as far as we can see it.
enum Watched {
    /// Held open, so a reused pid cannot pass for one that stayed.
    Open(Handle),
    /// No such process: it has exited.
    Gone,
    /// Windows will not let us look: it counts as staying.
    Hidden,
}

/// The processes an eviction waits for.
#[derive(Default)]
pub struct Evictor {
    watched: Vec<(u32, Watched)>,
}

fn is(e: &windows::core::Error, code: windows::Win32::Foundation::WIN32_ERROR) -> bool {
    e.code() == code.to_hresult()
}

enum Opened {
    Open(Handle),
    Gone,
    Denied,
}

fn open(pid: u32, access: PROCESS_ACCESS_RIGHTS) -> Opened {
    // SAFETY: OpenProcess either fails or returns a handle that `Handle`
    // closes.
    match unsafe { OpenProcess(access, false, pid) } {
        Ok(h) => Opened::Open(Handle(h)),
        Err(e) if is(&e, ERROR_INVALID_PARAMETER) => Opened::Gone,
        Err(_) => Opened::Denied,
    }
}

fn exited(handle: &Handle, wait_ms: u32) -> bool {
    // SAFETY: a wait on a handle we hold.
    let event = unsafe { WaitForSingleObject(handle.0, wait_ms) };
    event == WAIT_OBJECT_0
}

/// RAMlord is Staff and never a tenant; this keeps it from ever closing
/// itself all the same.
fn others(pids: &[u32]) -> Vec<u32> {
    let own = std::process::id();
    pids.iter().copied().filter(|&p| p != own).collect()
}

impl Evictor {
    fn watch(&mut self, pids: &[u32]) {
        self.watched = pids
            .iter()
            .map(|&pid| {
                let watched =
                    match open(pid, PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION) {
                        Opened::Open(h) => Watched::Open(h),
                        Opened::Gone => Watched::Gone,
                        Opened::Denied => Watched::Hidden,
                    };
                (pid, watched)
            })
            .collect();
    }

    /// Posts WM_CLOSE to every visible top-level window of `pids`, those
    /// on other virtual desktops included.
    pub fn close(&mut self, pids: &[u32]) -> Report {
        let pids = others(pids);
        self.watch(&pids);
        let pids: BTreeSet<u32> = pids.into_iter().collect();
        let mut posted = 0;
        for hwnd in windows_of(None) {
            // SAFETY: a plain query on a window handle from EnumWindows.
            let visible = unsafe { IsWindowVisible(hwnd) }.as_bool();
            if !visible || !pids.contains(&window_pid(hwnd)) {
                continue;
            }
            // SAFETY: posting a message to a window handle from EnumWindows.
            match unsafe { PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0)) } {
                Ok(()) => posted += 1,
                // A window of a process with higher rights refuses us.
                Err(e) if is(&e, ERROR_ACCESS_DENIED) => return Report::Denied,
                Err(_) => {}
            }
        }
        Report::Posted(posted)
    }

    /// TerminateProcess on each of `pids`. Reports `Denied` if Windows
    /// refuses any of them.
    pub fn terminate(&mut self, pids: &[u32]) -> Option<Report> {
        let pids = others(pids);
        self.watch(&pids);
        let mut refused = false;
        for pid in pids {
            match open(pid, PROCESS_TERMINATE | PROCESS_SYNCHRONIZE) {
                Opened::Open(h) => {
                    // SAFETY: TerminateProcess acts only on a process the
                    // user chose to force out, through a handle we hold.
                    if unsafe { TerminateProcess(h.0, 1) }.is_err() && !exited(&h, EXITING_MS) {
                        refused = true;
                    }
                }
                Opened::Gone => {}
                Opened::Denied => refused = true,
            }
        }
        refused.then_some(Report::Denied)
    }

    /// The pids that have not exited yet.
    pub fn remaining(&self) -> Vec<u32> {
        self.watched
            .iter()
            .filter(|(_, w)| match w {
                Watched::Open(h) => !exited(h, 0),
                Watched::Gone => false,
                Watched::Hidden => true,
            })
            .map(|(pid, _)| *pid)
            .collect()
    }

    pub fn clear(&mut self) {
        self.watched.clear();
    }
}
