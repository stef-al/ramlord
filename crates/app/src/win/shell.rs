//! The window: one frame scaled by a whole number with `StretchDIBits` and
//! `COLORONCOLOR`. It is a fixed size: the canvas times the largest scale
//! from 2 to 5 that fits 70% of its monitor's work area, chosen again when
//! the window moves to another monitor or the display changes. Per-monitor
//! DPI awareness v2 keeps the pixels square at any display scaling.
//!
//! Live, it samples every 2 seconds (5 while minimized), credits focus
//! every second, and redraws only when the frame changes. It saves
//! `state.json` every minute and when the window closes. Closing the
//! window ends the program; the tray comes with M5.
//!
//! Clicks and keys go to the core, which answers with commands: close or
//! terminate a tenant's processes, open Startup settings, save the Family
//! list. While an eviction waits, a timer polls the processes four times
//! a second.

use std::cell::RefCell;
use std::path::PathBuf;
use std::time::Instant;

use ramlord_core::Frame;
use ramlord_core::live::Live;
use ramlord_core::render::pick_scale;
use ramlord_core::ui::{Command, Input, Key};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BeginPaint, COLORONCOLOR, DIB_RGB_COLORS, EndPaint,
    GetMonitorInfoW, HMONITOR, InvalidateRect, MONITOR_DEFAULTTONEAREST, MONITOR_DEFAULTTOPRIMARY,
    MONITORINFO, MonitorFromPoint, MonitorFromRect, MonitorFromWindow, PAINTSTRUCT, SRCCOPY,
    SetStretchBltMode, StretchDIBits, UpdateWindow,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{
    AdjustWindowRectExForDpi, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForWindow,
    SetProcessDpiAwarenessContext,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    VIRTUAL_KEY, VK_DELETE, VK_DOWN, VK_ESCAPE, VK_F2, VK_F3, VK_RETURN, VK_UP,
};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::{
    AdjustWindowRectEx, CW_USEDEFAULT, CreateWindowExW, DefWindowProcW, DispatchMessageW,
    GetMessageW, IDC_ARROW, IsIconic, KillTimer, LoadCursorW, MSG, PostQuitMessage, RegisterClassW,
    SW_SHOW, SW_SHOWMINNOACTIVE, SW_SHOWNORMAL, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOZORDER, SetTimer,
    SetWindowPos, ShowWindow, TranslateMessage, WINDOW_EX_STYLE, WINDOW_STYLE, WM_DESTROY,
    WM_DISPLAYCHANGE, WM_DPICHANGED, WM_ERASEBKGND, WM_KEYDOWN, WM_LBUTTONDOWN, WM_PAINT, WM_TIMER,
    WNDCLASSW, WS_CAPTION, WS_MINIMIZEBOX, WS_OVERLAPPED, WS_SYSMENU,
};
use windows::core::w;

use super::evict::Evictor;
use super::provider::Provider;

/// Sampling intervals (SPEC.md, Timing).
const SAMPLE_MS: u32 = 2_000;
const SAMPLE_HIDDEN_MS: u32 = 5_000;
const TIMER_ID: usize = 1;
/// Focus is sampled every second (SPEC.md, Timing).
const FOCUS_TIMER_ID: usize = 2;
const FOCUS_MS: u32 = 1_000;
/// An eviction in progress is polled four times a second.
const EVICT_TIMER_ID: usize = 3;
const EVICT_MS: u32 = 250;
/// How often the state file is written while running.
const SAVE_EVERY_MS: u64 = 60_000;

/// A fixed size: no thick frame, no maximize box.
const STYLE: WINDOW_STYLE =
    WINDOW_STYLE(WS_OVERLAPPED.0 | WS_CAPTION.0 | WS_SYSMENU.0 | WS_MINIMIZEBOX.0);

/// What the window shows.
pub enum Content {
    /// One frame that never changes: `--demo`.
    Still(Frame),
    /// The PC itself. `state_path` is where `state.json` is saved,
    /// `settings_path` where the Family list is.
    Live {
        provider: Provider,
        live: Box<Live>,
        state_path: Option<PathBuf>,
        settings_path: Option<PathBuf>,
    },
}

struct State {
    content: Content,
    frame: Frame,
    scale: u32,
    interval_ms: u32,
    /// Sample time of the last save.
    saved_at_ms: u64,
    evictor: Evictor,
    /// The clock for the polite phase: steady, unlike the sample time.
    started: Instant,
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

fn work_area(monitor: HMONITOR) -> (u32, u32) {
    let mut info = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    // SAFETY: `cbSize` is set; the call fills `info`.
    if !unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
        return (0, 0);
    }
    let r = info.rcWork;
    (
        u32::try_from(r.right - r.left).unwrap_or(0),
        u32::try_from(r.bottom - r.top).unwrap_or(0),
    )
}

