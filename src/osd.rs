use std::cell::RefCell;
use std::mem::{size_of, zeroed};
use std::ptr;

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::Globalization::{GetLocaleInfoW, LOCALE_SISO639LANGNAME};
use windows_sys::Win32::Graphics::Gdi::{
    BeginPaint, CreateFontW, CreatePen, CreateSolidBrush, DeleteObject, DrawTextW, EndPaint,
    InvalidateRect, Rectangle, SelectObject, SetBkMode, SetTextColor, CLEARTYPE_QUALITY,
    CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET, DEFAULT_PITCH, DT_CENTER, DT_SINGLELINE, DT_VCENTER,
    FF_SWISS, FW_SEMIBOLD, HFONT, OUT_DEFAULT_PRECIS, PAINTSTRUCT, PS_SOLID, TRANSPARENT,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, GetKeyboardLayout, VK_CAPITAL};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetForegroundWindow, GetWindowThreadProcessId,
    KillTimer, RegisterClassExW, SetLayeredWindowAttributes, SetTimer, SetWindowPos, ShowWindow,
    SystemParametersInfoW, HWND_TOPMOST, LWA_ALPHA, SPI_GETWORKAREA, SWP_NOACTIVATE, SWP_NOSIZE,
    SW_HIDE, SW_SHOWNOACTIVATE, WM_ERASEBKGND, WM_PAINT, WM_TIMER, WNDCLASSEXW, WS_EX_LAYERED,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};

use crate::config::to_wide;

const OSD_WIDTH: i32 = 88;
const OSD_HEIGHT: i32 = 28;
const OSD_MARGIN: i32 = 12;
const OSD_INITIAL_ALPHA: u8 = 225;
const OSD_HOLD_MS: u32 = 500;
const OSD_FADE_INTERVAL_MS: u32 = 16;
const OSD_FADE_STEP: u8 = 28;

const TIMER_OSD_HOLD: usize = 101;
const TIMER_OSD_FADE: usize = 102;

fn rgb(r: u8, g: u8, b: u8) -> u32 {
    (r as u32) | ((g as u32) << 8) | ((b as u32) << 16)
}

struct OsdState {
    text: String,
    accent: bool,
    alpha: u8,
    font: HFONT,
}

thread_local! {
    static OSD_STATE: RefCell<Option<OsdState>> = const { RefCell::new(None) };
}

pub struct OsdWindow {
    hwnd: HWND,
}

impl OsdWindow {
    pub fn new() -> Option<Self> {
        unsafe {
            let hinstance = GetModuleHandleW(ptr::null());
            let class_name = to_wide("CapsLangOsdWindow");

            let wc = WNDCLASSEXW {
                cbSize: size_of::<WNDCLASSEXW>() as u32,
                style: 0,
                lpfnWndProc: Some(osd_wnd_proc),
                cbClsExtra: 0,
                cbWndExtra: 0,
                hInstance: hinstance,
                hIcon: ptr::null_mut(),
                hCursor: ptr::null_mut(),
                hbrBackground: ptr::null_mut(),
                lpszMenuName: ptr::null(),
                lpszClassName: class_name.as_ptr(),
                hIconSm: ptr::null_mut(),
            };
            RegisterClassExW(&wc);

            let ex_style = WS_EX_LAYERED
                | WS_EX_TRANSPARENT
                | WS_EX_TOPMOST
                | WS_EX_TOOLWINDOW
                | WS_EX_NOACTIVATE;

            let hwnd = CreateWindowExW(
                ex_style,
                class_name.as_ptr(),
                ptr::null(),
                WS_POPUP,
                0,
                0,
                OSD_WIDTH,
                OSD_HEIGHT,
                ptr::null_mut(),
                ptr::null_mut(),
                hinstance,
                ptr::null(),
            );

            if hwnd.is_null() {
                return None;
            }

            let font_name = to_wide("Segoe UI");
            let font = CreateFontW(
                -14,
                0,
                0,
                0,
                FW_SEMIBOLD as i32,
                0,
                0,
                0,
                DEFAULT_CHARSET as u32,
                OUT_DEFAULT_PRECIS as u32,
                CLIP_DEFAULT_PRECIS as u32,
                CLEARTYPE_QUALITY as u32,
                (DEFAULT_PITCH | FF_SWISS) as u32,
                font_name.as_ptr(),
            );

            OSD_STATE.with(|cell| {
                *cell.borrow_mut() = Some(OsdState {
                    text: String::new(),
                    accent: true,
                    alpha: OSD_INITIAL_ALPHA,
                    font,
                });
            });

            Some(Self { hwnd })
        }
    }

