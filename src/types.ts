export interface ClipboardItem {
  id: number;
  text: string;
  created_at: number;
  updated_at: number;
  is_favorite: boolean;
  favorite_at: number | null;
  is_deleted: boolean;
  copy_count: number;
  paste_count: number;
  last_copied_at: number;
  last_pasted_at: number | null;
  content_type: 'plain' | 'url' | 'email' | 'json' | 'code' | 'file';
  file_paths: string | null;
  file_count: number | null;
}

export type ThemeMode = 'system' | 'dark' | 'light';
export type LanguageMode = 'system' | 'zh-CN' | 'en-US';
export type PopupPosition = 'mouse' | 'center';
export type PasteStrategy = 'auto' | 'standard' | 'replace';

export interface LauncherSettings {
  language: LanguageMode;
  theme: ThemeMode;
  popupPosition: PopupPosition;
  pasteStrategy: PasteStrategy;
  pasteDelayMs: 100 | 200 | 300 | 500;
  width: number;
  height: number;
  transparency: number;
  historyRetention: 100 | 500 | 1000 | 5000 | 10000;
}
