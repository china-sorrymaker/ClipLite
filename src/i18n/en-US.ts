export default {
  app: {
    name: 'ClipLite'
  },
  launcher: {
    searchPlaceholder: 'Search clipboard history',
    clipboardViews: 'Clipboard views',
    empty: 'No clips found',
    copied: 'copied',
    pastes: 'pastes',
    minutes: '{count}m',
    hours: '{count}h'
  },
  tabs: {
    recent: 'Recent',
    favorites: 'Favorites',
    frequent: 'Frequent'
  },
  settings: {
    title: 'ClipLite Settings',
    appearance: 'Appearance',
    language: 'Language',
    languageFollowSystem: 'Follow System',
    languageChinese: 'Simplified Chinese',
    languageEnglish: 'English',
    theme: 'Theme',
    themeFollowSystem: 'Follow System',
    themeDark: 'Dark',
    themeLight: 'Light',
    transparency: 'Transparency',
    transparencyRecommended: '88% Recommended',
    popupPosition: 'Popup Position',
    popupMouse: 'Mouse Cursor',
    popupCenter: 'Screen Center',
    shortcutSection: 'Shortcut',
    shortcut: 'Global Shortcut',
    recordShortcut: 'Record Shortcut',
    pressShortcut: 'Press shortcut...',
    history: 'History',
    historyCleanupMode: 'Cleanup Mode',
    cleanupByTime: 'By Time',
    cleanupByCount: 'By Count',
    cleanupNever: 'Never',
    historyRetention: 'History Retention',
    retentionDays: '{count} days',
    retentionItems: '{count} items',
    historyFavoritesNote: 'Favorites are kept',
    cleanNow: 'Clean Now',
    startup: 'Startup',
    startWithWindows: 'Start with Windows'
  },
  actions: {
    settings: 'Settings',
    favorite: 'Favorite',
    removeFavorite: 'Remove favorite',
    copy: 'Copy',
    delete: 'Delete'
  },
  common: {
    save: 'Save',
    cancel: 'Cancel',
    close: 'Close'
  },
  contentType: {
    plain: 'plain',
    url: 'url',
    email: 'email',
    json: 'json',
    code: 'code'
  },
  error: {
    shortcutModifier: 'Shortcut must include at least one modifier.',
    shortcutKeyRequired: 'Shortcut must include at least one modifier and one key.',
    unsupportedShortcutModifier: 'Unsupported shortcut modifier: {value}',
    unsupportedShortcutKey: 'Unsupported shortcut key: {value}',
    settingsWindowNotFound: 'Settings window not found',
    mainWindowNotFound: 'Main window not found',
    databaseLockFailed: 'Database lock failed'
  },
  tray: {
    show: 'Show ClipLite',
    settings: 'Settings',
    quit: 'Quit'
  }
} as const;
