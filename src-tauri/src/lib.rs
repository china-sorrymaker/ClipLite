use arboard::Clipboard;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tauri::{
    image::Image,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    App, AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, State, WindowEvent,
};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt as AutostartExt};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

const HISTORY_LIMIT: i64 = 1000;
const DEFAULT_SHORTCUT: &str = "control+alt+KeyV";
const DEFAULT_WIDTH: u32 = 620;
const DEFAULT_HEIGHT: u32 = 460;
const DEFAULT_TRANSPARENCY: u8 = 88;
const DEFAULT_PASTE_DELAY_MS: u64 = 200;
const DEFAULT_HISTORY_CLEANUP_MODE: &str = "count";
const DEFAULT_HISTORY_RETENTION_DAYS: i64 = 30;
const DEFAULT_HISTORY_RETENTION: i64 = HISTORY_LIMIT;
const MIN_WIDTH: u32 = 420;
const MAX_WIDTH: u32 = 900;
const MIN_HEIGHT: u32 = 320;
const MAX_HEIGHT: u32 = 720;

type Db = Arc<Mutex<Connection>>;
type ActiveShortcut = Arc<Mutex<Shortcut>>;
type PreviousInteraction = Arc<Mutex<InteractionSnapshot>>;
type LauncherVisible = Arc<Mutex<bool>>;
type TrayMenuItemsState = Arc<Mutex<Option<TrayMenuItems>>>;

#[derive(Clone)]
struct AppState {
    db: Db,
    shortcut: ActiveShortcut,
    previous_interaction: PreviousInteraction,
    launcher_visible: LauncherVisible,
    tray_menu_items: TrayMenuItemsState,
}

#[derive(Clone)]
struct TrayMenuItems {
    show: MenuItem<tauri::Wry>,
    settings: MenuItem<tauri::Wry>,
    quit: MenuItem<tauri::Wry>,
}

