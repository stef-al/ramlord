//! The small Windows calls of M5 (SPEC.md, The Windows layer: extras): the
//! memory type from SMBIOS, the price file over WinHTTP, start with
//! Windows (the per-user Run key), the clipboard, the Pictures folder and
//! the local time for the card's file name. Each fails softly.

use std::path::PathBuf;

use ramlord_core::smbios::{self, MemoryType};
use windows::Win32::Foundation::GlobalFree;
use windows::Win32::Foundation::{ERROR_SUCCESS, HANDLE, HGLOBAL, HWND};
use windows::Win32::Networking::WinHttp::{
    WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, WINHTTP_ACCESS_TYPE_DEFAULT_PROXY, WINHTTP_FLAG_SECURE,
    WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_QUERY_STATUS_CODE, WinHttpCloseHandle, WinHttpConnect,
    WinHttpOpen, WinHttpOpenRequest, WinHttpQueryHeaders, WinHttpReadData, WinHttpReceiveResponse,
    WinHttpSendRequest, WinHttpSetTimeouts,
};
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
};
use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};
use windows::Win32::System::Registry::{
    HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ, RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW,
};
use windows::Win32::System::SystemInformation::{
    FIRMWARE_TABLE_PROVIDER, GetLocalTime, GetSystemFirmwareTable,
};
use windows::Win32::UI::Shell::{FOLDERID_Pictures, KF_FLAG_DEFAULT, SHGetKnownFolderPath};
use windows::core::{PCWSTR, w};

/// The memory type the firmware reports, if it reports one RAMlord knows.
pub fn memory_type() -> Option<MemoryType> {
    let rsmb = FIRMWARE_TABLE_PROVIDER(u32::from_be_bytes(*b"RSMB"));
    // SAFETY: a size query, then a fill of a buffer of that size.
    let size = unsafe { GetSystemFirmwareTable(rsmb, 0, None) };
    if size == 0 {
        return None;
    }
    let mut buffer = vec![0u8; size as usize];
    let written = unsafe { GetSystemFirmwareTable(rsmb, 0, Some(&mut buffer)) };
    buffer.truncate(written as usize);
    smbios::memory_type(&buffer)
}

/// A WinHTTP handle that closes itself.
struct Internet(*mut core::ffi::c_void);

impl Drop for Internet {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: a handle WinHTTP gave, closed once.
            unsafe {
                let _ = WinHttpCloseHandle(self.0);
            }
        }
    }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain([0]).collect()
}

/// GETs `https://host/path` and returns the body, at most 64 KB. Blocking:
/// run it on a thread of its own.
pub fn fetch(host: &str, path: &str) -> Result<String, String> {
    const MAX: usize = 64 * 1024;
    const TIMEOUT_MS: i32 = 10_000;
    let (host, path) = (wide(host), wide(path));
    // SAFETY: plain WinHTTP calls; every handle is closed by `Internet`,
    // and every buffer outlives the call that fills it.
    unsafe {
        // A generic agent name: nothing about the user or the PC. The
        // proxy is found the way the system's own settings say (a company
        // PC often has no direct way out); where that is not offered, the
        // WinHTTP default.
        let open = |access| {
            Internet(WinHttpOpen(
                w!("RAMlord"),
                access,
                PCWSTR::null(),
                PCWSTR::null(),
                0,
            ))
        };
        let mut session = open(WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY);
        if session.0.is_null() {
            session = open(WINHTTP_ACCESS_TYPE_DEFAULT_PROXY);
        }
        if session.0.is_null() {
            return Err("WinHttpOpen failed".into());
        }
        // Without limits a name that never resolves would hold this thread
        // for good: ten seconds for each step.
        let _ = WinHttpSetTimeouts(session.0, TIMEOUT_MS, TIMEOUT_MS, TIMEOUT_MS, TIMEOUT_MS);
        let connection = Internet(WinHttpConnect(session.0, PCWSTR(host.as_ptr()), 443, 0));
        if connection.0.is_null() {
            return Err("WinHttpConnect failed".into());
        }
        let request = Internet(WinHttpOpenRequest(
            connection.0,
            w!("GET"),
            PCWSTR(path.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            std::ptr::null(),
            WINHTTP_FLAG_SECURE,
        ));
        if request.0.is_null() {
            return Err("WinHttpOpenRequest failed".into());
        }
        WinHttpSendRequest(request.0, None, None, 0, 0, 0)
            .map_err(|e| format!("WinHttpSendRequest: {e}"))?;
        WinHttpReceiveResponse(request.0, std::ptr::null_mut())
            .map_err(|e| format!("WinHttpReceiveResponse: {e}"))?;
        let mut status = 0u32;
        let mut length = size_of::<u32>() as u32;
        WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            PCWSTR::null(),
            Some((&raw mut status).cast()),
            &mut length,
            std::ptr::null_mut(),
        )
        .map_err(|e| format!("WinHttpQueryHeaders: {e}"))?;
        if status != 200 {
            return Err(format!("HTTP {status}"));
        }
        let mut body = Vec::new();
        let mut chunk = [0u8; 4096];
        loop {
            let mut read = 0u32;
            WinHttpReadData(
                request.0,
                chunk.as_mut_ptr().cast(),
                chunk.len() as u32,
                &mut read,
            )
            .map_err(|e| format!("WinHttpReadData: {e}"))?;
            if read == 0 {
                break;
            }
            body.extend_from_slice(&chunk[..read as usize]);
            if body.len() > MAX {
                return Err("the price file is too large".into());
            }
        }
        String::from_utf8(body).map_err(|_| "the price file is not UTF-8".into())
    }
}