/// The outer window size for a client area of `w` by `h` at `dpi`.
fn window_size(hwnd: Option<HWND>, w: i32, h: i32) -> (i32, i32) {
    let mut rect = RECT {
        left: 0,
        top: 0,
        right: w,
        bottom: h,
    };
    // SAFETY: plain calls; `rect` is a live local.
    let ok =
        unsafe {
            match hwnd {
                Some(hwnd) => AdjustWindowRectExForDpi(
                    &mut rect,
                    STYLE,
                    false,
                    WINDOW_EX_STYLE::default(),
                    GetDpiForWindow(hwnd),
                )
                .is_ok(),
                None => false,
            }
        } || unsafe { AdjustWindowRectEx(&mut rect, STYLE, false, WINDOW_EX_STYLE::default()) }
            .is_ok();
    if ok {
        (rect.right - rect.left, rect.bottom - rect.top)
    } else {
        (w, h)
    }
}

/// Picks the scale for `monitor`, stores it and returns the client size.
fn rescale(monitor: HMONITOR) -> (i32, i32) {
    let (work_w, work_h) = work_area(monitor);
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        let Some(state) = s.as_mut() else {
            return (0, 0);
        };
        state.scale = pick_scale(state.frame.width, state.frame.height, work_w, work_h);
        (
            (state.frame.width * state.scale) as i32,
            (state.frame.height * state.scale) as i32,
        )
    })
}

/// Sizes the window for the monitor it is on.
fn fit(hwnd: HWND) {
    // SAFETY: plain query on our window.
    let monitor = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
    let (cw, ch) = rescale(monitor);
    let (w, h) = window_size(Some(hwnd), cw, ch);
    // SAFETY: resizes our own window; no state is borrowed here.
    unsafe {
        let _ = SetWindowPos(
            hwnd,
            None,
            0,
            0,
            w,
            h,
            SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
        );
        let _ = InvalidateRect(Some(hwnd), None, false);
    }
}

/// Opens the window and returns when it closes.
pub fn show(content: Content, minimized: bool) -> Result<(), String> {
    // Fails under Wine; that is harmless.
    // SAFETY: plain call with a predefined context value.
    let _ = unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };

    let mut content = content;
    let frame = match &mut content {
        Content::Still(frame) => frame.clone(),
        Content::Live { provider, live, .. } => {
            let snapshot = provider.sample()?;
            live.update(&snapshot).map_err(|e| e.to_string())?;
            live.frame().cloned().ok_or("no frame")?
        }
    };
    let live = matches!(content, Content::Live { .. });
    STATE.with(|s| {
        *s.borrow_mut() = Some(State {
            content,
            frame,
            scale: 2,
            interval_ms: SAMPLE_MS,
            saved_at_ms: 0,
            evictor: Evictor::default(),
            started: Instant::now(),
        })
    });
    // A first size from the primary monitor; `fit` corrects it once the
    // window knows its own monitor and DPI.
    // SAFETY: plain query.
    let primary = unsafe { MonitorFromPoint(POINT::default(), MONITOR_DEFAULTTOPRIMARY) };
    let (cw, ch) = rescale(primary);
    let (w, h) = window_size(None, cw, ch);

    let class = w!("RAMlordWindow");
    // SAFETY: standard Win32 window creation and message loop on this
    // thread. Every pointer passed points to a live local for the call.
    unsafe {
        let instance = GetModuleHandleW(None).map_err(|e| format!("GetModuleHandleW: {e}"))?;
        let wc = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: instance.into(),
            lpszClassName: class,
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            ..Default::default()
        };
        if RegisterClassW(&wc) == 0 {
            return Err("RegisterClassW failed".into());
        }
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            class,
            w!("RAMlord"),
            STYLE,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            w,
            h,
            None,
            None,
            Some(instance.into()),
            None,
        )
        .map_err(|e| format!("CreateWindowExW: {e}"))?;
        fit(hwnd);
        let _ = ShowWindow(
            hwnd,
            if minimized {
                SW_SHOWMINNOACTIVE
            } else {
                SW_SHOW
            },
        );
        let _ = UpdateWindow(hwnd);
        if live {
            SetTimer(Some(hwnd), TIMER_ID, SAMPLE_MS, None);
            SetTimer(Some(hwnd), FOCUS_TIMER_ID, FOCUS_MS, None);
        }

        let mut msg = MSG::default();
        loop {
            match GetMessageW(&mut msg, None, 0, 0).0 {
                0 => break,
                -1 => return Err("GetMessageW failed".into()),
                _ => {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
        }
    }
    Ok(())
}

