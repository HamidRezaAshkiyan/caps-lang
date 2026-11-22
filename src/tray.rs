use std::mem::{size_of, zeroed};
use std::ptr;

use windows_sys::Win32::Foundation::{HWND, POINT};
use windows_sys::Win32::Graphics::Gdi::{CreateBitmap, DeleteObject};
use windows_sys::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY,
    NOTIFYICONDATAW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreateIconIndirect, CreatePopupMenu, DestroyIcon, DestroyMenu, GetCursorPos,
    PostMessageW, SetForegroundWindow, TrackPopupMenu, HICON, ICONINFO, MF_CHECKED, MF_GRAYED,
    MF_POPUP, MF_SEPARATOR, MF_STRING, MF_UNCHECKED, TPM_BOTTOMALIGN, TPM_LEFTALIGN,
    TPM_RIGHTBUTTON, WM_NULL, WM_USER,
};

use crate::config::{to_wide, Config, SwitchMethod, HOLD_PRESETS};

pub const WM_TRAY_ICON: u32 = WM_USER + 1;
const TRAY_UID: u32 = 1;

pub const IDM_PAUSE: usize = 1001;
pub const IDM_STARTUP: usize = 1002;
pub const IDM_EXIT: usize = 1003;
pub const IDM_SHOW_OSD: usize = 1004;
pub const IDM_HOLD_BASE: usize = 2000;
pub const IDM_METHOD_WIN_SPACE: usize = 3001;
pub const IDM_METHOD_ALT_SHIFT: usize = 3002;

pub struct TrayIcon {
    hwnd: HWND,
    active_icon: HICON,
    paused_icon: HICON,
}

impl TrayIcon {
    pub fn new(hwnd: HWND, paused: bool, hold_ms: u32) -> Self {
        let active_icon = create_badge_icon(false);
        let paused_icon = create_badge_icon(true);

        let tray = Self {
            hwnd,
            active_icon,
            paused_icon,
        };
        tray.add(paused, hold_ms);
        tray
    }

    pub fn add(&self, paused: bool, hold_ms: u32) {
        let nid = self.build_nid(paused, hold_ms);
        unsafe {
            Shell_NotifyIconW(NIM_ADD, &nid);
        }
    }

    pub fn update(&self, paused: bool, hold_ms: u32) {
        let nid = self.build_nid(paused, hold_ms);
        unsafe {
            Shell_NotifyIconW(NIM_MODIFY, &nid);
        }
    }

    fn build_nid(&self, paused: bool, hold_ms: u32) -> NOTIFYICONDATAW {
        let mut nid: NOTIFYICONDATAW = unsafe { zeroed() };
        nid.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = self.hwnd;
        nid.uID = TRAY_UID;
        nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
        nid.uCallbackMessage = WM_TRAY_ICON;
        nid.hIcon = if paused {
            self.paused_icon
        } else {
            self.active_icon
        };

        let tip = if paused {
            format!("CapsLang v{} (Paused)", env!("CARGO_PKG_VERSION"))
        } else {
            format!(
                "CapsLang v{} — Tap: Language | Hold ({hold_ms}ms): Caps Lock",
                env!("CARGO_PKG_VERSION")
            )
        };
        let wide = to_wide(&tip);
        let copy_len = wide.len().min(nid.szTip.len());
        nid.szTip[..copy_len].copy_from_slice(&wide[..copy_len]);
        nid.szTip[nid.szTip.len() - 1] = 0;

        nid
    }
}

impl Drop for TrayIcon {
    fn drop(&mut self) {
        unsafe {
            let mut nid: NOTIFYICONDATAW = zeroed();
            nid.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
            nid.hWnd = self.hwnd;
            nid.uID = TRAY_UID;
            Shell_NotifyIconW(NIM_DELETE, &nid);

            if !self.active_icon.is_null() {
                DestroyIcon(self.active_icon);
            }
            if !self.paused_icon.is_null() {
                DestroyIcon(self.paused_icon);
            }
        }
    }
}