    pub fn show(&self, text: &str, accent: bool) {
        OSD_STATE.with(|cell| {
            if let Some(state) = cell.borrow_mut().as_mut() {
                state.text = text.to_string();
                state.accent = accent;
                state.alpha = OSD_INITIAL_ALPHA;
            }
        });

        unsafe {
            KillTimer(self.hwnd, TIMER_OSD_HOLD);
            KillTimer(self.hwnd, TIMER_OSD_FADE);

            // Position in the top-left corner of the primary work area.
            let mut work: RECT = zeroed();
            if SystemParametersInfoW(
                SPI_GETWORKAREA,
                0,
                &mut work as *mut RECT as *mut std::ffi::c_void,
                0,
            ) != 0
            {
                let x = work.left + OSD_MARGIN;
                let y = work.top + OSD_MARGIN;
                SetWindowPos(
                    self.hwnd,
                    HWND_TOPMOST,
                    x,
                    y,
                    0,
                    0,
                    SWP_NOSIZE | SWP_NOACTIVATE,
                );
            }

            SetLayeredWindowAttributes(self.hwnd, 0, OSD_INITIAL_ALPHA, LWA_ALPHA);
            ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
            InvalidateRect(self.hwnd, ptr::null(), 1);
            SetTimer(self.hwnd, TIMER_OSD_HOLD, OSD_HOLD_MS, None);
        }
    }
}

impl Drop for OsdWindow {
    fn drop(&mut self) {
        unsafe {
            if !self.hwnd.is_null() {
                DestroyWindow(self.hwnd);
            }
            OSD_STATE.with(|cell| {
                if let Some(state) = cell.borrow_mut().take() {
                    if !state.font.is_null() {
                        DeleteObject(state.font);
                    }
                }
            });
        }
    }
}

/// Returns the active foreground window's 2-letter ISO language code (e.g., `"EN"`, `"FA"`).
pub fn current_language_code() -> String {
    unsafe {
        let fg = GetForegroundWindow();
        let tid = if fg.is_null() {
            0
        } else {
            GetWindowThreadProcessId(fg, ptr::null_mut())
        };
        let hkl = GetKeyboardLayout(tid);
        let lang_id = (hkl as usize & 0xFFFF) as u32;

        let mut buf = [0u16; 16];
        let len = GetLocaleInfoW(
            lang_id,
            LOCALE_SISO639LANGNAME,
            buf.as_mut_ptr(),
            buf.len() as i32,
        );
        if len > 1 {
            let code = String::from_utf16_lossy(&buf[..(len as usize - 1)]);
            let upper = code.trim().to_uppercase();
            if !upper.is_empty() {
                return upper;
            }
        }
        "LANG".to_string()
    }
}

pub fn is_caps_lock_on() -> bool {
    unsafe { (GetKeyState(VK_CAPITAL as i32) & 1) != 0 }
}

unsafe extern "system" fn osd_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = zeroed();
            let hdc = BeginPaint(hwnd, &mut ps);

            OSD_STATE.with(|cell| {
                if let Some(state) = cell.borrow().as_ref() {
                    let bg_color = rgb(18, 18, 22);
                    let border_color = if state.accent {
                        rgb(59, 130, 246)
                    } else {
                        rgb(60, 63, 72)
                    };

                    let brush = CreateSolidBrush(bg_color);
                    let pen = CreatePen(PS_SOLID, 1, border_color);
                    let old_brush = SelectObject(hdc, brush);
                    let old_pen = SelectObject(hdc, pen);

                    Rectangle(hdc, 0, 0, OSD_WIDTH, OSD_HEIGHT);

                    SelectObject(hdc, old_brush);
                    SelectObject(hdc, old_pen);
                    DeleteObject(brush);
                    DeleteObject(pen);

                    SetBkMode(hdc, TRANSPARENT as i32);
                    SetTextColor(hdc, rgb(244, 244, 246));

                    let old_font = if !state.font.is_null() {
                        SelectObject(hdc, state.font)
                    } else {
                        ptr::null_mut()
                    };

                    let wide = to_wide(&state.text);
                    let mut rect = RECT {
                        left: 0,
                        top: 0,
                        right: OSD_WIDTH,
                        bottom: OSD_HEIGHT,
                    };
                    DrawTextW(
                        hdc,
                        wide.as_ptr(),
                        (wide.len().saturating_sub(1)) as i32,
                        &mut rect,
                        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
                    );

                    if !old_font.is_null() {
                        SelectObject(hdc, old_font);
                    }
                }
            });

            EndPaint(hwnd, &ps);
            0
        }
        WM_TIMER => {
            match wparam {
                TIMER_OSD_HOLD => {
                    KillTimer(hwnd, TIMER_OSD_HOLD);
                    SetTimer(hwnd, TIMER_OSD_FADE, OSD_FADE_INTERVAL_MS, None);
                }
                TIMER_OSD_FADE => {
                    let next_alpha = OSD_STATE.with(|cell| {
                        if let Some(state) = cell.borrow_mut().as_mut() {
                            state.alpha = state.alpha.saturating_sub(OSD_FADE_STEP);
                            state.alpha
                        } else {
                            0
                        }
                    });
                    if next_alpha == 0 {
                        KillTimer(hwnd, TIMER_OSD_FADE);
                        ShowWindow(hwnd, SW_HIDE);
                    } else {
                        SetLayeredWindowAttributes(hwnd, 0, next_alpha, LWA_ALPHA);
                    }
                }
                _ => {}
            }
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