#[derive(Debug, Default, Clone)]
struct InteractionSnapshot {
    hwnd: Option<isize>,
    mouse: Option<(i32, i32)>,
    caret: Option<(i32, i32)>,
    process_name: Option<String>,
    focused_control_hwnd: Option<isize>,
    caret_hwnd: Option<isize>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct LauncherSettings {
    language: String,
    theme: String,
    popup_position: String,
    paste_strategy: String,
    paste_delay_ms: u64,
    width: u32,
    height: u32,
    transparency: u8,
    history_cleanup_mode: String,
    history_retention_days: i64,
    history_retention: i64,
}

#[derive(Debug, Serialize)]
struct ClipboardItem {
    id: i64,
    text: String,
    created_at: i64,
    updated_at: i64,
    is_favorite: bool,
    favorite_at: Option<i64>,
    is_deleted: bool,
    copy_count: i64,
    paste_count: i64,
    last_copied_at: i64,
    last_pasted_at: Option<i64>,
    content_type: String,
}

#[derive(Debug, Serialize, Clone)]
struct HistoryUpdatedPayload {
    item_id: Option<i64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CleanHistoryResult {
    cleaned_count: usize,
    mode: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DeleteItemResult {
    id: i64,
    existed_before: bool,
    affected_rows: usize,
    exists_after: bool,
}

#[tauri::command]
fn list_items(state: State<'_, AppState>, tab: Option<String>) -> Result<Vec<ClipboardItem>, String> {
    let conn = state
        .db
        .lock()
        .map_err(|_| "Database lock failed".to_string())?;
    let sql = items_query(tab.as_deref());
    query_items(&conn, sql, params![HISTORY_LIMIT])
}

#[tauri::command]
fn search_items(
    state: State<'_, AppState>,
    query: String,
    tab: Option<String>,
) -> Result<Vec<ClipboardItem>, String> {
    let conn = state
        .db
        .lock()
        .map_err(|_| "Database lock failed".to_string())?;
    let pattern = format!("%{}%", query);

    let sql = search_query(tab.as_deref());
    query_items(&conn, sql, params![pattern, HISTORY_LIMIT])
}

#[tauri::command]
fn copy_item(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    copy_item_to_clipboard(&state, id, false)
}

#[tauri::command]
fn paste_item(app: AppHandle, state: State<'_, AppState>, id: i64) -> Result<(), String> {
    eprintln!("[paste_item] requested item_id={id}");
    let snapshot = state
        .previous_interaction
        .lock()
        .map_err(|_| "Previous interaction lock failed".to_string())?
        .clone();
    eprintln!(
        "[paste_item] using previous HWND={:?} process={:?} mouse={:?} caret={:?}",
        snapshot.hwnd, snapshot.process_name, snapshot.mouse, snapshot.caret
    );
    let settings = {
        let conn = state
            .db
            .lock()
            .map_err(|_| "Database lock failed".to_string())?;
        load_launcher_settings(&conn)?
    };

    copy_item_to_clipboard(&state, id, true)?;
    eprintln!("[paste_item] text copied to clipboard for item_id={id}");

    hide_launcher(&app, "paste")?;

    paste_into_previous_window(snapshot, settings.paste_strategy, settings.paste_delay_ms)?;
    eprintln!("[paste_item] paste sequence completed for item_id={id}");

    Ok(())
}

fn copy_item_to_clipboard(state: &State<'_, AppState>, id: i64, record_paste: bool) -> Result<(), String> {
    let text = {
        let conn = state
            .db
            .lock()
            .map_err(|_| "Database lock failed".to_string())?;
        conn.query_row(
            "SELECT text FROM clipboard_items WHERE id = ?1 AND is_deleted = 0",
            params![id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "Clipboard item not found".to_string())?
    };

    Clipboard::new()
        .map_err(|error| error.to_string())?
        .set_text(text)
        .map_err(|error| error.to_string())?;

    let conn = state
        .db
        .lock()
        .map_err(|_| "Database lock failed".to_string())?;
    let timestamp = now_timestamp();
    if record_paste {
        conn.execute(
            "UPDATE clipboard_items
             SET paste_count = paste_count + 1,
                 last_pasted_at = ?1,
                 updated_at = ?1
             WHERE id = ?2",
            params![timestamp, id],
        )
        .map_err(|error| error.to_string())?;
    } else {
        conn.execute(
            "UPDATE clipboard_items
             SET copy_count = copy_count + 1,
                 last_copied_at = ?1,
                 updated_at = ?1
             WHERE id = ?2",
            params![timestamp, id],
        )
        .map_err(|error| error.to_string())?;
    }

    Ok(())
}

#[tauri::command]
fn toggle_favorite(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    let conn = state
        .db
        .lock()
        .map_err(|_| "Database lock failed".to_string())?;
    conn.execute(
        "UPDATE clipboard_items
         SET is_favorite = CASE is_favorite WHEN 1 THEN 0 ELSE 1 END,
             favorite_at = CASE is_favorite WHEN 1 THEN 0 ELSE ?1 END
         WHERE id = ?2",
        params![now_timestamp(), id],
    )
    .map_err(|error| error.to_string())?;

    Ok(())
}

#[tauri::command]
fn delete_item(app: AppHandle, state: State<'_, AppState>, id: i64) -> Result<DeleteItemResult, String> {
    eprintln!("[delete_item] command entered");
    eprintln!("[delete_item] record id={id}");

    let conn = state
        .db
        .lock()
        .map_err(|_| "Database lock failed".to_string())?;

    let existed_before = clipboard_item_exists(&conn, id)?;
    eprintln!("[delete_item] exists before delete={existed_before}");
    eprintln!("[delete_item] SQL: DELETE FROM clipboard_items WHERE id = ?1");

    let affected_rows = conn
        .execute(
            "DELETE FROM clipboard_items WHERE id = ?1",
            params![id],
        )
        .map_err(|error| error.to_string())?;

    eprintln!("[delete_item] affected rows={affected_rows}");

    let exists_after = clipboard_item_exists(&conn, id)?;
    eprintln!("[delete_item] exists after delete={exists_after}");

    app.emit(
        "history-updated",
        HistoryUpdatedPayload { item_id: None },
    )
    .map_err(|error| error.to_string())?;

    Ok(DeleteItemResult {
        id,
        existed_before,
        affected_rows,
        exists_after,
    })
}

fn clipboard_item_exists(conn: &Connection, id: i64) -> Result<bool, String> {
    let count = conn
        .query_row(
            "SELECT COUNT(1) FROM clipboard_items WHERE id = ?1",
            params![id],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|error| error.to_string())?;

    Ok(count > 0)
}

#[tauri::command]
fn get_autostart_enabled(app: AppHandle) -> Result<bool, String> {
    app.autolaunch()
        .is_enabled()
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn set_autostart_enabled(app: AppHandle, enabled: bool) -> Result<bool, String> {
    if enabled {
        app.autolaunch()
            .enable()
            .map_err(|error| error.to_string())?;
    } else {
        app.autolaunch()
            .disable()
            .map_err(|error| error.to_string())?;
    }

    app.autolaunch()
        .is_enabled()
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn hide_window(app: AppHandle, reason: Option<String>) -> Result<(), String> {
    hide_launcher(&app, reason.as_deref().unwrap_or("command"))
}

#[tauri::command]
fn open_settings_window(app: AppHandle) -> Result<(), String> {
    let Some(window) = app.get_webview_window("settings") else {
        return Err("Settings window not found".to_string());
    };

    window.center().map_err(|error| error.to_string())?;
    window.show().map_err(|error| error.to_string())?;
    window.set_focus().map_err(|error| error.to_string())?;
    Ok(())
}

#[tauri::command]
fn get_global_shortcut(state: State<'_, AppState>) -> Result<String, String> {
    let conn = state
        .db
        .lock()
        .map_err(|_| "Database lock failed".to_string())?;
    get_setting(&conn, "global_shortcut").map(|value| {
        display_shortcut(&value.unwrap_or_else(|| state.shortcut.lock().unwrap().to_string()))
    })
}

#[tauri::command]
fn set_global_shortcut(
    app: AppHandle,
    state: State<'_, AppState>,
    shortcut: String,
) -> Result<String, String> {
    let normalized = normalize_shortcut(&shortcut)?;
    let next_shortcut = normalized
        .parse::<Shortcut>()
        .map_err(|error| error.to_string())?;

    let mut active_shortcut = state
        .shortcut
        .lock()
        .map_err(|_| "Shortcut lock failed".to_string())?;
    let previous_shortcut = *active_shortcut;

    app.global_shortcut()
        .unregister(previous_shortcut)
        .map_err(|error| error.to_string())?;

    if let Err(error) = app.global_shortcut().register(next_shortcut) {
        let _ = app.global_shortcut().register(previous_shortcut);
        return Err(error.to_string());
    }

    *active_shortcut = next_shortcut;

    let conn = state
        .db
        .lock()
        .map_err(|_| "Database lock failed".to_string())?;
    set_setting(&conn, "global_shortcut", &normalized)?;

    Ok(display_shortcut(&normalized))
}

#[tauri::command]
fn get_launcher_settings(state: State<'_, AppState>) -> Result<LauncherSettings, String> {
    let conn = state
        .db
        .lock()
        .map_err(|_| "Database lock failed".to_string())?;
    load_launcher_settings(&conn)
}

#[tauri::command]
fn set_launcher_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: LauncherSettings,
) -> Result<LauncherSettings, String> {
    let settings = sanitize_launcher_settings(settings);
    {
        let conn = state
            .db
            .lock()
            .map_err(|_| "Database lock failed".to_string())?;
        save_launcher_settings(&conn, &settings)?;
    }

    apply_launcher_window_size(&app, &settings)?;
    update_tray_language(&app, &settings);
    app.emit("settings-updated", settings.clone())
        .map_err(|error| error.to_string())?;
    Ok(settings)
}

#[tauri::command]
fn clean_history_now(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: LauncherSettings,
) -> Result<CleanHistoryResult, String> {
    let settings = sanitize_launcher_settings(settings);
    let cleaned_count = {
        let conn = state
            .db
            .lock()
            .map_err(|_| "Database lock failed".to_string())?;
        save_launcher_settings(&conn, &settings)?;
        clean_history(&conn, &settings)?
    };

    app.emit("history-updated", HistoryUpdatedPayload { item_id: None })
        .map_err(|error| error.to_string())?;
    app.emit("settings-updated", settings.clone())
        .map_err(|error| error.to_string())?;

    Ok(CleanHistoryResult {
        cleaned_count,
        mode: settings.history_cleanup_mode,
    })
}

pub fn run() {
    let active_shortcut = Arc::new(Mutex::new(Shortcut::new(
        Some(Modifiers::CONTROL | Modifiers::ALT),
        Code::KeyV,
    )));
    let previous_interaction = Arc::new(Mutex::new(InteractionSnapshot::default()));
    let launcher_visible = Arc::new(Mutex::new(false));
    let tray_menu_items = Arc::new(Mutex::new(None));
    let handler_shortcut = active_shortcut.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec!["--from-autostart"]),
        ))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(move |app, shortcut, event| {
                    let Ok(active_shortcut) = handler_shortcut.lock() else {
                        return;
                    };

                    if shortcut == &*active_shortcut && event.state() == ShortcutState::Pressed {
                        eprintln!("[launcher] shortcut pressed");
                        toggle_launcher(app, "global shortcut");
                    }
                })
                .build(),
        )
        .setup(move |app| {
            let db = open_database(app)?;
            initialize_database(&db)?;

            let shortcut = load_shortcut(&db).unwrap_or_else(|error| {
                eprintln!("Shortcut setting failed to load: {error}");
                Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyV)
            });
            let launcher_settings = {
                let conn = db.lock().map_err(|_| "Database lock failed")?;
                load_launcher_settings(&conn).unwrap_or_else(|error| {
                    eprintln!("Launcher settings failed to load: {error}");
                    default_launcher_settings()
                })
            };

            *active_shortcut.lock().map_err(|_| "Shortcut lock failed")? = shortcut;

            app.manage(AppState {
                db: db.clone(),
                shortcut: active_shortcut.clone(),
                previous_interaction: previous_interaction.clone(),
                launcher_visible: launcher_visible.clone(),
                tray_menu_items: tray_menu_items.clone(),
            });
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_skip_taskbar(true);
            }
            let _ = hide_launcher(app.handle(), "startup");
            eprintln!("[launcher] startup visible state initialized false");
            apply_launcher_window_size(app.handle(), &launcher_settings)?;
            setup_tray(app)?;
            app.global_shortcut().register(shortcut)?;
            start_clipboard_monitor(app.handle().clone(), db);

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_items,
            search_items,
            copy_item,
            paste_item,
            toggle_favorite,
            delete_item,
            get_autostart_enabled,
            set_autostart_enabled,
            hide_window,
            open_settings_window,
            get_global_shortcut,
            set_global_shortcut,
            get_launcher_settings,
            set_launcher_settings,
            clean_history_now
        ])
        .on_window_event(|window, event| {
            let label = window.label();
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                if label == "main" {
                    let _ = hide_launcher(window.app_handle(), "close");
                } else if label == "settings" {
                    let _ = window.hide();
                }
            } else if label == "main" {
                if let WindowEvent::Focused(false) = event {
                    let _ = hide_launcher(window.app_handle(), "blur");
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("failed to run ClipLite");
}

fn open_database(app: &App) -> Result<Db, Box<dyn std::error::Error>> {
    let app_data_dir = app.path().app_data_dir()?;
    fs::create_dir_all(&app_data_dir)?;
    let db_path = app_data_dir.join("cliplite.sqlite");
    Ok(Arc::new(Mutex::new(Connection::open(db_path)?)))
}

fn initialize_database(db: &Db) -> Result<(), Box<dyn std::error::Error>> {
    let conn = db.lock().map_err(|_| "Database lock failed")?;
    conn.execute_batch(
        "
        PRAGMA journal_mode = WAL;
        CREATE TABLE IF NOT EXISTS clipboard_items (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            text TEXT NOT NULL UNIQUE,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            is_favorite INTEGER NOT NULL DEFAULT 0,
            usage_count INTEGER NOT NULL DEFAULT 0,
            favorite_at INTEGER DEFAULT NULL,
            is_deleted INTEGER NOT NULL DEFAULT 0,
            copy_count INTEGER NOT NULL DEFAULT 0,
            paste_count INTEGER NOT NULL DEFAULT 0,
            last_copied_at INTEGER NOT NULL DEFAULT 0,
            last_pasted_at INTEGER DEFAULT NULL,
            content_type TEXT NOT NULL DEFAULT 'plain'
        );
        CREATE INDEX IF NOT EXISTS idx_clipboard_items_updated_at
            ON clipboard_items(updated_at DESC);
        CREATE TABLE IF NOT EXISTS app_settings (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        ",
    )?;
    add_column_if_missing(
        &conn,
        "clipboard_items",
        "usage_count",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    add_column_if_missing(
        &conn,
        "clipboard_items",
        "content_type",
        "TEXT NOT NULL DEFAULT 'plain'",
    )?;
    add_column_if_missing(
        &conn,
        "clipboard_items",
        "favorite_at",
        "INTEGER DEFAULT NULL",
    )?;
    add_column_if_missing(
        &conn,
        "clipboard_items",
        "is_deleted",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    add_column_if_missing(
        &conn,
        "clipboard_items",
        "copy_count",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    add_column_if_missing(
        &conn,
        "clipboard_items",
        "paste_count",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    add_column_if_missing(
        &conn,
        "clipboard_items",
        "last_copied_at",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    add_column_if_missing(
        &conn,
        "clipboard_items",
        "last_pasted_at",
        "INTEGER DEFAULT NULL",
    )?;
    ensure_nullable_clipboard_timestamps(&conn)?;
    conn.execute_batch(
        "
        CREATE INDEX IF NOT EXISTS idx_clipboard_items_last_copied_at
            ON clipboard_items(last_copied_at DESC);
        CREATE INDEX IF NOT EXISTS idx_clipboard_items_favorite_at
            ON clipboard_items(favorite_at DESC);
        CREATE INDEX IF NOT EXISTS idx_clipboard_items_paste_count
            ON clipboard_items(paste_count DESC, last_pasted_at DESC);
        UPDATE clipboard_items
        SET last_copied_at = CASE WHEN last_copied_at = 0 THEN updated_at ELSE last_copied_at END,
            copy_count = CASE WHEN copy_count = 0 THEN 1 ELSE copy_count END,
            paste_count = CASE WHEN paste_count = 0 THEN usage_count ELSE paste_count END,
            last_pasted_at = CASE
                WHEN (last_pasted_at IS NULL OR last_pasted_at = 0) AND usage_count > 0 THEN updated_at
                WHEN last_pasted_at = 0 THEN NULL
                ELSE last_pasted_at
            END,
            favorite_at = CASE
                WHEN (favorite_at IS NULL OR favorite_at = 0) AND is_favorite = 1 THEN updated_at
                WHEN favorite_at = 0 THEN NULL
                ELSE favorite_at
            END;
        ",
    )?;

    Ok(())
}

fn setup_tray(app: &mut App) -> tauri::Result<()> {
    let settings = {
        let state = app.state::<AppState>();
        load_launcher_settings_from_state(&state)
    };
    let locale = resolve_rust_locale(&settings.language);
    let show = MenuItem::with_id(app, "show", tray_text(locale, "show"), true, None::<&str>)?;
    let settings_item = MenuItem::with_id(app, "settings", tray_text(locale, "settings"), true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", tray_text(locale, "quit"), true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &settings_item, &quit])?;

    {
        let state = app.state::<AppState>();
        if let Ok(mut items) = state.tray_menu_items.lock() {
            *items = Some(TrayMenuItems {
                show: show.clone(),
                settings: settings_item.clone(),
                quit: quit.clone(),
            });
        };
    }

    let builder = TrayIconBuilder::with_id("main")
        .icon(create_tray_icon())
        .tooltip("ClipLite")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_main_window(app),
            "settings" => {
                let _ = open_settings_window(app.clone());
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                toggle_launcher(tray.app_handle(), "tray");
            }
        });

    builder.build(app)?;

    Ok(())
}

#[derive(Clone, Copy)]
enum RustLocale {
    En,
    Zh,
}

fn resolve_rust_locale(language: &str) -> RustLocale {
    match language {
        "zh-CN" => RustLocale::Zh,
        "en-US" => RustLocale::En,
        _ => system_rust_locale(),
    }
}

#[cfg(target_os = "windows")]
fn system_rust_locale() -> RustLocale {
    use windows::Win32::Globalization::GetUserDefaultLocaleName;

    let mut buffer = [0u16; 85];
    let len = unsafe { GetUserDefaultLocaleName(&mut buffer) };
    if len > 0 {
        let locale = String::from_utf16_lossy(&buffer[..len.saturating_sub(1) as usize]);
        if locale.to_ascii_lowercase().starts_with("zh") {
            return RustLocale::Zh;
        }
    }

    RustLocale::En
}

#[cfg(not(target_os = "windows"))]
fn system_rust_locale() -> RustLocale {
    std::env::var("LANG")
        .ok()
        .filter(|value| value.to_ascii_lowercase().starts_with("zh"))
        .map(|_| RustLocale::Zh)
        .unwrap_or(RustLocale::En)
}

fn tray_text(locale: RustLocale, key: &str) -> &'static str {
    match (locale, key) {
        (RustLocale::Zh, "show") => "显示 ClipLite",
        (RustLocale::Zh, "settings") => "设置",
        (RustLocale::Zh, "quit") => "退出",
        (_, "show") => "Show ClipLite",
        (_, "settings") => "Settings",
        (_, "quit") => "Quit",
        _ => "",
    }
}

fn update_tray_language(app: &AppHandle, settings: &LauncherSettings) {
    let locale = resolve_rust_locale(&settings.language);
    let state = app.state::<AppState>();
    let Ok(items) = state.tray_menu_items.lock() else {
        return;
    };
    let Some(items) = items.as_ref() else {
        return;
    };

    let _ = items.show.set_text(tray_text(locale, "show"));
    let _ = items.settings.set_text(tray_text(locale, "settings"));
    let _ = items.quit.set_text(tray_text(locale, "quit"));
}

fn create_tray_icon() -> Image<'static> {
    const SIZE: u32 = 32;
    let mut rgba = Vec::with_capacity((SIZE * SIZE * 4) as usize);

    for y in 0..SIZE {
        for x in 0..SIZE {
            let in_page = (7..=25).contains(&x) && (4..=28).contains(&y);
            let in_clip = (11..=21).contains(&x) && (2..=8).contains(&y);
            let in_line =
                (11..=21).contains(&x) && ((13..=15).contains(&y) || (19..=21).contains(&y));

            let pixel = if in_clip {
                [231, 198, 107, 255]
            } else if in_line {
                [244, 241, 234, 255]
            } else if in_page {
                [47, 125, 108, 255]
            } else {
                [0, 0, 0, 0]
            };

            rgba.extend_from_slice(&pixel);
        }
    }

    Image::new_owned(rgba, SIZE, SIZE)
}

fn start_clipboard_monitor(app: AppHandle, db: Db) {
    thread::spawn(move || {
        let mut clipboard = match Clipboard::new() {
            Ok(clipboard) => clipboard,
            Err(error) => {
                eprintln!("Clipboard monitor failed to start: {error}");
                return;
            }
        };
        let mut last_text = String::new();

        loop {
            if let Ok(text) = clipboard.get_text() {
                if should_store_text(&text, &last_text) {
                    last_text = text.clone();

                    match upsert_clipboard_text(&db, &text) {
                        Ok(item_id) => {
                            let _ = app.emit(
                                "history-updated",
                                HistoryUpdatedPayload {
                                    item_id: Some(item_id),
                                },
                            );
                        }
                        Err(error) => eprintln!("Clipboard save failed: {error}"),
                    }
                }
            }

            thread::sleep(Duration::from_millis(650));
        }
    });
}

fn should_store_text(text: &str, last_text: &str) -> bool {
    !text.trim().is_empty() && text != last_text
}

fn upsert_clipboard_text(db: &Db, text: &str) -> Result<i64, String> {
    let conn = db.lock().map_err(|_| "Database lock failed".to_string())?;
    let timestamp = now_timestamp();
    let content_type = detect_content_type(text);

    let existing_id = conn
        .query_row(
            "SELECT id FROM clipboard_items WHERE text = ?1",
            params![text],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;

    let item_id = if let Some(id) = existing_id {
        conn.execute(
            "UPDATE clipboard_items
             SET updated_at = ?1,
                 last_copied_at = ?1,
                 copy_count = copy_count + 1,
                 content_type = ?2,
                 is_deleted = 0
             WHERE id = ?3",
            params![timestamp, content_type, id],
        )
        .map_err(|error| error.to_string())?;
        id
    } else {
        conn.execute(
            "INSERT INTO clipboard_items (
                text,
                created_at,
                updated_at,
                copy_count,
                last_copied_at,
                content_type
             )
             VALUES (?1, ?2, ?2, 1, ?2, ?3)",
            params![text, timestamp, content_type],
        )
        .map_err(|error| error.to_string())?;
        conn.last_insert_rowid()
    };

    let settings = load_launcher_settings(&conn)?;
    let _ = clean_history(&conn, &settings)?;

    Ok(item_id)
}

fn clean_history(conn: &Connection, settings: &LauncherSettings) -> Result<usize, String> {
    let mode = settings.history_cleanup_mode.as_str();
    let total_before = count_clipboard_items(conn)?;

    eprintln!("[clean_history] cleanup mode={mode}");
    eprintln!(
        "[clean_history] selected days={} count={}",
        settings.history_retention_days, settings.history_retention
    );
    eprintln!("[clean_history] total records before={total_before}");

    let eligible_count = count_cleanup_eligible_items(conn, settings)?;
    eprintln!("[clean_history] eligible non-favorite records={eligible_count}");

    let cleaned_count = match mode {
        "time" => {
            let cutoff = now_timestamp() - settings.history_retention_days * 24 * 60 * 60;
            eprintln!(
                "[clean_history] SQL: DELETE FROM clipboard_items WHERE is_favorite = 0 AND last_copied_at < ?1"
            );
            conn.execute(
                "DELETE FROM clipboard_items
                 WHERE is_favorite = 0
                   AND last_copied_at < ?1",
                params![cutoff],
            )
            .map_err(|error| error.to_string())?
        }
        "count" => {
            eprintln!(
                "[clean_history] SQL: DELETE FROM clipboard_items WHERE is_favorite = 0 AND id NOT IN (SELECT id FROM clipboard_items WHERE is_favorite = 0 ORDER BY last_copied_at DESC, id DESC LIMIT ?1)"
            );
            conn.execute(
                "DELETE FROM clipboard_items
                 WHERE is_favorite = 0
                   AND id NOT IN (
                    SELECT id FROM clipboard_items
                    WHERE is_favorite = 0
                    ORDER BY last_copied_at DESC, id DESC
                    LIMIT ?1
                 )",
                params![settings.history_retention],
            )
            .map_err(|error| error.to_string())?
        }
        "never" => {
            eprintln!("[clean_history] SQL: none; auto cleanup is disabled");
            0
        }
        _ => {
            eprintln!("[clean_history] SQL: none; unsupported cleanup mode");
            0
        }
    };

    let total_after = count_clipboard_items(conn)?;
    eprintln!("[clean_history] affected rows={cleaned_count}");
    eprintln!("[clean_history] total records after={total_after}");

    Ok(cleaned_count)
}

fn count_clipboard_items(conn: &Connection) -> Result<i64, String> {
    conn.query_row("SELECT COUNT(1) FROM clipboard_items", [], |row| {
        row.get::<_, i64>(0)
    })
    .map_err(|error| error.to_string())
}

fn count_cleanup_eligible_items(conn: &Connection, settings: &LauncherSettings) -> Result<i64, String> {
    match settings.history_cleanup_mode.as_str() {
        "time" => {
            let cutoff = now_timestamp() - settings.history_retention_days * 24 * 60 * 60;
            conn.query_row(
                "SELECT COUNT(1)
                 FROM clipboard_items
                 WHERE is_favorite = 0
                   AND last_copied_at < ?1",
                params![cutoff],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|error| error.to_string())
        }
        "count" => conn
            .query_row(
                "SELECT COUNT(1)
                 FROM clipboard_items
                 WHERE is_favorite = 0
                   AND id NOT IN (
                    SELECT id FROM clipboard_items
                    WHERE is_favorite = 0
                    ORDER BY last_copied_at DESC, id DESC
                    LIMIT ?1
                 )",
                params![settings.history_retention],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|error| error.to_string()),
        "never" => Ok(0),
        _ => Ok(0),
    }
}

fn items_query(tab: Option<&str>) -> &'static str {
    match tab.unwrap_or("recent") {
        "favorites" => {
            "SELECT id, text, created_at, updated_at, is_favorite, favorite_at, is_deleted,
                    copy_count, paste_count, last_copied_at, last_pasted_at, content_type
             FROM clipboard_items
             WHERE is_deleted = 0 AND is_favorite = 1
             ORDER BY favorite_at DESC, last_copied_at DESC
             LIMIT ?1"
        }
        "frequent" => {
            "SELECT id, text, created_at, updated_at, is_favorite, favorite_at, is_deleted,
                    copy_count, paste_count, last_copied_at, last_pasted_at, content_type
             FROM clipboard_items
             WHERE is_deleted = 0 AND paste_count > 0
             ORDER BY paste_count DESC, last_pasted_at DESC
             LIMIT ?1"
        }
        _ => {
            "SELECT id, text, created_at, updated_at, is_favorite, favorite_at, is_deleted,
                    copy_count, paste_count, last_copied_at, last_pasted_at, content_type
             FROM clipboard_items
             WHERE is_deleted = 0
             ORDER BY last_copied_at DESC
             LIMIT ?1"
        }
    }
}

fn search_query(tab: Option<&str>) -> &'static str {
    match tab.unwrap_or("recent") {
        "favorites" => {
            "SELECT id, text, created_at, updated_at, is_favorite, favorite_at, is_deleted,
                    copy_count, paste_count, last_copied_at, last_pasted_at, content_type
             FROM clipboard_items
             WHERE is_deleted = 0 AND is_favorite = 1 AND text LIKE ?1
             ORDER BY favorite_at DESC, last_copied_at DESC
             LIMIT ?2"
        }
        "frequent" => {
            "SELECT id, text, created_at, updated_at, is_favorite, favorite_at, is_deleted,
                    copy_count, paste_count, last_copied_at, last_pasted_at, content_type
             FROM clipboard_items
             WHERE is_deleted = 0 AND paste_count > 0 AND text LIKE ?1
             ORDER BY paste_count DESC, last_pasted_at DESC
             LIMIT ?2"
        }
        _ => {
            "SELECT id, text, created_at, updated_at, is_favorite, favorite_at, is_deleted,
                    copy_count, paste_count, last_copied_at, last_pasted_at, content_type
             FROM clipboard_items
             WHERE is_deleted = 0 AND text LIKE ?1
             ORDER BY last_copied_at DESC
             LIMIT ?2"
        }
    }
}

fn query_items<P>(conn: &Connection, sql: &str, params: P) -> Result<Vec<ClipboardItem>, String>
where
    P: rusqlite::Params,
{
    let mut statement = conn.prepare(sql).map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params, |row| {
            Ok(ClipboardItem {
                id: row.get(0)?,
                text: row.get(1)?,
                created_at: row.get(2)?,
                updated_at: row.get(3)?,
                is_favorite: row.get::<_, i64>(4)? == 1,
                favorite_at: row.get(5)?,
                is_deleted: row.get::<_, i64>(6)? == 1,
                copy_count: row.get(7)?,
                paste_count: row.get(8)?,
                last_copied_at: row.get(9)?,
                last_pasted_at: row.get(10)?,
                content_type: row.get(11)?,
            })
        })
        .map_err(|error| error.to_string())?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

fn now_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default()
}

fn add_column_if_missing(
    conn: &Connection,
    table: &str,
    column: &str,
    definition: &str,
) -> Result<(), rusqlite::Error> {
    let mut statement = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let columns = statement.query_map([], |row| row.get::<_, String>(1))?;

    for existing_column in columns {
        if existing_column? == column {
            return Ok(());
        }
    }

    conn.execute(
        &format!("ALTER TABLE {table} ADD COLUMN {column} {definition}"),
        [],
    )?;
    Ok(())
}

fn ensure_nullable_clipboard_timestamps(conn: &Connection) -> rusqlite::Result<()> {
    let favorite_not_null = column_is_not_null(conn, "clipboard_items", "favorite_at")?;
    let pasted_not_null = column_is_not_null(conn, "clipboard_items", "last_pasted_at")?;
    if !favorite_not_null && !pasted_not_null {
        return Ok(());
    }

    conn.execute_batch(
        "
        DROP TABLE IF EXISTS clipboard_items_migrated;
        CREATE TABLE clipboard_items_migrated (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            text TEXT NOT NULL UNIQUE,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            is_favorite INTEGER NOT NULL DEFAULT 0,
            usage_count INTEGER NOT NULL DEFAULT 0,
            favorite_at INTEGER DEFAULT NULL,
            is_deleted INTEGER NOT NULL DEFAULT 0,
            copy_count INTEGER NOT NULL DEFAULT 0,
            paste_count INTEGER NOT NULL DEFAULT 0,
            last_copied_at INTEGER NOT NULL DEFAULT 0,
            last_pasted_at INTEGER DEFAULT NULL,
            content_type TEXT NOT NULL DEFAULT 'plain'
        );
        INSERT INTO clipboard_items_migrated (
            id,
            text,
            created_at,
            updated_at,
            is_favorite,
            usage_count,
            favorite_at,
            is_deleted,
            copy_count,
            paste_count,
            last_copied_at,
            last_pasted_at,
            content_type
        )
        SELECT
            id,
            text,
            created_at,
            updated_at,
            is_favorite,
            usage_count,
            NULLIF(favorite_at, 0),
            is_deleted,
            copy_count,
            paste_count,
            last_copied_at,
            NULLIF(last_pasted_at, 0),
            content_type
        FROM clipboard_items;
        DROP TABLE clipboard_items;
        ALTER TABLE clipboard_items_migrated RENAME TO clipboard_items;
        ",
    )
}

fn column_is_not_null(conn: &Connection, table: &str, column: &str) -> rusqlite::Result<bool> {
    let mut statement = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let rows = statement.query_map([], |row| {
        Ok((row.get::<_, String>(1)?, row.get::<_, i64>(3)?))
    })?;

    for row in rows {
        let (name, not_null) = row?;
        if name == column {
            return Ok(not_null != 0);
        }
    }

    Ok(false)
}

fn load_shortcut(db: &Db) -> Result<Shortcut, String> {
    let conn = db.lock().map_err(|_| "Database lock failed".to_string())?;
    let value =
        get_setting(&conn, "global_shortcut")?.unwrap_or_else(|| DEFAULT_SHORTCUT.to_string());
    value.parse::<Shortcut>().map_err(|error| error.to_string())
}

fn get_setting(conn: &Connection, key: &str) -> Result<Option<String>, String> {
    conn.query_row(
        "SELECT value FROM app_settings WHERE key = ?1",
        params![key],
        |row| row.get::<_, String>(0),
    )
    .optional()
    .map_err(|error| error.to_string())
}

fn set_setting(conn: &Connection, key: &str, value: &str) -> Result<(), String> {
    conn.execute(
        "INSERT INTO app_settings (key, value)
         VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )
    .map_err(|error| error.to_string())?;

    Ok(())
}

fn default_launcher_settings() -> LauncherSettings {
    LauncherSettings {
        language: "system".to_string(),
        theme: "system".to_string(),
        popup_position: "mouse".to_string(),
        paste_strategy: "auto".to_string(),
        paste_delay_ms: DEFAULT_PASTE_DELAY_MS,
        width: DEFAULT_WIDTH,
        height: DEFAULT_HEIGHT,
        transparency: DEFAULT_TRANSPARENCY,
        history_cleanup_mode: DEFAULT_HISTORY_CLEANUP_MODE.to_string(),
        history_retention_days: DEFAULT_HISTORY_RETENTION_DAYS,
        history_retention: DEFAULT_HISTORY_RETENTION,
    }
}

fn load_launcher_settings(conn: &Connection) -> Result<LauncherSettings, String> {
    let defaults = default_launcher_settings();
    Ok(sanitize_launcher_settings(LauncherSettings {
        language: get_setting(conn, "language")?.unwrap_or(defaults.language),
        theme: get_setting(conn, "theme")?.unwrap_or(defaults.theme),
        popup_position: get_setting(conn, "popup_position")?.unwrap_or(defaults.popup_position),
        paste_strategy: get_setting(conn, "paste_strategy")?.unwrap_or(defaults.paste_strategy),
        paste_delay_ms: get_setting(conn, "paste_delay_ms")?
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(defaults.paste_delay_ms),
        width: get_setting(conn, "window_width")?
            .and_then(|value| value.parse::<u32>().ok())
            .unwrap_or(defaults.width),
        height: get_setting(conn, "window_height")?
            .and_then(|value| value.parse::<u32>().ok())
            .unwrap_or(defaults.height),
        transparency: get_setting(conn, "transparency")?
            .and_then(|value| value.parse::<u8>().ok())
            .unwrap_or(defaults.transparency),
        history_cleanup_mode: get_setting(conn, "history_cleanup_mode")?
            .unwrap_or(defaults.history_cleanup_mode),
        history_retention_days: get_setting(conn, "history_retention_days")?
            .and_then(|value| value.parse::<i64>().ok())
            .unwrap_or(defaults.history_retention_days),
        history_retention: get_setting(conn, "history_retention")?
            .and_then(|value| value.parse::<i64>().ok())
            .unwrap_or(defaults.history_retention),
    }))
}

fn save_launcher_settings(conn: &Connection, settings: &LauncherSettings) -> Result<(), String> {
    set_setting(conn, "language", &settings.language)?;
    set_setting(conn, "theme", &settings.theme)?;
    set_setting(conn, "popup_position", &settings.popup_position)?;
    set_setting(conn, "paste_strategy", &settings.paste_strategy)?;
    set_setting(conn, "paste_delay_ms", &settings.paste_delay_ms.to_string())?;
    set_setting(conn, "window_width", &settings.width.to_string())?;
    set_setting(conn, "window_height", &settings.height.to_string())?;
    set_setting(conn, "transparency", &settings.transparency.to_string())?;
    set_setting(conn, "history_cleanup_mode", &settings.history_cleanup_mode)?;
    set_setting(
        conn,
        "history_retention_days",
        &settings.history_retention_days.to_string(),
    )?;
    set_setting(conn, "history_retention", &settings.history_retention.to_string())?;
    Ok(())
}

fn sanitize_launcher_settings(settings: LauncherSettings) -> LauncherSettings {
    LauncherSettings {
        language: match settings.language.as_str() {
            "system" | "zh-CN" | "en-US" => settings.language,
            _ => "system".to_string(),
        },
        theme: match settings.theme.as_str() {
            "light" | "dark" | "system" => settings.theme,
            _ => "system".to_string(),
        },
        popup_position: match settings.popup_position.as_str() {
            "mouse" | "center" => settings.popup_position,
            _ => "mouse".to_string(),
        },
        paste_strategy: match settings.paste_strategy.as_str() {
            "auto" | "standard" | "replace" => settings.paste_strategy,
            "browser" | "native" => "standard".to_string(),
            _ => "auto".to_string(),
        },
        paste_delay_ms: match settings.paste_delay_ms {
            100 | 200 | 300 | 500 => settings.paste_delay_ms,
            _ => DEFAULT_PASTE_DELAY_MS,
        },
        width: settings.width.clamp(MIN_WIDTH, MAX_WIDTH),
        height: settings.height.clamp(MIN_HEIGHT, MAX_HEIGHT),
        transparency: match settings.transparency {
            70 | 80 | 88 | 90 | 100 => settings.transparency,
            _ => DEFAULT_TRANSPARENCY,
        },
        history_cleanup_mode: match settings.history_cleanup_mode.as_str() {
            "time" | "count" | "never" => settings.history_cleanup_mode,
            _ => DEFAULT_HISTORY_CLEANUP_MODE.to_string(),
        },
        history_retention_days: match settings.history_retention_days {
            1 | 7 | 30 | 90 => settings.history_retention_days,
            _ => DEFAULT_HISTORY_RETENTION_DAYS,
        },
        history_retention: match settings.history_retention {
            100 | 500 | 1000 | 5000 | 10000 => settings.history_retention,
            _ => DEFAULT_HISTORY_RETENTION,
        },
    }
}

fn apply_launcher_window_size(app: &AppHandle, settings: &LauncherSettings) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("main") {
        window
            .set_size(PhysicalSize::new(settings.width, settings.height))
            .map_err(|error| error.to_string())?;
        eprintln!(
            "[window] applied launcher size {}x{}",
            settings.width, settings.height
        );
    }

    Ok(())
}

fn normalize_shortcut(value: &str) -> Result<String, String> {
    let parts = value
        .split('+')
        .map(|part| part.trim())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();

    if parts.len() < 2 {
        return Err("Shortcut must include at least one modifier and one key".to_string());
    }

    let mut normalized = Vec::with_capacity(parts.len());
    for (index, part) in parts.iter().enumerate() {
        let upper = part.to_ascii_uppercase();
        let is_key = index == parts.len() - 1;

        let token = if !is_key {
            match upper.as_str() {
                "CTRL" | "CONTROL" => "control".to_string(),
                "ALT" | "OPTION" => "alt".to_string(),
                "SHIFT" => "shift".to_string(),
                "WIN" | "WINDOWS" | "SUPER" | "CMD" | "COMMAND" => "super".to_string(),
                _ => return Err(format!("Unsupported shortcut modifier: {part}")),
            }
        } else if upper.len() == 1 && upper.as_bytes()[0].is_ascii_alphanumeric() {
            format!("Key{upper}")
        } else if upper.starts_with('F') && upper[1..].parse::<u8>().is_ok() {
            upper
        } else if upper.starts_with("KEY") {
            format!("Key{}", &upper[3..])
        } else {
            match upper.as_str() {
                "`" | "~" | "BACKQUOTE" => "Backquote".to_string(),
                "SPACE" => "Space".to_string(),
                "TAB" => "Tab".to_string(),
                "ENTER" => "Enter".to_string(),
                "ESC" | "ESCAPE" => "Escape".to_string(),
                "ARROWUP" | "UP" => "ArrowUp".to_string(),
                "ARROWDOWN" | "DOWN" => "ArrowDown".to_string(),
                "ARROWLEFT" | "LEFT" => "ArrowLeft".to_string(),
                "ARROWRIGHT" | "RIGHT" => "ArrowRight".to_string(),
                "BACKSPACE" => "Backspace".to_string(),
                "DELETE" | "DEL" => "Delete".to_string(),
                _ => return Err(format!("Unsupported shortcut key: {part}")),
            }
        };

        normalized.push(token);
    }

    let shortcut = normalized.join("+");
    shortcut
        .parse::<Shortcut>()
        .map_err(|error| error.to_string())?;
    Ok(shortcut)
}

fn display_shortcut(shortcut: &str) -> String {
    shortcut
        .split('+')
        .map(|part| match part {
            "control" => "Ctrl".to_string(),
            "alt" => "Alt".to_string(),
            "shift" => "Shift".to_string(),
            "super" => "Win".to_string(),
            key if key.starts_with("Key") => key.trim_start_matches("Key").to_string(),
            "Backquote" => "`".to_string(),
            key => key.to_string(),
        })
        .collect::<Vec<_>>()
        .join("+")
}

fn detect_content_type(text: &str) -> String {
    let trimmed = text.trim();
    let lower = trimmed.to_ascii_lowercase();

    if lower.starts_with("http://") || lower.starts_with("https://") {
        "url"
    } else if trimmed.contains('@') && trimmed.contains('.') && !trimmed.contains(' ') {
        "email"
    } else if (trimmed.starts_with('{') && trimmed.ends_with('}'))
        || (trimmed.starts_with('[') && trimmed.ends_with(']'))
    {
        "json"
    } else if trimmed.contains("function ")
        || trimmed.contains("const ")
        || trimmed.contains("let ")
        || trimmed.contains("class ")
        || trimmed.contains("=>")
        || trimmed.contains("#include")
        || trimmed.contains("SELECT ")
        || trimmed.contains("select ")
    {
        "code"
    } else {
        "plain"
    }
    .to_string()
}

fn toggle_launcher(app: &AppHandle, reason: &str) {
    let real_visible = app
        .get_webview_window("main")
        .and_then(|window| window.is_visible().ok())
        .unwrap_or(false);
    let state = app.state::<AppState>();
    let tracked_visible = state.launcher_visible.lock().map(|visible| *visible).unwrap_or(false);
    eprintln!(
        "[launcher] toggle reason={reason} current visible state tracked={tracked_visible} real={real_visible}"
    );

    if real_visible {
        let _ = hide_launcher(app, reason);
    } else {
        let _ = show_launcher(app);
    }
}

fn show_main_window(app: &AppHandle) {
    let _ = show_launcher(app);
}

fn show_launcher(app: &AppHandle) -> Result<(), String> {
    eprintln!("[launcher] show_launcher called");
    let Some(window) = app.get_webview_window("main") else {
        return Err("Main window not found".to_string());
    };

    let snapshot = capture_interaction(app);
    let state = app.state::<AppState>();
    if let Ok(mut previous) = state.previous_interaction.lock() {
        *previous = snapshot.clone();
    }
    let settings = load_launcher_settings_from_state(&state);
    apply_launcher_window_size(app, &settings)?;
    position_main_window(app, &settings, &snapshot);
    window.show().map_err(|error| error.to_string())?;
    window.set_focus().map_err(|error| error.to_string())?;
    if let Ok(mut visible) = state.launcher_visible.lock() {
        *visible = true;
        eprintln!("[launcher] updated visible state=true");
    }
    app.emit("window-shown", ()).map_err(|error| error.to_string())?;
    Ok(())
}

fn hide_launcher(app: &AppHandle, reason: &str) -> Result<(), String> {
    eprintln!("[launcher] hide_launcher called reason={reason}");
    if let Some(window) = app.get_webview_window("main") {
        let real_visible = window.is_visible().unwrap_or(false);
        eprintln!("[launcher] hide current real visible={real_visible}");
        if real_visible {
            window.hide().map_err(|error| error.to_string())?;
        }
    } else {
        eprintln!("[launcher] hide skipped: main window not found");
    }

    let state = app.state::<AppState>();
    if let Ok(mut visible) = state.launcher_visible.lock() {
        *visible = false;
        eprintln!("[launcher] updated visible state=false");
    }

    Ok(())
}

fn load_launcher_settings_from_state(state: &State<'_, AppState>) -> LauncherSettings {
    state
        .db
        .lock()
        .ok()
        .and_then(|conn| load_launcher_settings(&conn).ok())
        .unwrap_or_else(default_launcher_settings)
}

fn position_main_window(
    app: &AppHandle,
    settings: &LauncherSettings,
    snapshot: &InteractionSnapshot,
) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };

    let anchor = snapshot.caret.or(snapshot.mouse);
    let Some(monitor) = monitor_for_anchor(&window, anchor) else {
        eprintln!("[window] no monitor available for popup positioning");
        return;
    };
    let area = monitor.work_area();
    let left = area.position.x;
    let top = area.position.y;
    let right = left + area.size.width as i32;
    let bottom = top + area.size.height as i32;
    let width = settings.width as i32;
    let height = settings.height as i32;

    let (raw_x, raw_y) = if settings.popup_position == "center" {
        (left + (area.size.width as i32 - width) / 2, top + (area.size.height as i32 - height) / 2)
    } else if let Some((anchor_x, anchor_y)) = anchor {
        (anchor_x - width / 2, anchor_y + 18)
    } else {
        (left + (area.size.width as i32 - width) / 2, top + (area.size.height as i32 - height) / 2)
    };

    let x = raw_x.clamp(left, right - width);
    let y = raw_y.clamp(top, bottom - height);
    match window.set_position(PhysicalPosition::new(x, y)) {
        Ok(()) => eprintln!(
            "[window] positioned popup at x={x} y={y} mode={} anchor={anchor:?}",
            settings.popup_position
        ),
        Err(error) => eprintln!("[window] failed to position popup: {error}"),
    }
}

