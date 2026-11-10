//! The provider: builds a Snapshot from Win32 (SPEC.md, What is measured;
//! The Windows layer). It reads; it never changes anything.
//!
//! One `NtQuerySystemInformation(SystemProcessInformation)` call lists every
//! process with its private working set, working set, commit, session,
//! start time, parent and name, without opening any of them: this is how
//! Task Manager measures processes it cannot open, such as services. Only
//! processes outside session 0 are opened, for their image path, owner
//! and FileDescription.

use std::collections::{BTreeSet, HashMap};
use std::ffi::c_void;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ramlord_core::snapshot::{Process, Snapshot};
use windows::Wdk::System::SystemInformation::{NtQuerySystemInformation, SystemProcessInformation};
use windows::Win32::Foundation::{
    CloseHandle, FILETIME, HANDLE, HWND, LPARAM, RECT, STATUS_INFO_LENGTH_MISMATCH, TRUE,
};
use windows::Win32::Graphics::Dwm::{DWMWA_CLOAKED, DwmGetWindowAttribute};
use windows::Win32::Security::{
    GetLengthSid, GetTokenInformation, IsValidSid, TOKEN_QUERY, TOKEN_USER, TokenUser,
};
use windows::Win32::Storage::FileSystem::{
    GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW,
};
use windows::Win32::System::ProcessStatus::{
    GetPerformanceInfo, GetProcessMemoryInfo, PERFORMANCE_INFORMATION, PROCESS_MEMORY_COUNTERS,
    PROCESS_MEMORY_COUNTERS_EX2,
};
use windows::Win32::System::SystemInformation::GetPhysicallyInstalledSystemMemory;
use windows::Win32::System::Threading::{
    GetCurrentProcess, GetCurrentProcessId, GetProcessTimes, OpenProcess, OpenProcessToken,
    PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::System::WindowsProgramming::SYSTEM_PROCESS_INFORMATION;
use windows::Win32::UI::WindowsAndMessaging::{
    EnumChildWindows, EnumWindows, GWL_EXSTYLE, GetClassNameW, GetForegroundWindow,
    GetWindowLongPtrW, GetWindowRect, GetWindowThreadProcessId, IsWindowVisible, WS_EX_TOOLWINDOW,
};
use windows::core::{BOOL, PCWSTR, PWSTR};

/// A process handle that closes itself.
pub(super) struct Handle(pub(super) HANDLE);

impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: the handle came from OpenProcess and is closed once.
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// 100-nanosecond intervals since 1601 to milliseconds since 1970; 0 for
/// an unknown time.
fn ticks_to_unix_ms(ticks: u64) -> u64 {
    const EPOCH_DIFFERENCE: u64 = 116_444_736_000_000_000;
    ticks.saturating_sub(EPOCH_DIFFERENCE) / 10_000
}

fn filetime_to_unix_ms(ft: FILETIME) -> u64 {
    ticks_to_unix_ms((u64::from(ft.dwHighDateTime) << 32) | u64::from(ft.dwLowDateTime))
}

fn filetime_duration(ft: FILETIME) -> Duration {
    let ticks = (u64::from(ft.dwHighDateTime) << 32) | u64::from(ft.dwLowDateTime);
    Duration::from_nanos(ticks.saturating_mul(100))
}

/// RAMlord's own memory: private working set (None when the EX2
/// structure is not supported) and working set.
struct Counters {
    private_ws: Option<u64>,
    ws: u64,
}

fn counters(process: HANDLE) -> Option<Counters> {
    let mut ex2 = PROCESS_MEMORY_COUNTERS_EX2 {
        cb: size_of::<PROCESS_MEMORY_COUNTERS_EX2>() as u32,
        ..Default::default()
    };
    // SAFETY: the EX2 structure starts with PROCESS_MEMORY_COUNTERS and `cb`
    // tells the call how much of it to fill.
    if unsafe { GetProcessMemoryInfo(process, (&raw mut ex2).cast(), ex2.cb) }.is_ok() {
        return Some(Counters {
            private_ws: Some(ex2.PrivateWorkingSetSize as u64),
            ws: ex2.WorkingSetSize as u64,
        });
    }
    let mut basic = PROCESS_MEMORY_COUNTERS {
        cb: size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
        ..Default::default()
    };
    // SAFETY: as above, with the basic structure.
    if unsafe { GetProcessMemoryInfo(process, &mut basic, basic.cb) }.is_ok() {
        return Some(Counters {
            private_ws: None,
            ws: basic.WorkingSetSize as u64,
        });
    }
    None
}

/// This process's CPU time so far, user and kernel.
pub fn own_cpu_time() -> Duration {
    let (mut c, mut e, mut k, mut u) = Default::default();
    // SAFETY: the pseudo handle of the current process needs no closing.
    match unsafe { GetProcessTimes(GetCurrentProcess(), &mut c, &mut e, &mut k, &mut u) } {
        Ok(()) => filetime_duration(k) + filetime_duration(u),
        Err(_) => Duration::ZERO,
    }
}

/// This process's private working set, or its working set if that is all
/// Windows reports.
pub fn own_private_bytes() -> u64 {
    // SAFETY: the pseudo handle of the current process needs no closing.
    match counters(unsafe { GetCurrentProcess() }) {
        Some(c) => c.private_ws.filter(|&p| p > 0).unwrap_or(c.ws),
        None => 0,
    }
}

/// What the provider learns from one opened process.
struct Opened {
    path: Option<String>,
    started_at_ms: u64,
    /// The SID of the user the process runs as, if it can be read.
    owner: Option<Vec<u8>>,
}

/// The SID, as bytes, of the user `process` runs as. Compared, never
/// stored: a snapshot holds no user names or SIDs.
fn owner_sid(process: HANDLE) -> Option<Vec<u8>> {
    let mut token = HANDLE::default();
    // SAFETY: on success `token` is a handle we own, closed by `Handle`.
    unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) }.ok()?;
    let token = Handle(token);
    let mut len = 0u32;
    // SAFETY: a size query; it fails with the size needed.
    let _ = unsafe { GetTokenInformation(token.0, TokenUser, None, 0, &mut len) };
    if len == 0 {
        return None;
    }
    // u64 words keep the TOKEN_USER aligned.
    let mut buf = vec![0u64; (len as usize).div_ceil(8)];
    // SAFETY: `buf` holds at least `len` bytes.
    unsafe {
        GetTokenInformation(
            token.0,
            TokenUser,
            Some(buf.as_mut_ptr().cast()),
            len,
            &mut len,
        )
    }
    .ok()?;
    // SAFETY: the call filled a TOKEN_USER whose SID points into `buf`;
    // the SID's length is checked before its bytes are copied.
    unsafe {
        let user = &*(buf.as_ptr() as *const TOKEN_USER);
        let sid = user.User.Sid;
        if !IsValidSid(sid).as_bool() {
            return None;
        }
        let n = GetLengthSid(sid) as usize;
        Some(std::slice::from_raw_parts(sid.0 as *const u8, n).to_vec())
    }
}

