<script setup lang="ts">
import { nextTick, onMounted, ref, watch } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import {
  Clipboard,
  Copy,
  File,
  Files,
  Folder,
  Hash,
  Keyboard,
  Link,
  Mail,
  Search,
  Settings,
  Star,
  StarOff,
  Trash2
} from '@lucide/vue';
import { useClipboardStore, type HistoryTab } from './stores/clipboard';
import { createTranslator, resolveLocale } from './i18n';
import type { ClipboardItem, LauncherSettings } from './types';

const store = useClipboardStore();
const currentWindow = getCurrentWindow();
const isSettingsWindow = currentWindow.label === 'settings';
const search = ref('');
const searchInput = ref<HTMLInputElement | null>(null);
const recordingShortcut = ref(false);
const fileMissingByItem = ref<Record<number, boolean>>({});
const fileFolderByItem = ref<Record<number, boolean>>({});

type FilePathStatus = {
  exists: boolean;
  isDir: boolean;
};

let searchTimer: number | undefined;
let systemThemeQuery: MediaQueryList | undefined;

watch(search, (value) => {
  window.clearTimeout(searchTimer);
  searchTimer = window.setTimeout(() => {
    void store.setSearch(value);
  }, 90);
});

watch(() => store.launcherSettings, applyVisualSettings, { deep: true });
watch(
  () => store.items.map((item) => `${item.id}:${item.file_paths ?? ''}`).join('|'),
  () => {
    void refreshFileStatuses();
  }
);

const t = (key: string, params?: Record<string, string | number>) =>
  createTranslator(store.launcherSettings.language)(key, params);

onMounted(async () => {
  await store.initialize();
  await refreshFileStatuses();
  applyVisualSettings();
  systemThemeQuery = window.matchMedia('(prefers-color-scheme: light)');
  systemThemeQuery.addEventListener('change', applyVisualSettings);
  focusSearch();

  await listen('window-shown', () => {
    void nextTick(focusSearch);
  });

  await listen<LauncherSettings>('settings-updated', (event) => {
    store.launcherSettings = event.payload;
    applyVisualSettings();
  });
});

function focusSearch() {
  searchInput.value?.focus();
  searchInput.value?.select();
}

function preview(text: string) {
  return text.replace(/\s+/g, ' ').trim();
}

function filePaths(item: ClipboardItem) {
  if (!item.file_paths) {
    return [] as string[];
  }

  try {
    return JSON.parse(item.file_paths) as string[];
  } catch {
    return [] as string[];
  }
}

function baseName(path: string) {
  return path.split(/[\\/]/).filter(Boolean).pop() ?? path;
}

function itemPreview(item: ClipboardItem) {
  if (item.content_type === 'file') {
    const paths = filePaths(item);
    if (paths.length === 0) {
      return 'File selection';
    }
    if (paths.length > 1) {
      return `${paths.length} files`;
    }

    return baseName(paths[0]);
  }

  return preview(item.text);
}

function fileMeta(item: ClipboardItem) {
  if (item.content_type !== 'file') {
    return contentTypeLabel(item.content_type);
  }

  const paths = filePaths(item);
  const missing = fileMissingByItem.value[item.id];
  const prefix = paths.length > 1
    ? 'Multiple files'
    : fileFolderByItem.value[item.id]
      ? 'Folder'
      : 'File';

  return missing ? `${prefix} / Missing` : prefix;
}

async function refreshFileStatuses() {
  const nextMissing: Record<number, boolean> = {};
  const nextFolder: Record<number, boolean> = {};
  for (const item of store.items) {
    if (item.content_type !== 'file' || !item.file_paths) {
      continue;
    }

    try {
      const statuses = await invoke<FilePathStatus[]>('file_paths_status', { pathsJson: item.file_paths });
      nextMissing[item.id] = statuses.some((status) => !status.exists);
      nextFolder[item.id] = statuses.length === 1 && statuses[0]?.isDir === true;
    } catch {
      nextMissing[item.id] = true;
    }
  }
  fileMissingByItem.value = nextMissing;
  fileFolderByItem.value = nextFolder;
}

function formatTime(timestamp: number) {
  const date = new Date(timestamp * 1000);
  const diff = Date.now() - date.getTime();
  const minute = 60 * 1000;
  const hour = 60 * minute;
  const day = 24 * hour;

  if (diff < hour) {
    return t('launcher.minutes', { count: Math.max(1, Math.round(diff / minute)) });
  }

  if (diff < day) {
    return t('launcher.hours', { count: Math.round(diff / hour) });
  }

  return new Intl.DateTimeFormat(resolveLocale(store.launcherSettings.language), {
    month: 'short',
    day: 'numeric'
  }).format(date);
}