fn monitor_for_anchor(
    window: &tauri::WebviewWindow,
    anchor: Option<(i32, i32)>,
) -> Option<tauri::Monitor> {
    let monitors = window.available_monitors().ok()?;
    if let Some((x, y)) = anchor {
        if let Some(monitor) = monitors.iter().find(|monitor| {
            let area = monitor.work_area();
            let left = area.position.x;
            let top = area.position.y;
            let right = left + area.size.width as i32;
            let bottom = top + area.size.height as i32;
            x >= left && x < right && y >= top && y < bottom
        }) {
            return Some(monitor.clone());
        }
    }

    window.current_monitor().ok().flatten().or_else(|| window.primary_monitor().ok().flatten())
}

#[cfg(target_os = "windows")]
fn capture_interaction(app: &AppHandle) -> InteractionSnapshot {
    use windows::Win32::{
        Foundation::POINT,
        Graphics::Gdi::ClientToScreen,
        UI::WindowsAndMessaging::{
            GetCursorPos, GetForegroundWindow, GetGUIThreadInfo, GetWindowThreadProcessId,
            GUITHREADINFO,
        },
    };

    let mut snapshot = InteractionSnapshot::default();

    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.0.is_null() {
        eprintln!("[paste_item] no foreground hwnd to remember");
    } else if is_cliplite_window(app, hwnd.0 as isize) {
        eprintln!("[paste_item] foreground hwnd is ClipLite; previous hwnd not changed");
    } else {
        let hwnd_value = hwnd.0 as isize;
        snapshot.hwnd = Some(hwnd_value);
        snapshot.process_name = previous_process_name(hwnd);
        eprintln!("[paste_item] previous HWND captured: 0x{hwnd_value:x}");
        eprintln!("[paste_item] previous process name: {:?}", snapshot.process_name);

        let thread_id = unsafe { GetWindowThreadProcessId(hwnd, None) };
        let mut gui = GUITHREADINFO {
            cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
            ..Default::default()
        };
        if thread_id != 0 && unsafe { GetGUIThreadInfo(thread_id, &mut gui).is_ok() } {
            if !gui.hwndFocus.0.is_null() {
                snapshot.focused_control_hwnd = Some(gui.hwndFocus.0 as isize);
            }
            if !gui.hwndCaret.0.is_null() {
                snapshot.caret_hwnd = Some(gui.hwndCaret.0 as isize);
            }
            eprintln!(
                "[window] focused control hwnd={:?} caret hwnd={:?}",
                snapshot.focused_control_hwnd, snapshot.caret_hwnd
            );
            if !gui.hwndCaret.0.is_null() {
                let mut caret = POINT {
                    x: gui.rcCaret.left,
                    y: gui.rcCaret.bottom,
                };
                if unsafe { ClientToScreen(gui.hwndCaret, &mut caret).as_bool() } {
                    snapshot.caret = Some((caret.x, caret.y));
                    eprintln!("[window] caret position captured: x={} y={}", caret.x, caret.y);
                } else {
                    eprintln!("[window] caret position conversion failed");
                }
            } else {
                eprintln!("[window] no caret hwnd available from previous HWND");
            }
        } else {
            eprintln!("[window] no focused control info available from previous HWND");
        }
    }

    let mut point = POINT::default();
    if unsafe { GetCursorPos(&mut point).is_ok() } {
        snapshot.mouse = Some((point.x, point.y));
        eprintln!("[window] mouse position captured: x={} y={}", point.x, point.y);
    } else {
        eprintln!("[window] failed to capture mouse position");
    }

    snapshot
}

