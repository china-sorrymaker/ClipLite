# ClipLite

ClipLite is a lightweight desktop clipboard manager built with Tauri 2, Vue 3, TypeScript, Pinia, Vite, and SQLite.

The MVP focuses on text clipboard history only. Image clipboard, cloud sync, and AI features are intentionally out of scope.

## Features

- Monitor clipboard text changes
- Save clipboard history to SQLite
- Show and search clipboard history
- Click or double-click history items to copy them back to the clipboard
- Favorite and unfavorite items
- Delete items
- Dark mode interface
- Global shortcut: `Ctrl+Alt+V`
- Starts hidden in the Windows system tray
- System tray icon with show and quit actions
- Tray icon click toggles the main window
- Closing the window hides it to tray instead of exiting
- Optional Windows auto-start setting
- Frameless rounded glass UI
- Auto-focused search field
- `Esc` hides the window
- `Enter` copies the selected item and hides the window
- Full keyboard navigation
- Customizable global shortcut
- Separate Favorites tab
- Usage-frequency ranking
- Content type detection and lightweight syntax highlighting
- Deduplicate repeated clipboard text
- Keep the latest 1000 records

## Keyboard Controls

```text
Ctrl+Alt+V       Show or hide ClipLite by default
Esc              Hide window
Enter            Copy selected clip and hide window
Up / Down        Move selection
Home / End       Jump to first or last result
Tab              Switch All/Favorites tabs
Ctrl+1           All tab
Ctrl+2           Favorites tab
```

The global shortcut can be changed in the Settings panel. Examples: `Ctrl+Alt+V`, `Ctrl+Shift+Space`, `Alt+Space`.

## Prerequisites

- Node.js 18 or newer
- npm
- Rust stable toolchain with Cargo
- Tauri 2 system dependencies for your OS

On Windows, install Rust from:

```bash
https://rustup.rs/
```

Tauri's platform prerequisite guide:

```bash
https://tauri.app/start/prerequisites/
```

## Setup

```bash
npm install
```

## Run In Development

```bash
npm run tauri:dev
```

## Build

```bash
npm run tauri:build
```

## Windows Production Packaging

ClipLite is configured to build an NSIS Windows installer executable.

```bash
npm run build
npm run tauri:build
```

The installer is generated under:

```bash
src-tauri/target/release/bundle/nsis/
```

The production app starts hidden in the system tray. Use the tray icon or `Ctrl+Alt+V` to show or hide the main window. Closing the window hides it to the tray; use the tray menu's `Quit` item to exit the app.

## Frontend-Only Development

This starts only the Vite dev server. Tauri commands are unavailable in a normal browser tab, so clipboard history features require `npm run tauri:dev`.

```bash
npm run dev
```

## Data Storage

ClipLite stores its SQLite database in the application data directory selected by Tauri for your operating system. The database file is named:

```bash
cliplite.sqlite
```
