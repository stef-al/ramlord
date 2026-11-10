//! The window: one frame scaled by a whole number with `StretchDIBits`
//! and `COLORONCOLOR`. M1 shows a still frame and takes no input; closing
//! the window ends the program.

use std::sync::OnceLock;

use ramlord_core::Frame;
use ramlord_core::render::pick_scale;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BeginPaint, COLORONCOLOR, DIB_RGB_COLORS, EndPaint,
    PAINTSTRUCT, SRCCOPY, SetStretchBltMode, StretchDIBits, UpdateWindow,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AdjustWindowRectEx, CW_USEDEFAULT, CreateWindowExW, DefWindowProcW, DispatchMessageW,
    GetMessageW, IDC_ARROW, LoadCursorW, MSG, PostQuitMessage, RegisterClassW, SPI_GETWORKAREA,
    SW_SHOW, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, ShowWindow, SystemParametersInfoW,
    TranslateMessage, WINDOW_EX_STYLE, WM_DESTROY, WM_ERASEBKGND, WM_PAINT, WNDCLASSW, WS_CAPTION,
    WS_MINIMIZEBOX, WS_OVERLAPPED, WS_SYSMENU,
};
use windows::core::w;

/// What the window shows. Set once before the window opens.
struct Picture {
    frame: Frame,
    scale: u32,
}

static PICTURE: OnceLock<Picture> = OnceLock::new();

/// The work area of the primary monitor in pixels, or zero if unknown.
fn work_area() -> (u32, u32) {
    let mut area = RECT::default();
    // SAFETY: SPI_GETWORKAREA writes one RECT to the pointer we pass.
    let ok = unsafe {
        SystemParametersInfoW(
            SPI_GETWORKAREA,
            0,
            Some((&raw mut area).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS::default(),
        )
    };
    match ok {
        Ok(()) => (
            u32::try_from(area.right - area.left).unwrap_or(0),
            u32::try_from(area.bottom - area.top).unwrap_or(0),
        ),
        Err(_) => (0, 0),
    }
}

/// Opens a fixed-size window showing `frame` at the largest whole scale
/// from 2 to 5 that fits 70% of the work area, and returns when it closes.
pub fn show(frame: Frame) -> Result<(), String> {
    // Per-monitor DPI awareness v2 keeps the pixels square. It fails under
    // Wine; that is harmless.
    // SAFETY: plain call with a predefined context value.
    let _ = unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };

    let (work_w, work_h) = work_area();
    let scale = pick_scale(frame.width, frame.height, work_w, work_h);
    let client_w = (frame.width * scale) as i32;
    let client_h = (frame.height * scale) as i32;
    PICTURE
        .set(Picture { frame, scale })
        .map_err(|_| "the window is already open".to_string())?;

    // A fixed size: no thick frame, no maximize box.
    let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX;
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
        let mut rect = RECT {
            left: 0,
            top: 0,
            right: client_w,
            bottom: client_h,
        };
        AdjustWindowRectEx(&mut rect, style, false, WINDOW_EX_STYLE::default())
            .map_err(|e| format!("AdjustWindowRectEx: {e}"))?;
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            class,
            w!("RAMlord"),
            style,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            rect.right - rect.left,
            rect.bottom - rect.top,
            None,
            None,
            Some(instance.into()),
            None,
        )
        .map_err(|e| format!("CreateWindowExW: {e}"))?;
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = UpdateWindow(hwnd);

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

extern "system" fn window_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => {
            paint(hwnd);
            LRESULT(0)
        }
        // The frame covers the whole client area; skipping the erase
        // avoids a flash of background.
        WM_ERASEBKGND => LRESULT(1),
        WM_DESTROY => {
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
    if let Some(picture) = PICTURE.get() {
        let frame = &picture.frame;
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
                (frame.width * picture.scale) as i32,
                (frame.height * picture.scale) as i32,
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
    }
    // SAFETY: ends the paint begun above.
    unsafe {
        let _ = EndPaint(hwnd, &ps);
    }
}