/// One sample: draw it, and repaint only if the frame changed.
fn tick(hwnd: HWND) {
    // SAFETY: plain query on our window.
    let hidden = unsafe { IsIconic(hwnd) }.as_bool();
    let want = if hidden { SAMPLE_HIDDEN_MS } else { SAMPLE_MS };
    let (changed, retime) = STATE.with(|s| {
        let mut s = s.borrow_mut();
        let Some(state) = s.as_mut() else {
            return (false, false);
        };
        let retime = state.interval_ms != want;
        state.interval_ms = want;
        let Content::Live {
            provider,
            live,
            state_path,
            ..
        } = &mut state.content
        else {
            return (false, retime);
        };
        // A failed sample keeps the last frame; the next tick tries again.
        let Ok(snapshot) = provider.sample() else {
            return (false, retime);
        };
        let updated = live.update(&snapshot);
        if snapshot.at_ms.abs_diff(state.saved_at_ms) >= SAVE_EVERY_MS {
            if let Some(path) = state_path {
                // A failed save is retried at the next one.
                let _ = crate::store::save(path, live.state());
            }
            state.saved_at_ms = snapshot.at_ms;
        }
        match updated {
            Ok(true) => {
                if let Some(frame) = live.frame() {
                    state.frame = frame.clone();
                }
                (true, retime)
            }
            _ => (false, retime),
        }
    });
    // SAFETY: timer and repaint requests on our own window.
    unsafe {
        if retime {
            SetTimer(Some(hwnd), TIMER_ID, want, None);
        }
        if changed {
            let _ = InvalidateRect(Some(hwnd), None, false);
        }
    }
}

/// What a click or a key asks of the Windows layer, outside the borrow of
/// the state.
#[derive(Default)]
struct After {
    repaint: bool,
    /// Whether an eviction now waits on its processes.
    waiting: bool,
    startup_settings: bool,
}

/// Runs `step` on the live building with the evictor and a steady clock,
/// then copies a changed frame.
fn with_live(
    step: impl FnOnce(&mut Live, &mut Evictor, &Option<PathBuf>, u64, &mut After),
) -> After {
    STATE.with(|s| {
        let mut after = After::default();
        let Ok(mut s) = s.try_borrow_mut() else {
            return after;
        };
        let Some(State {
            content:
                Content::Live {
                    live,
                    settings_path,
                    ..
                },
            frame,
            evictor,
            started,
            ..
        }) = s.as_mut()
        else {
            return after;
        };
        let now_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        step(live, evictor, settings_path, now_ms, &mut after);
        after.waiting = live.waiting();
        if !after.waiting {
            evictor.clear();
        }
        if let Some(new) = live.frame()
            && new != frame
        {
            *frame = new.clone();
            after.repaint = true;
        }
        after
    })
}

/// A click or a key: the core decides, this carries out its commands.
fn input(hwnd: HWND, input: Input) {
    let after = with_live(|live, evictor, settings_path, now_ms, after| {
        for command in live.input(input, now_ms) {
            match command {
                Command::Close { pids } => {
                    live.report(evictor.close(&pids));
                }
                Command::Terminate { pids } => {
                    if let Some(report) = evictor.terminate(&pids) {
                        live.report(report);
                    }
                }
                Command::OpenStartupSettings => after.startup_settings = true,
                Command::SaveFamily(family) => {
                    if let Some(path) = settings_path {
                        // A failed save leaves the list in memory for this run.
                        let _ = crate::store::save_family(path, &family);
                    }
                }
                // The rent roll card and the settings screen come with M5.
                Command::RentRollCard | Command::Settings => {}
            }
        }
    });
    finish(hwnd, &after);
}

/// The eviction timer: which of the processes remain.
fn poll(hwnd: HWND) {
    let after = with_live(|live, evictor, _, now_ms, _| {
        live.poll(&evictor.remaining(), now_ms);
    });
    finish(hwnd, &after);
}

fn finish(hwnd: HWND, after: &After) {
    // SAFETY: timer and repaint requests on our own window; ShellExecuteW
    // gets static strings.
    unsafe {
        if after.waiting {
            SetTimer(Some(hwnd), EVICT_TIMER_ID, EVICT_MS, None);
        } else {
            let _ = KillTimer(Some(hwnd), EVICT_TIMER_ID);
        }
        if after.repaint {
            let _ = InvalidateRect(Some(hwnd), None, false);
        }
        if after.startup_settings {
            ShellExecuteW(
                Some(hwnd),
                w!("open"),
                w!("ms-settings:startupapps"),
                None,
                None,
                SW_SHOWNORMAL,
            );
        }
    }
}

fn key(code: u16) -> Option<Key> {
    Some(match VIRTUAL_KEY(code) {
        VK_UP => Key::Up,
        VK_DOWN => Key::Down,
        VK_RETURN => Key::Enter,
        VK_DELETE => Key::Delete,
        VK_ESCAPE => Key::Escape,
        VK_F2 => Key::F2,
        VK_F3 => Key::F3,
        _ => return None,
    })
}