function setTab(tab: HistoryTab) {
  store.setTab(tab);
  focusSearch();
}

function pasteItem(item: ClipboardItem) {
  void store.copyItem(item.id, true);
}

function copyItem(event: MouseEvent, item: ClipboardItem) {
  event.stopPropagation();
  void store.copyItem(item.id);
}

function toggleFavorite(event: MouseEvent, item: ClipboardItem) {
  event.stopPropagation();
  void store.toggleFavorite(item.id);
}

function deleteItem(event: MouseEvent, item: ClipboardItem) {
  event.stopPropagation();
  void store.deleteItem(item.id);
}

function onAutoStartChange(event: Event) {
  const target = event.target as HTMLInputElement;
  void store.setAutoStart(target.checked);
}

function startShortcutRecording() {
  recordingShortcut.value = true;
}

function onKeydown(event: KeyboardEvent) {
  if (recordingShortcut.value) {
    captureShortcut(event);
    return;
  }

  if (event.key === 'Escape') {
    event.preventDefault();
    if (isSettingsWindow) {
      void currentWindow.hide();
    } else {
      void store.hideWindow('esc');
    }
    return;
  }

  if ((event.ctrlKey || event.metaKey) && event.key === ',') {
    event.preventDefault();
    void openSettingsWindow();
    return;
  }

  if (event.ctrlKey && event.key === '1') {
    event.preventDefault();
    setTab('recent');
    return;
  }

  if (event.ctrlKey && event.key === '2') {
    event.preventDefault();
    setTab('favorites');
    return;
  }

  if (event.ctrlKey && event.key === '3') {
    event.preventDefault();
    setTab('frequent');
    return;
  }

  if (event.key === 'ArrowDown') {
    event.preventDefault();
    store.selectNext();
    return;
  }

  if (event.key === 'ArrowUp') {
    event.preventDefault();
    store.selectPrevious();
    return;
  }

  if (event.key === 'Enter' && store.selectedItem) {
    event.preventDefault();
    pasteItem(store.selectedItem);
    return;
  }

  if (event.key === 'Tab') {
    event.preventDefault();
    const tabs: HistoryTab[] = ['recent', 'favorites', 'frequent'];
    const index = tabs.indexOf(store.activeTab);
    setTab(tabs[(index + 1) % tabs.length]);
  }
}

function openSettingsWindow() {
  void invoke('open_settings_window');
}

function typeIcon(type: ClipboardItem['content_type']) {
  return type === 'url' ? Link : type === 'email' ? Mail : type === 'code' || type === 'json' ? Hash : Clipboard;
}

function itemIcon(item: ClipboardItem) {
  if (item.content_type !== 'file') {
    return typeIcon(item.content_type);
  }

  const paths = filePaths(item);
  if (paths.length > 1) {
    return Files;
  }
  if (fileFolderByItem.value[item.id]) {
    return Folder;
  }

  return File;
}

function contentTypeLabel(type: ClipboardItem['content_type']) {
  return t(`contentType.${type}`);
}

function localizedError(error: string) {
  if (error === 'Shortcut must include at least one modifier and one key') {
    return t('error.shortcutKeyRequired');
  }
  if (error === 'Settings window not found') {
    return t('error.settingsWindowNotFound');
  }
  if (error === 'Main window not found') {
    return t('error.mainWindowNotFound');
  }
  if (error === 'Database lock failed') {
    return t('error.databaseLockFailed');
  }

  const modifier = error.match(/^Unsupported shortcut modifier: (.+)$/);
  if (modifier) {
    return t('error.unsupportedShortcutModifier', { value: modifier[1] });
  }

  const key = error.match(/^Unsupported shortcut key: (.+)$/);
  if (key) {
    return t('error.unsupportedShortcutKey', { value: key[1] });
  }

  return error;
}

function captureShortcut(event: KeyboardEvent) {
  event.preventDefault();
  event.stopPropagation();

  if (event.key === 'Escape') {
    recordingShortcut.value = false;
    return;
  }

  const key = shortcutKeyName(event);
  if (!key) {
    return;
  }

  const parts = [];
  if (event.ctrlKey) parts.push('Ctrl');
  if (event.altKey) parts.push('Alt');
  if (event.shiftKey) parts.push('Shift');
  if (event.metaKey) parts.push('Win');

  if (parts.length === 0) {
    store.error = t('error.shortcutModifier');
    return;
  }

  parts.push(key);
  recordingShortcut.value = false;
  void store.setGlobalShortcut(parts.join('+'));
}

