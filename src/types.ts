export interface ClipboardItem {
  id: number;
  text: string;
  created_at: number;
  updated_at: number;
  is_favorite: boolean;
  usage_count: number;
  content_type: 'plain' | 'url' | 'email' | 'json' | 'code';
}

export type ThemeMode = 'system' | 'dark' | 'light';
export type PopupPosition = 'mouse' | 'center';
export type PasteStrategy = 'auto' | 'browser' | 'native';

export interface LauncherSettings {
  theme: ThemeMode;
  popupPosition: PopupPosition;
  pasteStrategy: PasteStrategy;
  pasteDelayMs: 100 | 200 | 300 | 500;
  width: number;
  height: number;
  transparency: number;
}