/// A client-area point in canvas pixels.
fn canvas_point(lparam: LPARAM) -> Option<(i32, i32)> {
    // GET_X_LPARAM and GET_Y_LPARAM: signed words.
    let x = i32::from(lparam.0 as u16 as i16);
    let y = i32::from((lparam.0 >> 16) as u16 as i16);
    let scale = STATE.with(|s| s.try_borrow().ok()?.as_ref().map(|state| state.scale))?;
    let scale = i32::try_from(scale).ok().filter(|&s| s > 0)?;
    Some((x.div_euclid(scale), y.div_euclid(scale)))
}

/// Credits focus to the foreground window's process.
fn focus() {
    let Some(pid) = super::provider::foreground_pid() else {
        return;
    };
    STATE.with(|s| {
        if let Ok(mut s) = s.try_borrow_mut()
            && let Some(State {
                content: Content::Live { live, .. },
                ..
            }) = s.as_mut()
        {
            live.focus(pid);
        }
    });
}

/// Writes `state.json`, if the window is live.
fn save() {
    STATE.with(|s| {
        if let Ok(s) = s.try_borrow()
            && let Some(State {
                content:
                    Content::Live {
                        live,
                        state_path: Some(path),
                        ..
                    },
                ..
            }) = s.as_ref()
        {
            let _ = crate::store::save(path, live.state());
        }
    });
}

extern "system" fn window_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => {
            paint(hwnd);
            LRESULT(0)
        }
        WM_TIMER => {
            match wparam.0 {
                FOCUS_TIMER_ID => focus(),
                EVICT_TIMER_ID => poll(hwnd),
                _ => tick(hwnd),
            }
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            if let Some((x, y)) = canvas_point(lparam) {
                input(hwnd, Input::Click { x, y });
            }
            LRESULT(0)
        }
        WM_KEYDOWN => match key(wparam.0 as u16) {
            Some(k) => {
                // Bit 30: the key was already down. A held key repeats
                // Up and Down, but never answers a dialog.
                let repeat = lparam.0 & (1 << 30) != 0;
                if !(repeat && matches!(k, Key::Enter | Key::Delete)) {
                    input(hwnd, Input::Key(k));
                }
                LRESULT(0)
            }
            // SAFETY: the default handling for other keys.
            None => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
        },
        // Moved to a monitor with another DPI: keep the suggested position,
        // pick the scale for that monitor.
        WM_DPICHANGED => {
            // SAFETY: for WM_DPICHANGED, lparam points to the suggested RECT.
            let suggested = unsafe { *(lparam.0 as *const RECT) };
            // SAFETY: plain query.
            let monitor = unsafe { MonitorFromRect(&suggested, MONITOR_DEFAULTTONEAREST) };
            let (cw, ch) = rescale(monitor);
            let (w, h) = window_size(Some(hwnd), cw, ch);
            // SAFETY: moves and resizes our own window.
            unsafe {
                let _ = SetWindowPos(
                    hwnd,
                    None,
                    suggested.left,
                    suggested.top,
                    w,
                    h,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
                let _ = InvalidateRect(Some(hwnd), None, false);
            }
            LRESULT(0)
        }
        WM_DISPLAYCHANGE => {
            fit(hwnd);
            LRESULT(0)
        }
        // The frame covers the whole client area; skipping the erase
        // avoids a flash of background.
        WM_ERASEBKGND => LRESULT(1),
        WM_DESTROY => {
            save();
            // SAFETY: ends this thread's message loop.
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
        // SAFETY: the default handling for every other message.
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

fn paint(hwnd: HWND) {
    let mut ps = PAINTSTRUCT::default();
    // SAFETY: BeginPaint and EndPaint pair up on the same PAINTSTRUCT.
    let hdc = unsafe { BeginPaint(hwnd, &mut ps) };
    STATE.with(|s| {
        let Ok(s) = s.try_borrow() else { return };
        let Some(state) = s.as_ref() else { return };
        let frame = &state.frame;
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: frame.width as i32,
                // Negative: rows run top-down, as in `Frame`.
                biHeight: -(frame.height as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        // SAFETY: `frame.pixels` holds width * height 32-bit pixels in the
        // top-down BGRX layout `info` describes, and outlives the call.
        unsafe {
            SetStretchBltMode(hdc, COLORONCOLOR);
            StretchDIBits(
                hdc,
                0,
                0,
                (frame.width * state.scale) as i32,
                (frame.height * state.scale) as i32,
                0,
                0,
                frame.width as i32,
                frame.height as i32,
                Some(frame.pixels.as_ptr().cast()),
                &info,
                DIB_RGB_COLORS,
                SRCCOPY,
            );
        }
    });
    // SAFETY: ends the paint begun above.
    unsafe {
        let _ = EndPaint(hwnd, &ps);
    }
}