fn open(pid: u32) -> Option<Opened> {
    // SAFETY: OpenProcess either fails or returns a handle we own.
    let handle =
        Handle(unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?);
    let mut buf = [0u16; 1024];
    let mut len = buf.len() as u32;
    // SAFETY: `len` holds the buffer's size in characters.
    let path = unsafe {
        QueryFullProcessImageNameW(
            handle.0,
            PROCESS_NAME_WIN32,
            PWSTR(buf.as_mut_ptr()),
            &mut len,
        )
    }
    .ok()
    .map(|()| String::from_utf16_lossy(&buf[..len as usize]));
    let (mut created, mut exited, mut kernel, mut user) = Default::default();
    // SAFETY: four FILETIME outputs on a live handle.
    let started_at_ms = match unsafe {
        GetProcessTimes(handle.0, &mut created, &mut exited, &mut kernel, &mut user)
    } {
        Ok(()) => filetime_to_unix_ms(created),
        Err(_) => 0,
    };
    Some(Opened {
        path,
        started_at_ms,
        owner: owner_sid(handle.0),
    })
}

/// One entry of the system's process list.
struct Listed {
    pid: u32,
    parent_pid: u32,
    session: u32,
    started_at_ms: u64,
    name: String,
    private_ws: u64,
    ws: u64,
    commit: u64,
}

/// Where Windows keeps two fields inside what the documented
/// SYSTEM_PROCESS_INFORMATION calls `Reserved1` (the layout since Vista,
/// which Task Manager reads): the private working set and the create time.
const PRIVATE_WS_AT: usize = 0;
const CREATE_TIME_AT: usize = 24;

