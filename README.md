# CapsLang

macOS-style Caps Lock utility for Windows, written in pure Rust (`windows-sys`) with zero external runtime dependencies.

## Features

- **Short tap Caps Lock** → switches input language (`Win + Space` by default, or `Alt + Shift` silent mode).
- **Hold Caps Lock** (default `350 ms`) → toggles Caps Lock normally.
- **System tray icon** with menu to:
  - Pause / resume remapping
  - Change hold duration (`200 ms` – `600 ms`, persisted in Registry)
  - Choose language switch shortcut (`Win + Space` or `Alt + Shift`)
  - Toggle **Start with Windows** (`HKCU\Software\Microsoft\Windows\CurrentVersion\Run`)
  - Exit cleanly
- **Single-instance guard** & automatic tray icon recovery if Windows Explorer restarts.

## Building

### On Windows (MSVC)

```powershell
cargo build --release
```

Output binary: `target\release\caps-lang.exe`

### Cross-compiling from macOS

Install the MinGW-w64 linker once:

```bash
brew install mingw-w64
rustup target add x86_64-pc-windows-gnu
```

Then build the standalone Windows `.exe`:

```bash
cargo build --release --target x86_64-pc-windows-gnu
```

Output binary: `target/x86_64-pc-windows-gnu/release/caps-lang.exe`