function shortcutKeyName(event: KeyboardEvent) {
  if (['Control', 'Alt', 'Shift', 'Meta'].includes(event.key)) {
    return '';
  }

  if (event.code === 'Backquote' || event.key === '`' || event.key === '~') {
    return 'Backquote';
  }

  if (event.code.startsWith('Key')) {
    return event.code.slice(3);
  }

  if (event.code.startsWith('Digit')) {
    return event.code.slice(5);
  }

  if (/^F\d{1,2}$/.test(event.key)) {
    return event.key;
  }

  const names: Record<string, string> = {
    ' ': 'Space',
    Spacebar: 'Space',
    Enter: 'Enter',
    Tab: 'Tab',
    Escape: 'Escape',
    Esc: 'Escape',
    ArrowUp: 'ArrowUp',
    ArrowDown: 'ArrowDown',
    ArrowLeft: 'ArrowLeft',
    ArrowRight: 'ArrowRight',
    Backspace: 'Backspace',
    Delete: 'Delete'
  };

  return names[event.key] ?? (event.key.length === 1 ? event.key.toUpperCase() : '');
}

function updateSetting(partial: Partial<typeof store.launcherSettings>) {
  void store.setLauncherSettings({ ...store.launcherSettings, ...partial });
}

function applyVisualSettings() {
  const setting = store.launcherSettings.theme;
  const resolved = setting === 'system'
    ? (window.matchMedia('(prefers-color-scheme: light)').matches ? 'light' : 'dark')
    : setting;

  document.documentElement.dataset.theme = resolved;
  document.documentElement.style.setProperty('--launcher-alpha', String(store.launcherSettings.transparency / 100));
  void currentWindow.setTitle(isSettingsWindow ? t('settings.title') : t('app.name'));
}
</script>