/// The name of a process list entry; its buffer must lie inside the list.
fn listed_name(info: &SYSTEM_PROCESS_INFORMATION, list: &[u64]) -> String {
    let name = &info.ImageName;
    let lo = list.as_ptr() as usize;
    let hi = lo + list.len() * 8;
    let start = name.Buffer.0 as usize;
    let bytes = usize::from(name.Length);
    if name.Buffer.is_null() || bytes == 0 || start < lo || start + bytes > hi || start % 2 != 0 {
        return String::new();
    }
    // SAFETY: the name's `bytes` lie inside `list`, checked above, and are
    // aligned UTF-16.
    String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(name.Buffer.0, bytes / 2) })
}

/// Every process, from one NtQuerySystemInformation call.
fn process_list() -> Result<Vec<Listed>, String> {
    // u64 words keep every entry 8-byte aligned.
    let mut list: Vec<u64> = vec![0; 64 * 1024];
    for _ in 0..8 {
        let bytes = u32::try_from(list.len() * 8).unwrap_or(u32::MAX);
        let mut needed = 0u32;
        // SAFETY: the buffer holds `bytes` bytes, as the call is told.
        let status = unsafe {
            NtQuerySystemInformation(
                SystemProcessInformation,
                list.as_mut_ptr().cast(),
                bytes,
                &mut needed,
            )
        };
        if status == STATUS_INFO_LENGTH_MISMATCH {
            // Processes come and go between calls: ask for some room more.
            let want = (needed as usize).max(list.len() * 8) + 64 * 1024;
            list.resize(want.div_ceil(8), 0);
            continue;
        }
        if status.0 < 0 {
            return Err(format!("NtQuerySystemInformation: {:#010x}", status.0));
        }
        let mut found = Vec::new();
        let mut offset = 0usize;
        let size = size_of::<SYSTEM_PROCESS_INFORMATION>();
        while offset + size <= list.len() * 8 {
            // SAFETY: the entry lies inside `list` (checked by the loop) at an
            // offset the system aligned, in a buffer aligned to 8.
            let info = unsafe {
                &*(list.as_ptr().cast::<u8>().add(offset) as *const SYSTEM_PROCESS_INFORMATION)
            };
            let field = |at: usize| {
                let mut bytes = [0u8; 8];
                bytes.copy_from_slice(&info.Reserved1[at..at + 8]);
                u64::from_ne_bytes(bytes)
            };
            found.push(Listed {
                pid: info.UniqueProcessId.0 as usize as u32,
                parent_pid: info.Reserved2 as usize as u32,
                session: info.SessionId,
                started_at_ms: ticks_to_unix_ms(field(CREATE_TIME_AT)),
                name: listed_name(info, &list),
                private_ws: field(PRIVATE_WS_AT),
                ws: info.WorkingSetSize as u64,
                commit: info.PagefileUsage as u64,
            });
            if info.NextEntryOffset == 0 {
                break;
            }
            offset += info.NextEntryOffset as usize;
        }
        return Ok(found);
    }
    Err("NtQuerySystemInformation: the process list kept growing".into())
}

/// The FileDescription in an image's version resource.
fn file_description(path: &str) -> Option<String> {
    let name = wide(path);
    // SAFETY: `name` is a NUL-terminated wide string.
    let size = unsafe { GetFileVersionInfoSizeW(PCWSTR(name.as_ptr()), None) };
    if size == 0 {
        return None;
    }
    let mut data = vec![0u8; size as usize];
    // SAFETY: `data` holds `size` bytes, as the call was told.
    unsafe { GetFileVersionInfoW(PCWSTR(name.as_ptr()), None, size, data.as_mut_ptr().cast()) }
        .ok()?;

    // Look up a value; the slice points into `data`, clamped to its end.
    let query = |key: &str| -> Option<&[u16]> {
        let key = wide(key);
        let mut ptr: *mut c_void = std::ptr::null_mut();
        let mut len = 0u32;
        // SAFETY: on success `ptr` points into `data` and `len` counts the
        // value's units (bytes for Translation, characters for strings).
        let ok = unsafe {
            VerQueryValueW(
                data.as_ptr().cast(),
                PCWSTR(key.as_ptr()),
                &mut ptr,
                &mut len,
            )
        };
        if !ok.as_bool() || ptr.is_null() {
            return None;
        }
        let offset = (ptr as usize).checked_sub(data.as_ptr() as usize)?;
        let room = data.len().checked_sub(offset)? / 2;
        // SAFETY: `ptr` lies inside `data`, and at most `room` u16 values
        // fit between it and the end of `data`.
        Some(unsafe { std::slice::from_raw_parts(ptr as *const u16, (len as usize).min(room)) })
    };

    let mut languages: Vec<String> = Vec::new();
    if let Some(units) = query("\\VarFileInfo\\Translation") {
        // This length is in bytes, so only the first half of the slice is
        // the value: a language and a code page per pair.
        for pair in units[..units.len() / 2].chunks_exact(2) {
            languages.push(format!("{:04x}{:04x}", pair[0], pair[1]));
        }
    }
    languages.extend(["040904b0", "040904e4", "04090000"].map(String::from));
    for language in languages {
        if let Some(text) = query(&format!("\\StringFileInfo\\{language}\\FileDescription")) {
            let text = String::from_utf16_lossy(text);
            let text = text.trim_end_matches('\0').trim();
            if !text.is_empty() {
                return Some(text.to_string());
            }
        }
    }
    None
}

