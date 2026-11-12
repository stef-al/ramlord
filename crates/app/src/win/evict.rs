//! Eviction on Windows (SPEC.md, Evicting a tenant): WM_CLOSE to every
//! visible top-level window of the tenant, then TerminateProcess if the
//! user asks. The core decides; this only acts and reports. No elevation,
//! no bulk eviction, nothing automatic.
//!
//! Windows hands a process id out again once its process has gone, so an
//! id alone could name a stranger. Every process is therefore opened
//! first and its start time compared with the one the snapshot recorded
//! (`ProcessKey`); only a match is acted on, through that same handle,
//! and the handle is held while the eviction waits.

use std::collections::BTreeSet;

use ramlord_core::ui::{ProcessKey, Report};
use windows::Win32::Foundation::{
    ERROR_ACCESS_DENIED, ERROR_INVALID_PARAMETER, LPARAM, WAIT_OBJECT_0, WPARAM,
};
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_ACCESS_RIGHTS, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
    PROCESS_TERMINATE, TerminateProcess, WaitForSingleObject,
};
use windows::Win32::UI::WindowsAndMessaging::{IsWindowVisible, PostMessageW, WM_CLOSE};

use super::provider::{Handle, started_at_ms, window_pid, windows_of};

/// How long a refused TerminateProcess may take to show the process was
/// already on its way out (Windows refuses a process that is exiting).
const EXITING_MS: u32 = 200;

/// One evicted process, as far as we can see it.
enum Watched {
    /// The process the eviction named, held open: its id cannot be given
    /// to another process while the handle lives.
    Open(Handle),
    /// It has exited: no such process, or its id is another's by now.
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

/// Opens the process `key` names with `access` (which must include
/// `PROCESS_QUERY_LIMITED_INFORMATION`) and checks it is that process: a
/// later one with the same id started at another time. A key without a
/// start time cannot be checked, so it is never acted on.
fn open(key: ProcessKey, access: PROCESS_ACCESS_RIGHTS) -> Watched {
    // SAFETY: OpenProcess either fails or returns a handle that `Handle`
    // closes.
    match unsafe { OpenProcess(access, false, key.pid) } {
        Ok(h) => {
            let handle = Handle(h);
            match started_at_ms(handle.0) {
                Some(started) if key.started_at_ms != 0 && started == key.started_at_ms => {
                    Watched::Open(handle)
                }
                Some(_) if key.started_at_ms != 0 => Watched::Gone,
                _ => Watched::Hidden,
            }
        }
        Err(e) if is(&e, ERROR_INVALID_PARAMETER) => Watched::Gone,
        Err(_) => Watched::Hidden,
    }
}

fn exited(handle: &Handle, wait_ms: u32) -> bool {
    // SAFETY: a wait on a handle we hold.
    let event = unsafe { WaitForSingleObject(handle.0, wait_ms) };
    event == WAIT_OBJECT_0
}

/// RAMlord is Staff and never a tenant; this keeps it from ever closing
/// itself all the same.
fn others(processes: &[ProcessKey]) -> Vec<ProcessKey> {
    let own = std::process::id();
    processes.iter().copied().filter(|p| p.pid != own).collect()
}

impl Evictor {
    /// Posts WM_CLOSE to every visible top-level window of `processes`,
    /// those on other virtual desktops included.
    pub fn close(&mut self, processes: &[ProcessKey]) -> Report {
        self.watched = others(processes)
            .into_iter()
            .map(|key| {
                let access = PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION;
                (key.pid, open(key, access))
            })
            .collect();
        // Windows are matched by process id. Only ids whose process was
        // checked and is held open count: those cannot change hands.
        let pids: BTreeSet<u32> = self
            .watched
            .iter()
            .filter(|(_, w)| matches!(w, Watched::Open(_)))
            .map(|(pid, _)| *pid)
            .collect();
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

    /// TerminateProcess on each of `processes` that is still the process
    /// named. Reports `Denied` if Windows refuses any of them.
    pub fn terminate(&mut self, processes: &[ProcessKey]) -> Option<Report> {
        let mut refused = false;
        self.watched = others(processes)
            .into_iter()
            .map(|key| {
                let access =
                    PROCESS_TERMINATE | PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION;
                let watched = open(key, access);
                match &watched {
                    Watched::Open(h) => {
                        // SAFETY: TerminateProcess acts only on a process
                        // the user chose to force out, through the handle
                        // its identity was checked on.
                        if unsafe { TerminateProcess(h.0, 1) }.is_err() && !exited(h, EXITING_MS) {
                            refused = true;
                        }
                    }
                    Watched::Gone => {}
                    Watched::Hidden => refused = true,
                }
                (key.pid, watched)
            })
            .collect();
        refused.then_some(Report::Denied)
    }

    /// How many of the processes have not exited yet.
    pub fn remaining(&self) -> usize {
        self.watched
            .iter()
            .filter(|(_, w)| match w {
                Watched::Open(h) => !exited(h, 0),
                Watched::Gone => false,
                Watched::Hidden => true,
            })
            .count()
    }

    pub fn clear(&mut self) {
        self.watched.clear();
    }
}