#[cfg(not(target_os = "windows"))]
fn capture_interaction(_: &AppHandle) -> InteractionSnapshot {
    InteractionSnapshot::default()
}

#[cfg(target_os = "windows")]
fn is_cliplite_window(app: &AppHandle, hwnd: isize) -> bool {
    app.get_webview_window("main")
        .and_then(|window| window.hwnd().ok())
        .map(|cliplite_hwnd| cliplite_hwnd.0 as isize == hwnd)
        .unwrap_or(false)
}

#[cfg(target_os = "windows")]
fn previous_process_name(hwnd: windows::Win32::Foundation::HWND) -> Option<String> {
    use windows::{
        core::PWSTR,
        Win32::{
            Foundation::CloseHandle,
            System::Threading::{
                OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_FORMAT,
                PROCESS_QUERY_LIMITED_INFORMATION,
            },
            UI::WindowsAndMessaging::GetWindowThreadProcessId,
        },
    };

    let mut process_id = 0;
    unsafe {
        GetWindowThreadProcessId(hwnd, Some(&mut process_id));
    }
    if process_id == 0 {
        return None;
    }

    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id).ok()? };
    let mut buffer = vec![0u16; 1024];
    let mut size = buffer.len() as u32;
    let result = unsafe {
        QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_FORMAT(0),
            PWSTR(buffer.as_mut_ptr()),
            &mut size,
        )
    };
    let _ = unsafe { CloseHandle(process) };
    result.ok()?;

    let path = String::from_utf16_lossy(&buffer[..size as usize]);
    path.rsplit(['\\', '/']).next().map(|name| name.to_string())
}