/// Visible top-level windows: visible, not cloaked, not tool windows and
/// larger than zero.
pub(super) fn is_app_window(hwnd: HWND) -> bool {
    // SAFETY: plain queries on a window handle from EnumWindows.
    unsafe {
        if !IsWindowVisible(hwnd).as_bool() {
            return false;
        }
        let mut cloaked = 0u32;
        let cloaked = DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            (&raw mut cloaked).cast(),
            size_of::<u32>() as u32,
        )
        .is_ok()
            && cloaked != 0;
        if cloaked {
            return false;
        }
        let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        if ex_style & WS_EX_TOOLWINDOW.0 != 0 {
            return false;
        }
        let mut rect = RECT::default();
        GetWindowRect(hwnd, &mut rect).is_ok() && rect.right > rect.left && rect.bottom > rect.top
    }
}

pub(super) fn window_pid(hwnd: HWND) -> u32 {
    let mut pid = 0u32;
    // SAFETY: writes one u32.
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    pid
}

extern "system" fn collect(hwnd: HWND, list: LPARAM) -> BOOL {
    // SAFETY: `list` is the `&mut Vec<HWND>` passed by `windows_of`, alive
    // for the whole enumeration.
    let list = unsafe { &mut *(list.0 as *mut Vec<HWND>) };
    list.push(hwnd);
    TRUE
}

/// Top-level windows, or the children of `parent`.
pub(super) fn windows_of(parent: Option<HWND>) -> Vec<HWND> {
    let mut list: Vec<HWND> = Vec::new();
    let param = LPARAM(&raw mut list as isize);
    // SAFETY: `collect` only pushes into `list`, which outlives the call.
    unsafe {
        match parent {
            None => {
                let _ = EnumWindows(Some(collect), param);
            }
            Some(parent) => {
                let _ = EnumChildWindows(Some(parent), Some(collect), param);
            }
        }
    }
    list
}

/// The process a window belongs to. A Store app's frame belongs to
/// ApplicationFrameHost; for a window of class ApplicationFrameWindow,
/// use the child window that belongs to a different process.
fn window_owner(hwnd: HWND) -> u32 {
    let pid = window_pid(hwnd);
    let mut class = [0u16; 64];
    // SAFETY: the buffer's length is passed with it.
    let len = unsafe { GetClassNameW(hwnd, &mut class) };
    let class = String::from_utf16_lossy(&class[..len.max(0) as usize]);
    if class == "ApplicationFrameWindow" {
        let child = windows_of(Some(hwnd))
            .into_iter()
            .map(window_pid)
            .find(|&child| child != pid && child != 0);
        if let Some(child) = child {
            return child;
        }
    }
    pid
}

/// The process that owns the foreground window, if any. The Windows layer
/// asks once a second (SPEC.md, Flags: focus).
pub fn foreground_pid() -> Option<u32> {
    // SAFETY: plain query.
    let foreground = unsafe { GetForegroundWindow() };
    (!foreground.is_invalid()).then(|| window_owner(foreground))
}

/// Visible windows are enumerated every 5 seconds (SPEC.md, Timing).
const WINDOWS_EVERY_MS: u64 = 5_000;

/// Builds snapshots. Keeps FileDescriptions by image path, so each image's
/// version resource is read once, and the visible windows for 5 seconds.
pub struct Provider {
    descriptions: HashMap<String, Option<String>>,
    visible: BTreeSet<u32>,
    windows_at_ms: Option<u64>,
}

impl Provider {
    pub fn new() -> Provider {
        Provider {
            descriptions: HashMap::new(),
            visible: BTreeSet::new(),
            windows_at_ms: None,
        }
    }

