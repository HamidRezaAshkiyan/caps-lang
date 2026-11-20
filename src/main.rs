#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(windows)]
mod config;
#[cfg(windows)]
mod hook;
#[cfg(windows)]
mod osd;
#[cfg(windows)]
mod tray;

#[cfg(not(windows))]
fn main() {
    eprintln!(
        "CapsLang is a Windows-only utility. Build for Windows with:\n  cargo build --release --target x86_64-pc-windows-gnu"
    );
    std::process::exit(1);
}

#[cfg(windows)]
fn main() {
    windows_app::run();
}

#[cfg(windows)]
mod windows_app {
    use std::cell::RefCell;
    use std::mem::{size_of, zeroed};
    use std::ptr;

    use windows_sys::Win32::Foundation::{
        CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HWND, LPARAM, LRESULT, WPARAM,
    };
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::System::Threading::CreateMutexW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW, KillTimer,
        PostQuitMessage, RegisterClassExW, RegisterWindowMessageW, SetTimer, TranslateMessage, MSG,
        WM_COMMAND, WM_CONTEXTMENU, WM_DESTROY, WM_LBUTTONUP, WM_RBUTTONUP, WM_TIMER,
        WNDCLASSEXW, WS_OVERLAPPED,
    };

    use crate::config::{
        is_startup_enabled, set_startup_enabled, to_wide, Config, SwitchMethod, HOLD_PRESETS,
    };
    use crate::hook::{self, TIMER_HOLD_ID, WM_APP_SWITCH_LANG};
    use crate::osd::{self, OsdWindow};
    use crate::tray::{
        self, TrayIcon, IDM_EXIT, IDM_HOLD_BASE, IDM_METHOD_ALT_SHIFT, IDM_METHOD_WIN_SPACE,
        IDM_PAUSE, IDM_SHOW_OSD, IDM_STARTUP, WM_TRAY_ICON,
    };

    const TIMER_OSD_LANG_ID: usize = 2;
    const TIMER_OSD_CAPS_ID: usize = 3;

    struct AppState {
        config: Config,
        tray: TrayIcon,
        osd: Option<OsdWindow>,
        taskbar_created_msg: u32,
    }

    thread_local! {
        static APP: RefCell<Option<AppState>> = const { RefCell::new(None) };
    }

    pub fn run() {
        unsafe {
            // Ensure only one instance runs at a time.
            let mutex_name = to_wide("Local\\CapsLang_SingleInstance_Mutex");
            let mutex = CreateMutexW(ptr::null(), 0, mutex_name.as_ptr());
            if mutex.is_null() {
                return;
            }
            if GetLastError() == ERROR_ALREADY_EXISTS {
                CloseHandle(mutex);
                return;
            }

            let hinstance = GetModuleHandleW(ptr::null());
            let class_name = to_wide("CapsLangHiddenWindow");

            let wc = WNDCLASSEXW {
                cbSize: size_of::<WNDCLASSEXW>() as u32,
                style: 0,
                lpfnWndProc: Some(wnd_proc),
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

            if RegisterClassExW(&wc) == 0 {
                CloseHandle(mutex);
                return;
            }

            let title = to_wide("CapsLang");
            let hwnd = CreateWindowExW(
                0,
                class_name.as_ptr(),
                title.as_ptr(),
                WS_OVERLAPPED,
                0,
                0,
                0,
                0,
                ptr::null_mut(),
                ptr::null_mut(),
                hinstance,
                ptr::null(),
            );

            if hwnd.is_null() {
                CloseHandle(mutex);
                return;
            }

            let config = Config::load();
            if !hook::install(hwnd, config) {
                DestroyWindow(hwnd);
                CloseHandle(mutex);
                return;
            }

            let taskbar_msg_name = to_wide("TaskbarCreated");
            let taskbar_created_msg = RegisterWindowMessageW(taskbar_msg_name.as_ptr());
            let tray = TrayIcon::new(hwnd, false, config.hold_duration_ms);
            let osd = OsdWindow::new();

            APP.with(|cell| {
                *cell.borrow_mut() = Some(AppState {
                    config,
                    tray,
                    osd,
                    taskbar_created_msg,
                });
            });

            let mut msg: MSG = zeroed();
            while GetMessageW(&mut msg, ptr::null_mut(), 0, 0) > 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }

            hook::uninstall();
            APP.with(|cell| {
                *cell.borrow_mut() = None;
            });
            CloseHandle(mutex);
        }
    }

    unsafe extern "system" fn wnd_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        // Re-add tray icon if Windows Explorer restarts.
        let is_taskbar_created = APP.with(|cell| {
            cell.borrow()
                .as_ref()
                .is_some_and(|s| s.taskbar_created_msg != 0 && msg == s.taskbar_created_msg)
        });
        if is_taskbar_created {
            APP.with(|cell| {
                if let Some(state) = cell.borrow().as_ref() {
                    state
                        .tray
                        .add(hook::is_paused(), state.config.hold_duration_ms);
                }
            });
            return 0;
        }

        match msg {
            WM_TIMER => {
                match wparam {
                    TIMER_HOLD_ID => {
                        if hook::on_hold_timer(hwnd) {
                            // Query Caps Lock toggle state shortly after synthetic keypress.
                            SetTimer(hwnd, TIMER_OSD_CAPS_ID, 50, None);
                        }
                    }
                    TIMER_OSD_LANG_ID => {
                        KillTimer(hwnd, TIMER_OSD_LANG_ID);
                        let code = osd::current_language_code();
                        show_osd(&code, true);
                    }
                    TIMER_OSD_CAPS_ID => {
                        KillTimer(hwnd, TIMER_OSD_CAPS_ID);
                        if osd::is_caps_lock_on() {
                            show_osd("CAPS ON", true);
                        } else {
                            show_osd("CAPS OFF", false);
                        }
                    }
                    _ => {}
                }
                0
            }
            WM_APP_SWITCH_LANG => {
                hook::send_switch_language();
                // Allow Windows ~90ms to apply the new keyboard layout before querying it.
                SetTimer(hwnd, TIMER_OSD_LANG_ID, 90, None);
                0
            }
            WM_TRAY_ICON => {
                let mouse_msg = (lparam & 0xFFFF) as u32;
                if matches!(mouse_msg, WM_RBUTTONUP | WM_LBUTTONUP | WM_CONTEXTMENU) {
                    let config = APP.with(|cell| {
                        cell.borrow()
                            .as_ref()
                            .map(|s| s.config)
                            .unwrap_or_default()
                    });
                    tray::show_context_menu(hwnd, hook::is_paused(), config, is_startup_enabled());
                }
                0
            }
            WM_COMMAND => {
                let cmd = wparam & 0xFFFF;
                handle_command(hwnd, cmd);
                0
            }
            WM_DESTROY => {
                hook::uninstall();
                APP.with(|cell| {
                    *cell.borrow_mut() = None;
                });
                PostQuitMessage(0);
                0
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }

    fn show_osd(text: &str, accent: bool) {
        APP.with(|cell| {
            if let Some(state) = cell.borrow().as_ref() {
                if state.config.show_osd {
                    if let Some(osd) = state.osd.as_ref() {
                        osd.show(text, accent);
                    }
                }
            }
        });
    }

    fn handle_command(hwnd: HWND, cmd: usize) {
        match cmd {
            IDM_PAUSE => {
                let paused = hook::toggle_paused();
                APP.with(|cell| {
                    if let Some(state) = cell.borrow().as_ref() {
                        state.tray.update(paused, state.config.hold_duration_ms);
                    }
                });
            }
            IDM_SHOW_OSD => update_config(|c| c.show_osd = !c.show_osd),
            IDM_STARTUP => {
                let currently_enabled = is_startup_enabled();
                let _ = set_startup_enabled(!currently_enabled);
            }
            IDM_METHOD_WIN_SPACE => update_config(|c| c.switch_method = SwitchMethod::WinSpace),
            IDM_METHOD_ALT_SHIFT => update_config(|c| c.switch_method = SwitchMethod::AltShift),
            IDM_EXIT => unsafe {
                DestroyWindow(hwnd);
            },
            id if (IDM_HOLD_BASE..IDM_HOLD_BASE + HOLD_PRESETS.len()).contains(&id) => {
                let ms = HOLD_PRESETS[id - IDM_HOLD_BASE];
                update_config(|c| c.hold_duration_ms = ms);
            }
            _ => {}
        }
    }

    fn update_config(f: impl FnOnce(&mut Config)) {
        APP.with(|cell| {
            if let Some(state) = cell.borrow_mut().as_mut() {
                f(&mut state.config);
                state.config.save();
                hook::apply_config(state.config);
                state
                    .tray
                    .update(hook::is_paused(), state.config.hold_duration_ms);
            }
        });
    }
}