#[cfg(target_os = "windows")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PasteStrategy {
    Browser,
    Standard,
    NativeReplace,
}

#[cfg(target_os = "windows")]
fn select_paste_strategy(requested: &str, process_name: Option<&str>) -> PasteStrategy {
    let process = process_name.unwrap_or_default().to_ascii_lowercase();
    let is_browser = matches!(process.as_str(), "chrome.exe" | "msedge.exe" | "firefox.exe");
    let is_native_chat = matches!(
        process.as_str(),
        "wechat.exe" | "weixin.exe" | "qq.exe" | "feishu.exe" | "lark.exe" | "dingtalk.exe"
    );
    match requested {
        "standard" => return PasteStrategy::Standard,
        "replace" => return PasteStrategy::NativeReplace,
        _ => {}
    }

    if is_browser {
        PasteStrategy::Browser
    } else if is_native_chat {
        PasteStrategy::NativeReplace
    } else {
        PasteStrategy::Standard
    }
}

#[cfg(target_os = "windows")]
fn paste_into_previous_window(
    snapshot: InteractionSnapshot,
    requested_strategy: String,
    delay_ms: u64,
) -> Result<(), String> {
    use windows::Win32::{
        Foundation::HWND,
        System::Threading::{AttachThreadInput, GetCurrentThreadId},
        UI::{
            Input::KeyboardAndMouse::{
                GetFocus, SendInput, SetFocus, INPUT, VK_CONTROL, VK_DELETE, VK_MENU, VK_TAB, VK_V,
            },
            WindowsAndMessaging::{
                GetForegroundWindow, GetLastActivePopup, GetWindowPlacement,
                GetWindowThreadProcessId, IsWindow, IsZoomed, SetForegroundWindow,
                SetWindowPlacement, WINDOWPLACEMENT,
            },
        },
    };

    let hwnd = snapshot
        .hwnd
        .ok_or_else(|| "No previous application to paste into".to_string())?;
    let original_hwnd = HWND(hwnd as *mut core::ffi::c_void);
    let strategy = select_paste_strategy(&requested_strategy, snapshot.process_name.as_deref());
    let strategy_log = match strategy {
        PasteStrategy::Browser => "StandardPaste",
        PasteStrategy::Standard => "StandardPaste",
        PasteStrategy::NativeReplace => "NativeReplaceExperimental",
    };
    eprintln!("[paste_item] previous process name: {:?}", snapshot.process_name);
    eprintln!(
        "[paste_item] selected strategy: {strategy_log} internal={strategy:?} requested={requested_strategy} delay={delay_ms}ms"
    );
    eprintln!("[paste_item] previous HWND=0x{hwnd:x}");

    if strategy == PasteStrategy::NativeReplace {
        eprintln!("[paste_item] NativeReplace restores immediately after hiding ClipLite");
    } else {
        eprintln!("[paste_item] wait {delay_ms}ms after hiding ClipLite");
        thread::sleep(Duration::from_millis(delay_ms));
    }

    let mut target_hwnd = original_hwnd;
    let mut target_valid = unsafe { IsWindow(Some(target_hwnd)).as_bool() };
    eprintln!("[paste_item] IsWindow(previous_hwnd)={target_valid}");
    if !target_valid {
        eprintln!("[paste_item] previous HWND invalid: 0x{hwnd:x}");
        let popup = unsafe { GetLastActivePopup(original_hwnd) };
        if !popup.0.is_null() && unsafe { IsWindow(Some(popup)).as_bool() } {
            target_hwnd = popup;
            target_valid = true;
            eprintln!("[paste_item] using GetLastActivePopup fallback HWND={:?}", popup.0);
        } else {
            eprintln!("[paste_item] GetLastActivePopup fallback unavailable; will try Alt+Tab");
        }
    }

    let placement_before = if target_valid {
        let mut placement = WINDOWPLACEMENT {
            length: std::mem::size_of::<WINDOWPLACEMENT>() as u32,
            ..Default::default()
        };
        match unsafe { GetWindowPlacement(target_hwnd, &mut placement) } {
            Ok(()) => {
                log_window_placement("before paste", &placement);
                Some(placement)
            }
            Err(error) => {
                eprintln!("[paste_item] GetWindowPlacement before paste failed: {error}");
                None
            }
        }
    } else {
        None
    };
    let zoomed_before = target_valid && unsafe { IsZoomed(target_hwnd).as_bool() };
    eprintln!("[paste_item] IsZoomed(previous_hwnd)={zoomed_before}");
    eprintln!("[paste_item] restoring previous foreground HWND={:?}", target_hwnd.0);

    let restore_start = Instant::now();
    let focused = if target_valid && strategy == PasteStrategy::Browser {
        let current_thread = unsafe { GetCurrentThreadId() };
        let target_thread = unsafe { GetWindowThreadProcessId(target_hwnd, None) };
        let foreground_before = unsafe { GetForegroundWindow() };
        let foreground_thread = if foreground_before.0.is_null() {
            0
        } else {
            unsafe { GetWindowThreadProcessId(foreground_before, None) }
        };
        eprintln!(
            "[paste_item] focus threads current={current_thread} previous={target_thread} foreground={foreground_thread}"
        );

        let attach_current = target_thread != 0
            && current_thread != target_thread
            && unsafe { AttachThreadInput(current_thread, target_thread, true).as_bool() };
        let attach_foreground = target_thread != 0
            && foreground_thread != 0
            && foreground_thread != target_thread
            && unsafe { AttachThreadInput(foreground_thread, target_thread, true).as_bool() };
        eprintln!(
            "[paste_item] AttachThreadInput current={attach_current} foreground={attach_foreground}"
        );

        let result = unsafe {
            let foreground_result = SetForegroundWindow(target_hwnd).as_bool();
            let foreground_after = GetForegroundWindow();
            eprintln!(
                "[paste_item] SetForegroundWindow={foreground_result} foreground={:?}",
                foreground_after.0
            );
            foreground_result && foreground_after == target_hwnd
        };

        if attach_foreground {
            let detached = unsafe { AttachThreadInput(foreground_thread, target_thread, false).as_bool() };
            eprintln!("[paste_item] detach foreground thread={detached}");
        }
        if attach_current {
            let detached = unsafe { AttachThreadInput(current_thread, target_thread, false).as_bool() };
            eprintln!("[paste_item] detach current thread={detached}");
        }

        result
    } else if target_valid {
        let focused_control_hwnd = snapshot
            .focused_control_hwnd
            .map(|hwnd| HWND(hwnd as *mut core::ffi::c_void));
        let focused_control_valid = focused_control_hwnd
            .map(|hwnd| unsafe { IsWindow(Some(hwnd)).as_bool() })
            .unwrap_or(false);
        let foreground_before_paste = unsafe { GetForegroundWindow() };
        eprintln!("[paste_item] Native restore previous_hwnd={:?}", target_hwnd.0);
        eprintln!(
            "[paste_item] Native restore focused_control_hwnd={:?}",
            focused_control_hwnd.map(|hwnd| hwnd.0)
        );
        eprintln!("[paste_item] Native restore IsWindow(focused_control_hwnd)={focused_control_valid}");
        eprintln!(
            "[paste_item] Native restore GetForegroundWindow before paste={:?}",
            foreground_before_paste.0
        );

        let current_thread = unsafe { GetCurrentThreadId() };
        let target_thread = unsafe { GetWindowThreadProcessId(target_hwnd, None) };
        let foreground_thread = if foreground_before_paste.0.is_null() {
            0
        } else {
            unsafe { GetWindowThreadProcessId(foreground_before_paste, None) }
        };
        let focused_control_thread = focused_control_hwnd
            .filter(|_| focused_control_valid)
            .map(|hwnd| unsafe { GetWindowThreadProcessId(hwnd, None) })
            .unwrap_or(0);
        eprintln!(
            "[paste_item] Native restore threads current={current_thread} previous={target_thread} foreground={foreground_thread} focused_control={focused_control_thread}"
        );

        let attach_current_target = target_thread != 0
            && current_thread != target_thread
            && unsafe { AttachThreadInput(current_thread, target_thread, true).as_bool() };
        let attach_foreground_target = target_thread != 0
            && foreground_thread != 0
            && foreground_thread != target_thread
            && unsafe { AttachThreadInput(foreground_thread, target_thread, true).as_bool() };
        let attach_current_control = focused_control_thread != 0
            && focused_control_thread != target_thread
            && focused_control_thread != current_thread
            && unsafe { AttachThreadInput(current_thread, focused_control_thread, true).as_bool() };
        eprintln!(
            "[paste_item] Native restore AttachThreadInput current_target={attach_current_target} foreground_target={attach_foreground_target} current_control={attach_current_control}"
        );

        let result = unsafe {
            let foreground_result = SetForegroundWindow(target_hwnd).as_bool();
            let foreground_after = GetForegroundWindow();
            eprintln!(
                "[paste_item] native/simple SetForegroundWindow={foreground_result} foreground={:?}",
                foreground_after.0
            );
            if let Some(control_hwnd) = focused_control_hwnd.filter(|_| focused_control_valid) {
                let previous_focus = SetFocus(Some(control_hwnd));
                eprintln!(
                    "[paste_item] Native restore SetFocus focused_control_hwnd={:?} previous_focus={:?}",
                    control_hwnd.0,
                    previous_focus.as_ref().map(|hwnd| hwnd.0)
                );
                if let Err(error) = previous_focus {
                    eprintln!("[paste_item] Native restore SetFocus failed: {error}");
                }
            } else {
                eprintln!("[paste_item] Native restore skipped SetFocus: no valid focused_control_hwnd");
            }
            let focus_after = GetFocus();
            eprintln!(
                "[paste_item] Native restore GetFocus result after restore={:?}",
                focus_after.0
            );
            foreground_result && foreground_after == target_hwnd
        };

        if attach_current_control {
            let detached = unsafe { AttachThreadInput(current_thread, focused_control_thread, false).as_bool() };
            eprintln!("[paste_item] Native restore detach current/control thread={detached}");
        }
        if attach_foreground_target {
            let detached = unsafe { AttachThreadInput(foreground_thread, target_thread, false).as_bool() };
            eprintln!("[paste_item] Native restore detach foreground/target thread={detached}");
        }
        if attach_current_target {
            let detached = unsafe { AttachThreadInput(current_thread, target_thread, false).as_bool() };
            eprintln!("[paste_item] Native restore detach current/target thread={detached}");
        }

        result
    } else {
        false
    };
    eprintln!(
        "[paste_item] foreground restore time {:?}",
        restore_start.elapsed()
    );

    if !focused && strategy == PasteStrategy::Browser {
        eprintln!("[paste_item] SetForegroundWindow did not restore target; sending Alt+Tab fallback");
        let alt_tab = [
            keyboard_input(VK_MENU.0, false),
            keyboard_input(VK_TAB.0, false),
            keyboard_input(VK_TAB.0, true),
            keyboard_input(VK_MENU.0, true),
        ];
        let sent = unsafe { SendInput(&alt_tab, std::mem::size_of::<INPUT>() as i32) };
        eprintln!("[paste_item] Alt+Tab fallback sent={sent}/{}", alt_tab.len());
        thread::sleep(Duration::from_millis(delay_ms));

        if target_valid {
            let (retry, foreground) = unsafe {
                (
                    SetForegroundWindow(target_hwnd).as_bool(),
                    GetForegroundWindow(),
                )
            };
            eprintln!(
                "[paste_item] retry SetForegroundWindow={retry} foreground={:?}",
                foreground.0
            );
        }
    }

    let foreground = unsafe { GetForegroundWindow() };
    if foreground == target_hwnd {
        eprintln!("[paste_item] previous foreground HWND restored: {:?}", target_hwnd.0);
    } else {
        eprintln!("[paste_item] foreground after restore attempts={:?}", foreground.0);
    }

    if strategy == PasteStrategy::NativeReplace {
        eprintln!("[paste_item] NativeReplace wait 200ms before Delete");
        thread::sleep(Duration::from_millis(200));
        let delete_input = [keyboard_input(VK_DELETE.0, false), keyboard_input(VK_DELETE.0, true)];
        let delete_start = Instant::now();
        let delete_sent = unsafe { SendInput(&delete_input, std::mem::size_of::<INPUT>() as i32) };
        eprintln!("[paste_item] NativeReplace delete send time {:?}", delete_start.elapsed());
        eprintln!(
            "[paste_item] NativeReplace Delete SendInput sent={delete_sent}/{}",
            delete_input.len()
        );
        eprintln!("[paste_item] NativeReplace wait 50ms before Ctrl+V");
        thread::sleep(Duration::from_millis(50));
    } else {
        eprintln!("[paste_item] wait {delay_ms}ms before Ctrl+V");
        thread::sleep(Duration::from_millis(delay_ms));
    }

    let inputs = [
        keyboard_input(VK_CONTROL.0, false),
        keyboard_input(VK_V.0, false),
        keyboard_input(VK_V.0, true),
        keyboard_input(VK_CONTROL.0, true),
    ];

    let paste_start = Instant::now();
    let foreground_before_paste = unsafe { GetForegroundWindow() };
    eprintln!(
        "[paste_item] current foreground HWND before paste={:?}",
        foreground_before_paste.0
    );
    let sent = unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
    eprintln!("[paste_item] paste send time {:?}", paste_start.elapsed());
    eprintln!("[paste_item] ctrl+v SendInput sent={sent}/{}", inputs.len());
    if let Some(before) = placement_before {
        let mut after = WINDOWPLACEMENT {
            length: std::mem::size_of::<WINDOWPLACEMENT>() as u32,
            ..Default::default()
        };
        match unsafe { GetWindowPlacement(target_hwnd, &mut after) } {
            Ok(()) => {
                log_window_placement("after paste", &after);
                if before != after {
                    eprintln!("[paste_item] window placement changed unexpectedly; restoring original WINDOWPLACEMENT");
                    if let Err(error) = unsafe { SetWindowPlacement(target_hwnd, &before) } {
                        eprintln!("[paste_item] SetWindowPlacement restore failed: {error}");
                    } else {
                        eprintln!("[paste_item] original WINDOWPLACEMENT restored");
                    }
                }
            }
            Err(error) => eprintln!("[paste_item] GetWindowPlacement after paste failed: {error}"),
        }
    }
    if sent == inputs.len() as u32 {
        eprintln!("[paste_item] Ctrl+V sent");
        Ok(())
    } else {
        Err("Failed to send paste shortcut".to_string())
    }
}

