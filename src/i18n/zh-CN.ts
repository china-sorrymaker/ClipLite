export default {
  app: {
    name: 'ClipLite'
  },
  launcher: {
    searchPlaceholder: '搜索剪贴板历史',
    clipboardViews: '剪贴板视图',
    empty: '没有找到剪贴内容',
    copied: '已复制',
    pastes: '次粘贴',
    minutes: '{count} 分钟',
    hours: '{count} 小时'
  },
  tabs: {
    recent: '最近',
    favorites: '收藏',
    frequent: '常用'
  },
  settings: {
    title: 'ClipLite 设置',
    appearance: '外观',
    language: '语言',
    languageFollowSystem: '跟随系统',
    languageChinese: '简体中文',
    languageEnglish: 'English',
    theme: '主题',
    themeFollowSystem: '跟随系统',
    themeDark: '深色',
    themeLight: '浅色',
    transparency: '透明度',
    transparencyRecommended: '88% 推荐',
    popupPosition: '弹窗位置',
    popupMouse: '鼠标光标',
    popupCenter: '屏幕居中',
    shortcutSection: '快捷键',
    shortcut: '全局快捷键',
    recordShortcut: '录制快捷键',
    pressShortcut: '请按快捷键...',
    history: '历史',
    historyRetention: '历史保留',
    retentionItems: '{count} 条',
    startup: '启动',
    startWithWindows: '开机自启动'
  },
  actions: {
    settings: '设置',
    favorite: '收藏',
    removeFavorite: '取消收藏',
    copy: '复制',
    delete: '删除'
  },
  common: {
    save: '保存',
    cancel: '取消',
    close: '关闭'
  },
  contentType: {
    plain: '文本',
    url: '链接',
    email: '邮箱',
    json: 'JSON',
    code: '代码'
  },
  error: {
    shortcutModifier: '快捷键必须包含至少一个修饰键。',
    shortcutKeyRequired: '快捷键必须包含至少一个修饰键和一个按键。',
    unsupportedShortcutModifier: '不支持的快捷键修饰键：{value}',
    unsupportedShortcutKey: '不支持的快捷键按键：{value}',
    settingsWindowNotFound: '未找到设置窗口',
    mainWindowNotFound: '未找到主窗口',
    databaseLockFailed: '数据库锁定失败'
  },
  tray: {
    show: '显示 ClipLite',
    settings: '设置',
    quit: '退出'
  }
} as const;
