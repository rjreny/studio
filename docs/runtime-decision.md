# Runtime decision

Date: 2026-08-23  
Prototypes: `prototypes/tauri-shell`, `prototypes/electron-shell`  
Rule honored: same specification, **not** shared implementation.

## Choice

**Host: Tauri 2** for Studio v1.

This is a local-first, React-on-Windows product with files, settings, and a modest native surface. That matches the frozen heuristic. Electron remains the right switch if Studio later becomes IDE-like (plugins, many Node-native modules, heavy in-process background work).

The small `platform/` boundary in the real app is how that switch stays possible. It is a goal, not a guarantee.

## What was built

Both shells implement: custom chrome, sidebar (Home / Library / Projects / Settings), 6,000-row virtualized list with sort/select/keyboard/context menu, command palette (Ctrl+K), Ctrl+,, Escape stack, theme + accent split, persisted prefs, native open-file, updater HTTP smoke, local NSIS installer.

## Measured (this machine, Windows 11)

| | Tauri 2 | Electron 39 |
|---|---|---|
| Dev window | `tauri-shell.exe` ~31 MB (WebView2 is a separate Edge process; not isolated cleanly on this box) | 4 processes, **~339 MB** combined |
| Installer | `prototypes/tauri-shell/release/Studio_0.1.0_x64-setup.exe` **2.1 MB** | `prototypes/electron-shell/release/Studio Setup 0.1.0.exe` **89.8 MB** |
| Native layer in prototype | Rust command `file_info` + dialog/store plugins | Main-process Node `fs` + `dialog` via preload |
| Custom chrome | Drawn caption buttons (`decorations: false`) | Native `titleBarOverlay` (Snap Layouts should work) |
| Snap Layouts | Expected **fail** without Win32 overlay (not patched in bakeoff) | Expected **pass** via overlay |

Installer size was recorded, not used as the decision.

## Still for you to feel

These cannot be faked from packaging logs. Use both windows before you treat this as irreversible:

- Drag, double-click maximize, restore, **Snap Layouts**
- Continuous resize with the 6k list visible
- DPI 100 / 125 / 150 / 200% and mixed-monitor move
- Keyboard: palette, settings, Escape, list selection
- Leave each running 10+ minutes
- Cold start stopwatch

If native Win11 caption + Snap Layouts feel like the whole product, switch to Electron and keep `platform/`. If the shell already feels like a real app in Tauri, stay.

## How native work felt to build

- **Tauri:** extra language at the boundary (`file_info` in Rust). Plugins and capabilities are explicit. First compile is slow; after that, fine.
- **Electron:** one language, preload bridge is the natural 2026 pattern. Packager/signing noise is heavier. Titlebar overlay is first-party on Windows.

## Next

Real Studio lives at `studio/`: Tauri host, `platform/` API, lint against host imports in features, one real feature (local notes), NSIS current-user. Prototypes stay for comparison.