<template>
  <main v-if="!isSettingsWindow" class="launcher" data-tauri-drag-region @keydown="onKeydown">
    <label class="search-row">
      <Search :size="21" />
      <input
        ref="searchInput"
        v-model="search"
        type="search"
        :placeholder="t('launcher.searchPlaceholder')"
        autocomplete="off"
        autofocus
      />
      <button class="icon-button settings-button" type="button" :title="t('actions.settings')" @click="openSettingsWindow">
        <Settings :size="16" />
      </button>
    </label>

    <div v-if="store.error" class="error" role="alert">{{ localizedError(store.error) }}</div>

    <nav class="tabs" :aria-label="t('launcher.clipboardViews')">
      <button
        class="tab-button"
        :class="{ active: store.activeTab === 'recent' }"
        type="button"
        @click="setTab('recent')"
      >
        {{ t('tabs.recent') }}
      </button>
      <button
        class="tab-button"
        :class="{ active: store.activeTab === 'favorites' }"
        type="button"
        @click="setTab('favorites')"
      >
        {{ t('tabs.favorites') }}
      </button>
      <button
        class="tab-button"
        :class="{ active: store.activeTab === 'frequent' }"
        type="button"
        @click="setTab('frequent')"
      >
        {{ t('tabs.frequent') }}
      </button>
    </nav>

    <section class="result-list" aria-live="polite">
      <button
        v-for="item in store.filteredItems"
        :key="item.id"
        class="result-item"
        :class="{ active: item.id === store.selectedId }"
        type="button"
        @click="pasteItem(item)"
        @dblclick="pasteItem(item)"
      >
        <component :is="itemIcon(item)" class="type-icon" :size="18" />
        <span class="result-main">
          <span class="clip-text">{{ itemPreview(item) }}</span>
          <span class="clip-meta">
            {{ fileMeta(item) }} / {{ formatTime(item.last_copied_at || item.updated_at) }}
            <template v-if="item.paste_count > 0"> / {{ item.paste_count }} {{ t('launcher.pastes') }}</template>
          </span>
        </span>
        <span class="quick-actions">
          <button
            class="icon-button"
            type="button"
            :title="item.is_favorite ? t('actions.removeFavorite') : t('actions.favorite')"
            @click="toggleFavorite($event, item)"
          >
            <StarOff v-if="item.is_favorite" :size="15" />
            <Star v-else :size="15" />
          </button>
          <button class="icon-button" type="button" :title="t('actions.copy')" @click="copyItem($event, item)">
            <Copy :size="15" />
          </button>
          <button
            class="icon-button danger"
            type="button"
            :title="t('actions.delete')"
            @click="deleteItem($event, item)"
          >
            <Trash2 :size="15" />
          </button>
        </span>
      </button>

      <div v-if="!store.loading && store.filteredItems.length === 0" class="empty-state">
        <Clipboard :size="24" />
        <span>{{ t('launcher.empty') }}</span>
      </div>
    </section>
  </main>

  <main v-else class="settings-page" @keydown="onKeydown">
    <header class="settings-header">
      <h1>{{ t('settings.title') }}</h1>
    </header>

    <div v-if="store.error" class="error" role="alert">{{ localizedError(store.error) }}</div>

    <section class="settings-group" :aria-label="t('settings.appearance')">
      <h2>{{ t('settings.appearance') }}</h2>
      <label class="setting-row">
        <span>{{ t('settings.language') }}</span>
        <select
          :value="store.launcherSettings.language"
          :disabled="store.settingsLoading"
          @change="updateSetting({ language: ($event.target as HTMLSelectElement).value as any })"
        >
          <option value="system">{{ t('settings.languageFollowSystem') }}</option>
          <option value="zh-CN">{{ t('settings.languageChinese') }}</option>
          <option value="en-US">{{ t('settings.languageEnglish') }}</option>
        </select>
      </label>
      <label class="setting-row">
        <span>{{ t('settings.theme') }}</span>
        <select
          :value="store.launcherSettings.theme"
          :disabled="store.settingsLoading"
          @change="updateSetting({ theme: ($event.target as HTMLSelectElement).value as any })"
        >
          <option value="system">{{ t('settings.themeFollowSystem') }}</option>
          <option value="dark">{{ t('settings.themeDark') }}</option>
          <option value="light">{{ t('settings.themeLight') }}</option>
        </select>
      </label>
      <label class="setting-row">
        <span>{{ t('settings.transparency') }}</span>
        <select
          :value="store.launcherSettings.transparency"
          :disabled="store.settingsLoading"
          @change="updateSetting({ transparency: Number(($event.target as HTMLSelectElement).value) })"
        >
          <option value="70">70%</option>
          <option value="80">80%</option>
          <option value="88">{{ t('settings.transparencyRecommended') }}</option>
          <option value="90">90%</option>
          <option value="100">100%</option>
        </select>
      </label>
      <label class="setting-row">
        <span>{{ t('settings.popupPosition') }}</span>
        <select
          :value="store.launcherSettings.popupPosition"
          :disabled="store.settingsLoading"
          @change="updateSetting({ popupPosition: ($event.target as HTMLSelectElement).value as any })"
        >
          <option value="mouse">{{ t('settings.popupMouse') }}</option>
          <option value="center">{{ t('settings.popupCenter') }}</option>
        </select>
      </label>
    </section>

    <section class="settings-group" :aria-label="t('settings.shortcutSection')">
      <h2>{{ t('settings.shortcutSection') }}</h2>
      <label class="setting-row">
        <span><Keyboard :size="15" /> {{ t('settings.shortcut') }}</span>
        <button class="setting-button" type="button" :disabled="store.shortcutLoading" @click="startShortcutRecording">
          {{ recordingShortcut ? t('settings.pressShortcut') : store.globalShortcut }}
        </button>
      </label>
    </section>

    <section class="settings-group" :aria-label="t('settings.history')">
      <h2>{{ t('settings.history') }}</h2>
      <label class="setting-row">
        <span>{{ t('settings.historyRetention') }}</span>
        <select
          :value="store.launcherSettings.historyRetention"
          :disabled="store.settingsLoading"
          @change="updateSetting({ historyRetention: Number(($event.target as HTMLSelectElement).value) as any })"
        >
          <option value="100">{{ t('settings.retentionItems', { count: 100 }) }}</option>
          <option value="500">{{ t('settings.retentionItems', { count: 500 }) }}</option>
          <option value="1000">{{ t('settings.retentionItems', { count: '1,000' }) }}</option>
          <option value="5000">{{ t('settings.retentionItems', { count: '5,000' }) }}</option>
          <option value="10000">{{ t('settings.retentionItems', { count: '10,000' }) }}</option>
        </select>
      </label>
    </section>

    <section class="settings-group" :aria-label="t('settings.startup')">
      <h2>{{ t('settings.startup') }}</h2>
      <label class="setting-row">
        <span>{{ t('settings.startWithWindows') }}</span>
        <input
          type="checkbox"
          :checked="store.autoStartEnabled"
          :disabled="store.autoStartLoading"
          @change="onAutoStartChange"
        />
      </label>
    </section>
  </main>
</template>