    pub fn sample(&mut self) -> Result<Snapshot, String> {
        let at_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        let mut perf = PERFORMANCE_INFORMATION {
            cb: size_of::<PERFORMANCE_INFORMATION>() as u32,
            ..Default::default()
        };
        // SAFETY: `cb` holds the structure's size.
        unsafe { GetPerformanceInfo(&mut perf, perf.cb) }
            .map_err(|e| format!("GetPerformanceInfo: {e}"))?;
        let page = perf.PageSize as u64;
        let physical_total = perf.PhysicalTotal as u64 * page;
        let mut installed_kb = 0u64;
        // SAFETY: writes one u64.
        let installed = match unsafe { GetPhysicallyInstalledSystemMemory(&mut installed_kb) } {
            Ok(()) => installed_kb * 1024,
            Err(_) => 0,
        };
        // Installed memory includes what the hardware reserves; that counts
        // as Staff. If the call fails or reports less, use PhysicalTotal.
        let capacity = installed.max(physical_total);

        // Visible windows every 5 seconds; the foreground at every sample.
        let stale = self
            .windows_at_ms
            .is_none_or(|t| at_ms < t || at_ms - t >= WINDOWS_EVERY_MS);
        if stale {
            self.visible = windows_of(None)
                .into_iter()
                .filter(|&w| is_app_window(w))
                .map(window_owner)
                .collect();
            self.windows_at_ms = Some(at_ms);
        }
        let visible = &self.visible;
        let foreground_pid = foreground_pid();

        let listed = process_list()?;
        // SAFETY: plain query.
        let self_pid = unsafe { GetCurrentProcessId() };
        // SAFETY: the pseudo handle of the current process needs no closing.
        let own_sid = owner_sid(unsafe { GetCurrentProcess() });
        let mut processes = Vec::with_capacity(listed.len());
        let mut ours: Vec<bool> = Vec::with_capacity(listed.len());
        for e in listed {
            // Session 0 processes are always Staff, and services mostly
            // refuse to open anyway: only the rest are opened.
            let opened = if e.session != 0 { open(e.pid) } else { None };
            ours.push(
                own_sid.is_some()
                    && opened.as_ref().and_then(|o| o.owner.as_ref()) == own_sid.as_ref(),
            );
            let path = opened.as_ref().and_then(|o| o.path.clone());
            let description = match &path {
                Some(path) => self
                    .descriptions
                    .entry(path.clone())
                    .or_insert_with(|| file_description(path))
                    .clone(),
                None => None,
            };
            let started_at_ms = if e.started_at_ms > 0 {
                e.started_at_ms
            } else {
                opened.as_ref().map_or(0, |o| o.started_at_ms)
            };
            processes.push(Process {
                pid: e.pid,
                parent_pid: e.parent_pid,
                started_at_ms,
                session: e.session,
                image_path: path,
                image_name: (!e.name.is_empty()).then_some(e.name),
                description,
                rent_bytes: e.private_ws,
                lease_bytes: e.commit,
                working_set_bytes: e.ws,
                has_visible_window: visible.contains(&e.pid),
                is_self: e.pid == self_pid,
            });
        }

        // Rent is the private working set. When every process reports 0
        // there (as under Wine), use the working set, marked as estimated.
        let estimate = !processes.iter().any(|p| p.rent_bytes > 0);
        if estimate {
            for process in &mut processes {
                process.rent_bytes = process.working_set_bytes;
            }
        }

        // Sign-in time: the start of the oldest process in our session that
        // runs as our user. Session 1 exists from boot, with winlogon and
        // dwm in it, so the session alone would give the boot time. If the
        // owners cannot be read, the session alone it is.
        let own_session = processes.iter().find(|p| p.is_self).map(|p| p.session);
        let any_ours = ours.iter().filter(|&&o| o).count() > 1;
        let signed_in_at_ms = processes
            .iter()
            .zip(&ours)
            .filter(|&(p, &o)| {
                Some(p.session) == own_session && p.started_at_ms > 0 && (o || !any_ours)
            })
            .map(|(p, _)| p.started_at_ms)
            .min()
            .unwrap_or(at_ms);

        Ok(Snapshot {
            at_ms,
            capacity_bytes: capacity,
            available_bytes: perf.PhysicalAvailable as u64 * page,
            commit_used_bytes: perf.CommitTotal as u64 * page,
            commit_limit_bytes: perf.CommitLimit as u64 * page,
            signed_in_at_ms,
            foreground_pid,
            rent_is_estimate: estimate,
            processes,
            physical_total_bytes: physical_total,
            kernel_paged_bytes: perf.KernelPaged as u64 * page,
            kernel_nonpaged_bytes: perf.KernelNonpaged as u64 * page,
            system_cache_bytes: perf.SystemCache as u64 * page,
        })
    }
}
