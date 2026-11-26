# AGENTS.md — CapsLang (`caps-lang`)

## Overview
**CapsLang** (`v0.2.2`) is a lightweight, Windows-only utility written in pure Rust (`windows-sys`) with zero external runtime dependencies that replicates macOS Caps Lock behavior on Windows:
- **Short tap Caps Lock:** switches input language (`Win + Space` or silent `Alt + Shift`).
- **Hold Caps Lock** (configurable `200–600 ms`, default `350 ms`): toggles Caps Lock normally.
- Displays a non-focus-stealing floating OSD pill (`EN`, `FA`, `CAPS ON`, `CAPS OFF`) and lives in the Windows system tray.

## Tech Stack & Requirements
- **Language / Toolchain:** Rust (Edition 2021).
- **Target OS:** Windows (`x86_64-pc-windows-msvc` on Windows, or cross-compiled from macOS/Linux via `x86_64-pc-windows-gnu` + `mingw-w64`).
- **Dependencies (`cfg(windows)` only):**
  - `windows-sys` `0.59` (Win32 Foundation, Globalization, GDI, Security, Registry, Threading, KeyboardAndMouse, Shell, WindowsAndMessaging).
  - Build dependency: `winresource` `0.1` (embeds `assets/caps-lang.ico` and PE version metadata via `build.rs`).
- **Release Profile:** Optimized for size (`opt-level = "s"`, `lto = true`, `codegen-units = 1`, `strip = true`, `windows_subsystem = "windows"` in release).

## Repository Structure
```text
caps-lang/
├── Cargo.toml                  # Package manifest & Win32 feature flags
├── build.rs                    # Embeds icon & Windows PE metadata via winresource
├── assets/caps-lang.ico        # Application & tray icon
├── src/
│   ├── main.rs                 # Win32 message loop, single-instance mutex, window proc, timers
│   ├── hook.rs                 # Low-level keyboard hook (WH_KEYBOARD_LL) & synthetic key injection
│   ├── osd.rs                  # Floating translucent GDI OSD pill window & layout query
│   ├── tray.rs                 # System tray icon (Shell_NotifyIconW) & context menu
│   └── config.rs               # HKCU Registry persistence & Windows Startup (Run) toggle
└── .github/workflows/          # Release / CI automation (generates portable exe + Winget manifests)
```

## Architecture & Key Flows
- **Entry Point (`src/main.rs`):** On non-Windows hosts, compiles a stub `main()` that prints the cross-compile command and exits with code `1`. On Windows:
  1. Acquires a named single-instance mutex (`Local\CapsLang_SingleInstance_Mutex`).
  2. Registers a hidden window class (`CapsLangHiddenWindow`) to receive Win32 messages, timers, and tray callbacks.
  3. Loads `Config` from Registry, installs the low-level keyboard hook (`hook::install`), registers the `"TaskbarCreated"` broadcast message (for Explorer crash recovery), and initializes `TrayIcon` and `OsdWindow`.
  4. Runs the standard Win32 `GetMessageW` / `DispatchMessageW` loop until `WM_DESTROY`.
- **Tap vs. Hold Flow:** `hook.rs` intercepts Caps Lock keydown/keyup events, arms `TIMER_HOLD_ID` for hold detection (`hook::on_hold_timer`), or posts `WM_APP_SWITCH_LANG` on short tap. Short timers (`TIMER_OSD_LANG_ID` at 90 ms, `TIMER_OSD_CAPS_ID` at 50 ms) allow Windows to update layout/Caps state before displaying the OSD pill.
- **State Management:** Thread-local `RefCell<Option<AppState>>` inside `windows_app` holds config, tray, and OSD handles on the single Win32 UI thread.

## Development & Commands
```bash
# Build on Windows (MSVC)
cargo build --release
# Output: target\release\caps-lang.exe

# Cross-compile from macOS (requires: brew install mingw-w64)
rustup target add x86_64-pc-windows-gnu
cargo build --release --target x86_64-pc-windows-gnu
# Output: target/x86_64-pc-windows-gnu/release/caps-lang.exe

# Type-check Windows code from macOS/Linux
cargo check --target x86_64-pc-windows-gnu
```

## Configuration & Environment
- No `.env` files or secrets. All settings are persisted in the Windows Registry under `HKCU\Software\CapsLang`:
  - `HoldDurationMs` (`REG_DWORD`, validated `100..=2000`, default `350`, presets `200/300/350/400/500/600`).
  - `SwitchMethod` (`REG_DWORD`: `0` = `Win+Space`, `1` = `Alt+Shift`).
  - `ShowOsd` (`REG_DWORD`: `0` / `1`, default `1`).
- **Start with Windows:** Stored as a `REG_SZ` entry named `CapsLang` under `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`.

## Conventions & Gotchas
- **Always check with the Windows target:** Running plain `cargo check` on macOS/Linux only compiles the 6-line `#[cfg(not(windows))]` stub and skips `config.rs`, `hook.rs`, `osd.rs`, and `tray.rs`. Always run `cargo check --target x86_64-pc-windows-gnu` (or build on Windows) to verify changes.
- **Release console suppression:** `#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]` hides the console window in release builds; use debug builds when inspecting `eprintln!` / stdout diagnostics.
- **Unsafe Win32 invariants:** All Win32 FFI calls live behind `unsafe` blocks on the main UI thread; never block inside `WH_KEYBOARD_LL` callbacks (post messages like `WM_APP_SWITCH_LANG` instead) or Windows will silently unhook the keyboard hook.