pub fn show_context_menu(hwnd: HWND, paused: bool, config: Config, startup_enabled: bool) {
    unsafe {
        let menu = CreatePopupMenu();
        if menu.is_null() {
            return;
        }

        // Status header
        let status_label = if paused {
            format!("CapsLang v{} — Paused", env!("CARGO_PKG_VERSION"))
        } else {
            format!(
                "CapsLang v{} — Active ({} ms)",
                env!("CARGO_PKG_VERSION"),
                config.hold_duration_ms
            )
        };
        append_item(menu, MF_STRING | MF_GRAYED, 0, &status_label);
        AppendMenuW(menu, MF_SEPARATOR, 0, ptr::null());

        // Pause toggle
        let pause_flags = MF_STRING | if paused { MF_CHECKED } else { MF_UNCHECKED };
        append_item(menu, pause_flags, IDM_PAUSE, "Pause");

        // Hold duration submenu
        let hold_menu = CreatePopupMenu();
        if !hold_menu.is_null() {
            for (idx, &ms) in HOLD_PRESETS.iter().enumerate() {
                let checked = if config.hold_duration_ms == ms {
                    MF_CHECKED
                } else {
                    MF_UNCHECKED
                };
                let label = if ms == 350 {
                    format!("{ms} ms (Default)")
                } else {
                    format!("{ms} ms")
                };
                append_item(hold_menu, MF_STRING | checked, IDM_HOLD_BASE + idx, &label);
            }
            append_item(menu, MF_POPUP, hold_menu as usize, "Hold Duration");
        }

        // Switch shortcut submenu
        let method_menu = CreatePopupMenu();
        if !method_menu.is_null() {
            let ws_checked = if config.switch_method == SwitchMethod::WinSpace {
                MF_CHECKED
            } else {
                MF_UNCHECKED
            };
            let as_checked = if config.switch_method == SwitchMethod::AltShift {
                MF_CHECKED
            } else {
                MF_UNCHECKED
            };
            append_item(
                method_menu,
                MF_STRING | ws_checked,
                IDM_METHOD_WIN_SPACE,
                "Win + Space (Recommended)",
            );
            append_item(
                method_menu,
                MF_STRING | as_checked,
                IDM_METHOD_ALT_SHIFT,
                "Alt + Shift (Silent)",
            );
            append_item(menu, MF_POPUP, method_menu as usize, "Language Shortcut");
        }

        // OSD popup toggle
        let osd_flags = MF_STRING
            | if config.show_osd {
                MF_CHECKED
            } else {
                MF_UNCHECKED
            };
        append_item(menu, osd_flags, IDM_SHOW_OSD, "Show OSD Popup");

        // Start with Windows toggle
        let startup_flags = MF_STRING
            | if startup_enabled {
                MF_CHECKED
            } else {
                MF_UNCHECKED
            };
        append_item(menu, startup_flags, IDM_STARTUP, "Start with Windows");

        AppendMenuW(menu, MF_SEPARATOR, 0, ptr::null());
        append_item(menu, MF_STRING, IDM_EXIT, "Exit");

        let mut pt = POINT { x: 0, y: 0 };
        GetCursorPos(&mut pt);

        // Required so clicking outside the popup menu dismisses it properly.
        SetForegroundWindow(hwnd);
        TrackPopupMenu(
            menu,
            TPM_LEFTALIGN | TPM_BOTTOMALIGN | TPM_RIGHTBUTTON,
            pt.x,
            pt.y,
            0,
            hwnd,
            ptr::null(),
        );
        PostMessageW(hwnd, WM_NULL, 0, 0);

        DestroyMenu(menu);
    }
}

unsafe fn append_item(menu: *mut std::ffi::c_void, flags: u32, id: usize, text: &str) {
    let wide = to_wide(text);
    AppendMenuW(menu, flags, id, wide.as_ptr());
}

/// Generates a crisp 32x32 RGBA rounded badge icon in memory (no external .ico required).
fn create_badge_icon(paused: bool) -> HICON {
    const SIZE: usize = 32;
    let mut bgra = vec![0u8; SIZE * SIZE * 4];

    // Background color (BGRA)
    let (bg_r, bg_g, bg_b) = if paused {
        (100u8, 116u8, 139u8) // Slate gray
    } else {
        (37u8, 99u8, 235u8) // Vibrant blue
    };

    for y in 0..SIZE {
        for x in 0..SIZE {
            // Rounded rectangle SDF with radius 7 inside [2..30]
            let dx = if x < 9 {
                9.0 - x as f32
            } else if x > 22 {
                x as f32 - 22.0
            } else {
                0.0
            };
            let dy = if y < 9 {
                9.0 - y as f32
            } else if y > 22 {
                y as f32 - 22.0
            } else {
                0.0
            };
            let dist = (dx * dx + dy * dy).sqrt();
            if dist > 7.5 {
                continue;
            }

            let alpha = if dist > 6.5 {
                ((7.5 - dist) * 255.0).clamp(0.0, 255.0) as u8
            } else {
                255
            };

            let is_glyph = if paused {
                // Two vertical pause bars
                (9..=22).contains(&y) && ((11..=13).contains(&x) || (18..=20).contains(&x))
            } else {
                // Upward Caps Lock arrow + base bar
                let in_head = (7..=15).contains(&y)
                    && (x as isize - 15).abs() <= (y as isize - 7)
                    && (x as isize - 16).abs() <= (y as isize - 7);
                let in_stem = (15..=20).contains(&y) && (13..=18).contains(&x);
                let in_base = (22..=24).contains(&y) && (11..=20).contains(&x);
                in_head || in_stem || in_base
            };

            let (r, g, b) = if is_glyph {
                (255u8, 255u8, 255u8)
            } else {
                (bg_r, bg_g, bg_b)
            };

            let idx = (y * SIZE + x) * 4;
            bgra[idx] = b;
            bgra[idx + 1] = g;
            bgra[idx + 2] = r;
            bgra[idx + 3] = alpha;
        }
    }

    unsafe {
        let color_bmp = CreateBitmap(
            SIZE as i32,
            SIZE as i32,
            1,
            32,
            bgra.as_ptr() as *const std::ffi::c_void,
        );
        // Empty monochrome mask (alpha channel in 32-bit color_bmp handles transparency)
        let mask_bytes = [0u8; (SIZE / 8) * SIZE];
        let mask_bmp = CreateBitmap(
            SIZE as i32,
            SIZE as i32,
            1,
            1,
            mask_bytes.as_ptr() as *const std::ffi::c_void,
        );

        let info = ICONINFO {
            fIcon: 1,
            xHotspot: 0,
            yHotspot: 0,
            hbmMask: mask_bmp,
            hbmColor: color_bmp,
        };

        let icon = CreateIconIndirect(&info);
        DeleteObject(color_bmp);
        DeleteObject(mask_bmp);
        icon
    }
}