#[cfg(target_os = "windows")]
fn log_window_placement(
    label: &str,
    placement: &windows::Win32::UI::WindowsAndMessaging::WINDOWPLACEMENT,
) {
    eprintln!(
        "[paste_item] WINDOWPLACEMENT {label}: showCmd={} flags={} min=({}, {}) max=({}, {}) normal=({}, {}, {}, {})",
        placement.showCmd,
        placement.flags.0,
        placement.ptMinPosition.x,
        placement.ptMinPosition.y,
        placement.ptMaxPosition.x,
        placement.ptMaxPosition.y,
        placement.rcNormalPosition.left,
        placement.rcNormalPosition.top,
        placement.rcNormalPosition.right,
        placement.rcNormalPosition.bottom
    );
}

#[cfg(target_os = "windows")]
fn keyboard_input(virtual_key: u16, key_up: bool) -> windows::Win32::UI::Input::KeyboardAndMouse::INPUT {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VIRTUAL_KEY,
    };

    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(virtual_key),
                wScan: 0,
                dwFlags: if key_up { KEYEVENTF_KEYUP } else { Default::default() },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

#[cfg(not(target_os = "windows"))]
fn paste_into_previous_window(_: InteractionSnapshot, _: String, _: u64) -> Result<(), String> {
    Ok(())
}
