# ClipLite

English | [简体中文](./README.zh-CN.md)

A lightweight and modern clipboard launcher for Windows.

ClipLite helps you quickly search, reuse, favorite, and manage clipboard history with a clean launcher-style interface. It supports text, images, file paths, favorites, frequently used items, themes, transparency, and multi-language settings.

![ClipLite Screenshot](./docs/screenshot-main.png)

## Features

- Quickly open clipboard history with a global shortcut
- Search clipboard history by keywords
- Support text, images, and file paths
- Favorite important clipboard items
- Sort frequently used items by usage count
- Dark mode and light mode support
- Adjustable transparency, window size, and font size
- Window position settings
- Chinese and English language support
- Local SQLite storage
- Privacy-friendly: data stays on your device

## Download

Download the latest version from the [Releases](../../releases) page.

## Usage

1. Install and launch ClipLite
2. Use the global shortcut to open the clipboard panel
3. Type keywords to search your clipboard history
4. Press Enter to paste the selected item

## Screenshots

### Main Window

![Main Window](./docs/screenshot-main.png)

### Settings

![Settings](./docs/screenshot-settings.png)

## Tech Stack

- Tauri
- Vue 3
- TypeScript
- SQLite
- Rust

## Why ClipLite?

The built-in Windows clipboard is useful but limited.  
Traditional clipboard managers are powerful but often feel outdated.

ClipLite aims to provide a lightweight, modern, and fast clipboard experience, inspired by launcher-style tools like Raycast and PowerToys Run.

## Suitable For

- Developers who frequently copy and paste code snippets
- Office users who reuse common text
- Users who need to quickly search clipboard history
- People who prefer lightweight productivity tools

## Roadmap

- [x] Text clipboard history
- [x] Favorites
- [x] Frequently used items
- [x] Image clipboard support
- [x] File path support
- [x] Theme switching
- [x] Chinese / English language switching
- [x] Transparency settings
- [x] Window position settings
- [ ] More stable auto-start
- [ ] Update notification
- [ ] More paste modes
- [ ] Enhanced shortcut customization

## Privacy

ClipLite stores clipboard data locally in a SQLite database.  
Your clipboard data is not uploaded to any server.

## License

MIT
