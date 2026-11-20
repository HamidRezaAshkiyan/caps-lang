use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::ptr;

use windows_sys::Win32::Foundation::ERROR_SUCCESS;
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW,
    RegSetValueExW, HKEY, HKEY_CURRENT_USER, KEY_READ, KEY_WRITE, REG_DWORD, REG_OPTION_NON_VOLATILE,
    REG_SZ,
};

pub const HOLD_PRESETS: &[u32] = &[200, 300, 350, 400, 500, 600];
pub const DEFAULT_HOLD_MS: u32 = 350;

const APP_REG_PATH: &str = "Software\\CapsLang";
const RUN_REG_PATH: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const APP_NAME: &str = "CapsLang";
const VAL_HOLD_MS: &str = "HoldDurationMs";
const VAL_SWITCH_METHOD: &str = "SwitchMethod";
const VAL_SHOW_OSD: &str = "ShowOsd";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum SwitchMethod {
    WinSpace = 0,
    AltShift = 1,
}

impl SwitchMethod {
    pub fn from_u32(v: u32) -> Self {
        match v {
            1 => SwitchMethod::AltShift,
            _ => SwitchMethod::WinSpace,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Config {
    pub hold_duration_ms: u32,
    pub switch_method: SwitchMethod,
    pub show_osd: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            hold_duration_ms: DEFAULT_HOLD_MS,
            switch_method: SwitchMethod::WinSpace,
            show_osd: true,
        }
    }
}

pub fn to_wide(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(Some(0)).collect()
}

impl Config {
    pub fn load() -> Self {
        let mut cfg = Self::default();
        let subkey = to_wide(APP_REG_PATH);
        let mut hkey: HKEY = ptr::null_mut();

        unsafe {
            if RegOpenKeyExW(HKEY_CURRENT_USER, subkey.as_ptr(), 0, KEY_READ, &mut hkey)
                == ERROR_SUCCESS
            {
                if let Some(ms) = read_dword(hkey, VAL_HOLD_MS) {
                    if (100..=2000).contains(&ms) {
                        cfg.hold_duration_ms = ms;
                    }
                }
                if let Some(method) = read_dword(hkey, VAL_SWITCH_METHOD) {
                    cfg.switch_method = SwitchMethod::from_u32(method);
                }
                if let Some(osd) = read_dword(hkey, VAL_SHOW_OSD) {
                    cfg.show_osd = osd != 0;
                }
                RegCloseKey(hkey);
            }
        }

        cfg
    }

    pub fn save(&self) {
        let subkey = to_wide(APP_REG_PATH);
        let mut hkey: HKEY = ptr::null_mut();

        unsafe {
            if RegCreateKeyExW(
                HKEY_CURRENT_USER,
                subkey.as_ptr(),
                0,
                ptr::null(),
                REG_OPTION_NON_VOLATILE,
                KEY_WRITE,
                ptr::null(),
                &mut hkey,
                ptr::null_mut(),
            ) == ERROR_SUCCESS
            {
                write_dword(hkey, VAL_HOLD_MS, self.hold_duration_ms);
                write_dword(hkey, VAL_SWITCH_METHOD, self.switch_method as u32);
                write_dword(hkey, VAL_SHOW_OSD, u32::from(self.show_osd));
                RegCloseKey(hkey);
            }
        }
    }
}

unsafe fn read_dword(hkey: HKEY, name: &str) -> Option<u32> {
    let val_name = to_wide(name);
    let mut data: u32 = 0;
    let mut size: u32 = std::mem::size_of::<u32>() as u32;
    let mut val_type: u32 = 0;

    let status = RegQueryValueExW(
        hkey,
        val_name.as_ptr(),
        ptr::null_mut(),
        &mut val_type,
        &mut data as *mut u32 as *mut u8,
        &mut size,
    );

    if status == ERROR_SUCCESS && val_type == REG_DWORD {
        Some(data)
    } else {
        None
    }
}

unsafe fn write_dword(hkey: HKEY, name: &str, value: u32) {
    let val_name = to_wide(name);
    let bytes = value.to_le_bytes();
    RegSetValueExW(
        hkey,
        val_name.as_ptr(),
        0,
        REG_DWORD,
        bytes.as_ptr(),
        bytes.len() as u32,
    );
}

pub fn is_startup_enabled() -> bool {
    let subkey = to_wide(RUN_REG_PATH);
    let val_name = to_wide(APP_NAME);
    let mut hkey: HKEY = ptr::null_mut();

    unsafe {
        if RegOpenKeyExW(HKEY_CURRENT_USER, subkey.as_ptr(), 0, KEY_READ, &mut hkey)
            != ERROR_SUCCESS
        {
            return false;
        }
        let mut size: u32 = 0;
        let status = RegQueryValueExW(
            hkey,
            val_name.as_ptr(),
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            &mut size,
        );
        RegCloseKey(hkey);
        status == ERROR_SUCCESS
    }
}

pub fn set_startup_enabled(enable: bool) -> bool {
    let subkey = to_wide(RUN_REG_PATH);
    let val_name = to_wide(APP_NAME);
    let mut hkey: HKEY = ptr::null_mut();

    unsafe {
        if RegOpenKeyExW(HKEY_CURRENT_USER, subkey.as_ptr(), 0, KEY_WRITE, &mut hkey)
            != ERROR_SUCCESS
        {
            return false;
        }

        let ok = if enable {
            if let Ok(exe_path) = std::env::current_exe() {
                let quoted = format!("\"{}\"", exe_path.display());
                let wide = to_wide(&quoted);
                let byte_len = (wide.len() * std::mem::size_of::<u16>()) as u32;
                RegSetValueExW(
                    hkey,
                    val_name.as_ptr(),
                    0,
                    REG_SZ,
                    wide.as_ptr() as *const u8,
                    byte_len,
                ) == ERROR_SUCCESS
            } else {
                false
            }
        } else {
            let status = RegDeleteValueW(hkey, val_name.as_ptr());
            status == ERROR_SUCCESS
        };

        RegCloseKey(hkey);
        ok
    }
}
