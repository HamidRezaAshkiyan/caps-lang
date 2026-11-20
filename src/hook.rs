use std::ffi::c_void;
use std::mem::size_of;
use std::ptr;
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU32, Ordering};

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VK_CAPITAL, VK_LMENU,
    VK_LSHIFT, VK_LWIN, VK_SPACE,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, KillTimer, PostMessageW, SetTimer, SetWindowsHookExW, UnhookWindowsHookEx,
    HHOOK, KBDLLHOOKSTRUCT, WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
    WM_USER,
};

use crate::config::{Config, SwitchMethod, DEFAULT_HOLD_MS};

/// Custom tag written to `dwExtraInfo` on synthetic key events so the hook ignores its own output.
const INJECTED_TAG: usize = 0x4341_5053; // "CAPS"

pub const TIMER_HOLD_ID: usize = 1;
pub const WM_APP_SWITCH_LANG: u32 = WM_USER + 10;

static HOOK_HANDLE: AtomicPtr<c_void> = AtomicPtr::new(ptr::null_mut());
static TARGET_HWND: AtomicPtr<c_void> = AtomicPtr::new(ptr::null_mut());
static PAUSED: AtomicBool = AtomicBool::new(false);
static CAPS_DOWN: AtomicBool = AtomicBool::new(false);
static HOLD_TRIGGERED: AtomicBool = AtomicBool::new(false);
static HOLD_DURATION_MS: AtomicU32 = AtomicU32::new(DEFAULT_HOLD_MS);
static SWITCH_METHOD: AtomicU32 = AtomicU32::new(SwitchMethod::WinSpace as u32);

pub fn install(hwnd: HWND, config: Config) -> bool {
    TARGET_HWND.store(hwnd, Ordering::SeqCst);
    apply_config(config);

    unsafe {
        let hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), ptr::null_mut(), 0);
        if hook.is_null() {
            return false;
        }
        HOOK_HANDLE.store(hook, Ordering::SeqCst);
        true
    }
}

pub fn uninstall() {
    let hook = HOOK_HANDLE.swap(ptr::null_mut(), Ordering::SeqCst) as HHOOK;
    if !hook.is_null() {
        unsafe {
            UnhookWindowsHookEx(hook);
        }
    }
}

pub fn apply_config(config: Config) {
    HOLD_DURATION_MS.store(config.hold_duration_ms, Ordering::Relaxed);
    SWITCH_METHOD.store(config.switch_method as u32, Ordering::Relaxed);
}

pub fn is_paused() -> bool {
    PAUSED.load(Ordering::Relaxed)
}

pub fn toggle_paused() -> bool {
    let now_paused = !PAUSED.fetch_xor(true, Ordering::SeqCst);
    if now_paused {
        // Cancel any in-flight hold state when pausing.
        CAPS_DOWN.store(false, Ordering::SeqCst);
        HOLD_TRIGGERED.store(false, Ordering::SeqCst);
        let hwnd = TARGET_HWND.load(Ordering::SeqCst) as HWND;
        if !hwnd.is_null() {
            unsafe {
                KillTimer(hwnd, TIMER_HOLD_ID);
            }
        }
    }
    now_paused
}

/// Called from the window procedure when `TIMER_HOLD_ID` fires.
/// Returns `true` if Caps Lock was toggled.
pub fn on_hold_timer(hwnd: HWND) -> bool {
    unsafe {
        KillTimer(hwnd, TIMER_HOLD_ID);
    }

    if CAPS_DOWN.load(Ordering::SeqCst) && !HOLD_TRIGGERED.swap(true, Ordering::SeqCst) {
        send_caps_toggle();
        return true;
    }
    false
}

/// Called from the window procedure on `WM_APP_SWITCH_LANG` (outside the hook callback).
pub fn send_switch_language() {
    let method = SwitchMethod::from_u32(SWITCH_METHOD.load(Ordering::Relaxed));
    match method {
        SwitchMethod::WinSpace => send_key_combo(VK_LWIN, VK_SPACE),
        SwitchMethod::AltShift => send_key_combo(VK_LMENU, VK_LSHIFT),
    }
}

fn send_caps_toggle() {
    let inputs = [
        key_input(VK_CAPITAL, 0),
        key_input(VK_CAPITAL, KEYEVENTF_KEYUP),
    ];
    unsafe {
        SendInput(
            inputs.len() as u32,
            inputs.as_ptr(),
            size_of::<INPUT>() as i32,
        );
    }
}

fn send_key_combo(modifier: u16, key: u16) {
    let inputs = [
        key_input(modifier, 0),
        key_input(key, 0),
        key_input(key, KEYEVENTF_KEYUP),
        key_input(modifier, KEYEVENTF_KEYUP),
    ];
    unsafe {
        SendInput(
            inputs.len() as u32,
            inputs.as_ptr(),
            size_of::<INPUT>() as i32,
        );
    }
}

fn key_input(vk: u16, flags: u32) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: INJECTED_TAG,
            },
        },
    }
}

unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code < 0 {
        return CallNextHookEx(ptr::null_mut(), code, wparam, lparam);
    }

    let kbd = &*(lparam as *const KBDLLHOOKSTRUCT);

    // Only intercept physical Caps Lock presses; ignore our own synthetic events.
    if kbd.vkCode != VK_CAPITAL as u32 || kbd.dwExtraInfo == INJECTED_TAG {
        return CallNextHookEx(ptr::null_mut(), code, wparam, lparam);
    }

    if PAUSED.load(Ordering::Relaxed) {
        return CallNextHookEx(ptr::null_mut(), code, wparam, lparam);
    }

    let hwnd = TARGET_HWND.load(Ordering::SeqCst) as HWND;
    let msg = wparam as u32;

    match msg {
        WM_KEYDOWN | WM_SYSKEYDOWN => {
            if !CAPS_DOWN.swap(true, Ordering::SeqCst) {
                HOLD_TRIGGERED.store(false, Ordering::SeqCst);
                let hold_ms = HOLD_DURATION_MS.load(Ordering::Relaxed);
                SetTimer(hwnd, TIMER_HOLD_ID, hold_ms, None);
            }
            // Swallow key down and all key-repeat events while held.
            1
        }
        WM_KEYUP | WM_SYSKEYUP => {
            if CAPS_DOWN.swap(false, Ordering::SeqCst) {
                KillTimer(hwnd, TIMER_HOLD_ID);
                if !HOLD_TRIGGERED.swap(false, Ordering::SeqCst) {
                    // Short tap: trigger input language switch asynchronously outside the hook.
                    PostMessageW(hwnd, WM_APP_SWITCH_LANG, 0, 0);
                }
                1
            } else {
                CallNextHookEx(ptr::null_mut(), code, wparam, lparam)
            }
        }
        _ => CallNextHookEx(ptr::null_mut(), code, wparam, lparam),
    }
}