const RUN_KEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const RUN_VALUE: PCWSTR = w!("RAMlord");

/// What the Run value says for this exe: `"<this exe>" --minimized`.
fn run_command() -> Result<String, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    Ok(format!("\"{}\" --minimized", exe.display()))
}

/// RAMlord's value in the per-user Run key, if it has one.
fn run_value() -> Option<String> {
    let mut bytes = 0u32;
    // SAFETY: a size query of one string value.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            RUN_KEY,
            RUN_VALUE,
            RRF_RT_REG_SZ,
            None,
            None,
            Some(&mut bytes),
        )
    };
    if status != ERROR_SUCCESS {
        return None;
    }
    let mut text = vec![0u16; (bytes as usize).div_ceil(2) + 1];
    let mut bytes = (text.len() * 2) as u32;
    // SAFETY: `text` holds `bytes` bytes, as the call is told.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            RUN_KEY,
            RUN_VALUE,
            RRF_RT_REG_SZ,
            None,
            Some(text.as_mut_ptr().cast()),
            Some(&mut bytes),
        )
    };
    if status != ERROR_SUCCESS {
        return None;
    }
    let end = text.iter().position(|&u| u == 0).unwrap_or(text.len());
    Some(String::from_utf16_lossy(&text[..end]))
}

/// Whether RAMlord starts with Windows: its value in the per-user Run key.
pub fn autostart() -> bool {
    run_value().is_some()
}

/// Keeps the Run value pointing at this exe. RAMlord is one file a person
/// may move; the value would then start nothing while the setting still
/// read ON. Does nothing when start with Windows is off.
pub fn refresh_autostart() {
    if let (Some(value), Ok(command)) = (run_value(), run_command())
        && !value.eq_ignore_ascii_case(&command)
    {
        let _ = set_autostart(true);
    }
}

/// Adds `"<this exe>" --minimized` to the per-user Run key, or removes it.
pub fn set_autostart(on: bool) -> Result<(), String> {
    if !on {
        // SAFETY: deletes one value under the user's own key.
        let status = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN_KEY, RUN_VALUE) };
        return match status {
            s if s == ERROR_SUCCESS => Ok(()),
            s if s.0 == 2 => Ok(()), // ERROR_FILE_NOT_FOUND: already gone
            s => Err(format!("RegDeleteKeyValueW: {}", s.0)),
        };
    }
    let command = wide(&run_command()?);
    // SAFETY: writes one string value under the user's own key; `command`
    // is NUL-terminated and outlives the call.
    let status = unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            RUN_KEY,
            RUN_VALUE,
            REG_SZ.0,
            Some(command.as_ptr().cast()),
            (command.len() * 2) as u32,
        )
    };
    if status == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(format!("RegSetKeyValueW: {}", status.0))
    }
}

/// Puts a device-independent bitmap (`output::clipboard_dib`) on the
/// clipboard as CF_DIB, which chat apps and editors paste.
pub fn copy_image(hwnd: HWND, dib: &[u8]) -> Result<(), String> {
    const CF_DIB: u32 = 8;
    // SAFETY: the memory is allocated, filled within its size and handed to
    // the clipboard, which then owns it; on any failure it is freed here.
    unsafe {
        let memory: HGLOBAL =
            GlobalAlloc(GMEM_MOVEABLE, dib.len()).map_err(|e| format!("GlobalAlloc: {e}"))?;
        let at = GlobalLock(memory).cast::<u8>();
        if at.is_null() {
            let _ = GlobalFree(Some(memory));
            return Err("GlobalLock failed".into());
        }
        std::ptr::copy_nonoverlapping(dib.as_ptr(), at, dib.len());
        let _ = GlobalUnlock(memory);
        if OpenClipboard(Some(hwnd)).is_err() {
            let _ = GlobalFree(Some(memory));
            return Err("OpenClipboard failed".into());
        }
        let _ = EmptyClipboard();
        let result = SetClipboardData(CF_DIB, Some(HANDLE(memory.0)));
        let _ = CloseClipboard();
        if let Err(e) = result {
            let _ = GlobalFree(Some(memory));
            return Err(format!("SetClipboardData: {e}"));
        }
    }
    Ok(())
}

/// The user's Pictures folder.
pub fn pictures() -> Option<PathBuf> {
    // SAFETY: the returned string is copied, then freed with CoTaskMemFree
    // as the call requires.
    unsafe {
        let path = SHGetKnownFolderPath(&FOLDERID_Pictures, KF_FLAG_DEFAULT, None).ok()?;
        let text = path.to_string().ok();
        CoTaskMemFree(Some(path.0 as *const _));
        text.map(PathBuf::from)
    }
}

/// The local date and time: year, month, day, hour, minute, second.
pub fn local_time() -> (u16, u16, u16, u16, u16, u16) {
    // SAFETY: plain query.
    let t = unsafe { GetLocalTime() };
    (t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond)
}
