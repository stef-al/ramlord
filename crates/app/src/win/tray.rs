//! The tray icon (SPEC.md, Window and tray): a small tower that fills with
//! occupancy and turns red when the building is full, drawn by the core.
//! A left click opens the window; the menu offers Open, Rent roll and Quit.

use ramlord_core::palette::TRAY_CLEAR;
use windows::Win32::Foundation::{HWND, LPARAM, POINT, WPARAM};
use windows::Win32::Graphics::Gdi::{CreateBitmap, DeleteObject, HGDIOBJ};
use windows::Win32::UI::Shell::{
    NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW,
    Shell_NotifyIconW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreateIconIndirect, CreatePopupMenu, DestroyIcon, DestroyMenu, GetCursorPos,
    GetSystemMetrics, HICON, ICONINFO, MF_SEPARATOR, MF_STRING, PostMessageW, SM_CXSMICON,
    SetForegroundWindow, TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenu, WM_APP, WM_NULL,
};
use windows::core::{BOOL, w};

/// The message the tray icon sends to the window.
pub const WM_TRAY: u32 = WM_APP + 1;

/// What the tray menu chose.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    Open,
    RentRoll,
    Quit,
}

/// The icon in the notification area, kept in step with the building.
pub struct Tray {
    hwnd: HWND,
    icon: Option<HICON>,
    shown: Option<(u64, bool)>,
    added: bool,
}

/// The icon for `occupancy` percent, at the small-icon size Windows asks
/// for (16 pixels at 100%, a whole multiple above).
fn make_icon(occupancy: u64, full: bool) -> Option<HICON> {
    // SAFETY: plain query.
    let wanted = unsafe { GetSystemMetrics(SM_CXSMICON) }.max(16) as u32;
    let frame = ramlord_core::render_tray_icon(occupancy, full).scaled((wanted / 16).max(1));
    let (w, h) = (frame.width as usize, frame.height as usize);
    // Monochrome rows are padded to 16 bits; a set bit lets the taskbar
    // show through.
    let stride = w.div_ceil(16) * 2;
    let mut mask = vec![0u8; stride * h];
    let mut color = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        for x in 0..w {
            let p = frame.pixels[y * w + x];
            if p == TRAY_CLEAR {
                mask[y * stride + x / 8] |= 0x80 >> (x % 8);
                color.extend([0, 0, 0, 0]);
            } else {
                color.extend([p as u8, (p >> 8) as u8, (p >> 16) as u8, 0xff]);
            }
        }
    }
    // SAFETY: both bit buffers hold w by h pixels in the layouts the calls
    // expect and outlive them; the bitmaps are deleted once the icon has
    // copied them.
    unsafe {
        let mask_bitmap = CreateBitmap(w as i32, h as i32, 1, 1, Some(mask.as_ptr().cast()));
        let color_bitmap = CreateBitmap(w as i32, h as i32, 1, 32, Some(color.as_ptr().cast()));
        let info = ICONINFO {
            fIcon: BOOL(1),
            xHotspot: 0,
            yHotspot: 0,
            hbmMask: mask_bitmap,
            hbmColor: color_bitmap,
        };
        let icon = CreateIconIndirect(&info).ok();
        let _ = DeleteObject(HGDIOBJ(mask_bitmap.0));
        let _ = DeleteObject(HGDIOBJ(color_bitmap.0));
        icon
    }
}

impl Tray {
    pub fn new(hwnd: HWND) -> Tray {
        Tray {
            hwnd,
            icon: None,
            shown: None,
            added: false,
        }
    }

    /// Whether the icon is in the notification area. Without it, closing
    /// the window must quit, or RAMlord could not be reached again.
    pub fn added(&self) -> bool {
        self.added
    }

    fn notify(&self, message: windows::Win32::UI::Shell::NOTIFY_ICON_MESSAGE, tip: &str) -> bool {
        let mut data = NOTIFYICONDATAW {
            cbSize: size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: self.hwnd,
            uID: 1,
            uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
            uCallbackMessage: WM_TRAY,
            hIcon: self.icon.unwrap_or_default(),
            ..Default::default()
        };
        for (slot, unit) in data.szTip.iter_mut().zip(tip.encode_utf16().take(127)) {
            *slot = unit;
        }
        // SAFETY: `data` is filled and sized for the call.
        unsafe { Shell_NotifyIconW(message, &data) }.as_bool()
    }

    /// Shows the building's occupancy; a new icon only when it changed.
    pub fn update(&mut self, occupancy: u64, full: bool) {
        if self.added && self.shown == Some((occupancy, full)) {
            return;
        }
        let Some(icon) = make_icon(occupancy, full) else {
            return;
        };
        let old = self.icon.replace(icon);
        self.shown = Some((occupancy, full));
        let tip = format!(
            "RAMlord: {occupancy}% occupied{}",
            if full { ", full" } else { "" }
        );
        if self.added {
            self.notify(NIM_MODIFY, &tip);
        } else {
            self.added = self.notify(NIM_ADD, &tip);
        }
        if let Some(old) = old {
            // SAFETY: an icon this tray made, no longer shown.
            unsafe {
                let _ = DestroyIcon(old);
            }
        }
    }

    /// After Explorer restarts ("TaskbarCreated"), the icon must be added
    /// again.
    pub fn readd(&mut self) {
        self.added = false;
        if let Some((occupancy, full)) = self.shown.take() {
            self.update(occupancy, full);
        }
    }

    pub fn remove(&mut self) {
        if self.added {
            self.notify(NIM_DELETE, "");
            self.added = false;
        }
        if let Some(icon) = self.icon.take() {
            // SAFETY: an icon this tray made.
            unsafe {
                let _ = DestroyIcon(icon);
            }
        }
    }
}

/// The tray menu at the cursor: Open, Rent roll, Quit.
pub fn menu(hwnd: HWND) -> Option<Choice> {
    // SAFETY: a menu made, shown and destroyed here; the strings are static.
    unsafe {
        let menu = CreatePopupMenu().ok()?;
        let _ = AppendMenuW(menu, MF_STRING, 1, w!("Open"));
        let _ = AppendMenuW(menu, MF_STRING, 2, w!("Rent roll"));
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, None);
        let _ = AppendMenuW(menu, MF_STRING, 3, w!("Quit"));
        let mut at = POINT::default();
        let _ = GetCursorPos(&mut at);
        // The menu closes when the user clicks elsewhere only if the
        // window is in front (a known Win32 rule for tray menus).
        let _ = SetForegroundWindow(hwnd);
        let chosen = TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_RIGHTBUTTON,
            at.x,
            at.y,
            None,
            hwnd,
            None,
        );
        let _ = PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0));
        let _ = DestroyMenu(menu);
        match chosen.0 {
            1 => Some(Choice::Open),
            2 => Some(Choice::RentRoll),
            3 => Some(Choice::Quit),
            _ => None,
        }
    }
}
