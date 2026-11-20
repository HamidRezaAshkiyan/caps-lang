# CapsLang

macOS-style Caps Lock utility for Windows, written in pure Rust (`windows-sys`) with zero external runtime dependencies.

## Features

- **Short tap Caps Lock** → switches input language (`Win + Space` by default, or `Alt + Shift` silent mode).
- **Hold Caps Lock** (default `350 ms`) → toggles Caps Lock normally.
- **Floating OSD pill** → sleek macOS-style translucent HUD that briefly shows the active language (`EN`, `FA`, …) or `CAPS ON` / `CAPS OFF` without stealing focus.
- **System tray icon** with menu to:
  - Pause / resume remapping
  - Toggle **Show OSD Popup**
  - Change hold duration (`200 ms` – `600 ms`, persisted in Registry)
  - Choose language switch shortcut (`Win + Space` or `Alt + Shift`)
  - Toggle **Start with Windows** (`HKCU\Software\Microsoft\Windows\CurrentVersion\Run`)
  - Exit cleanly
- **Embedded Windows icon & metadata** in `caps-lang.exe`.
- **Single-instance guard** & automatic tray icon recovery if Windows Explorer restarts.

## Installation

### Direct Download (Portable `.exe`)

Download the latest `caps-lang.exe` from [GitHub Releases](https://github.com/HamidRezaAshkiyan/caps-lang/releases/latest) and double-click to run.

### Winget

Each GitHub Release automatically generates ready-to-use Winget 1.6.0 portable manifests (`winget-manifests.zip`).

**Install locally from the release manifest:**
```powershell
# Enable local manifests once (Admin PowerShell)
winget settings --enable LocalManifestFiles

# Extract winget-manifests.zip and install
winget install --manifest .\HamidRezaAshkiyan.CapsLang
```

**Publish to the official Winget Community Repository (`microsoft/winget-pkgs`):**
Once submitted (via `wingetcreate submit .\HamidRezaAshkiyan.CapsLang` or a PR to `microsoft/winget-pkgs`), anyone can install with:
```powershell
winget install HamidRezaAshkiyan.CapsLang
```

## Building from Source

### On Windows (MSVC)

```powershell
cargo build --release
```

Output binary: `target\release\caps-lang.exe`

### Cross-compiling from macOS

```bash
brew install mingw-w64
rustup target add x86_64-pc-windows-gnu
cargo build --release --target x86_64-pc-windows-gnu
```

Output binary: `target/x86_64-pc-windows-gnu/release/caps-lang.exe`
