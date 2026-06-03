<script setup lang="ts">
import { nextTick, onMounted, ref, watch } from 'vue';
import { listen } from '@tauri-apps/api/event';
import {
  Clipboard,
  Copy,
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
import type { ClipboardItem } from './types';

const store = useClipboardStore();
const search = ref('');
const searchInput = ref<HTMLInputElement | null>(null);
const settingsOpen = ref(false);
const recordingShortcut = ref(false);

let searchTimer: number | undefined;
let systemThemeQuery: MediaQueryList | undefined;

watch(search, (value) => {
  window.clearTimeout(searchTimer);
  searchTimer = window.setTimeout(() => {
    void store.setSearch(value);
  }, 90);
});

watch(() => store.launcherSettings, applyVisualSettings, { deep: true });

onMounted(async () => {
  await store.initialize();
  applyVisualSettings();
  systemThemeQuery = window.matchMedia('(prefers-color-scheme: light)');
  systemThemeQuery.addEventListener('change', applyVisualSettings);
  focusSearch();

  await listen('window-shown', () => {
    settingsOpen.value = false;
    void nextTick(focusSearch);
  });
});

function focusSearch() {
  searchInput.value?.focus();
  searchInput.value?.select();
}

function preview(text: string) {
  return text.replace(/\s+/g, ' ').trim();
}

function formatTime(timestamp: number) {
  const date = new Date(timestamp * 1000);
  const diff = Date.now() - date.getTime();
  const minute = 60 * 1000;
  const hour = 60 * minute;
  const day = 24 * hour;

  if (diff < hour) {
    return `${Math.max(1, Math.round(diff / minute))}m`;
  }

  if (diff < day) {
    return `${Math.round(diff / hour)}h`;
  }

  return new Intl.DateTimeFormat(undefined, { month: 'short', day: 'numeric' }).format(date);
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
    if (settingsOpen.value) {
      settingsOpen.value = false;
      focusSearch();
    } else {
      void store.hideWindow();
    }
    return;
  }

  if ((event.ctrlKey || event.metaKey) && event.key === ',') {
    event.preventDefault();
    settingsOpen.value = !settingsOpen.value;
    return;
  }

  if (event.ctrlKey && event.key === '1') {
    event.preventDefault();
    setTab('all');
    return;
  }

  if (event.ctrlKey && event.key === '2') {
    event.preventDefault();
    setTab('favorites');
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
    setTab(store.activeTab === 'all' ? 'favorites' : 'all');
  }
}

function typeIcon(type: ClipboardItem['content_type']) {
  return type === 'url' ? Link : type === 'email' ? Mail : type === 'code' || type === 'json' ? Hash : Clipboard;
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
    store.error = 'Shortcut must include at least one modifier.';
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
    Enter: 'Enter',
    Tab: 'Tab'
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
}
</script>

<template>
  <main class="launcher" data-tauri-drag-region @keydown="onKeydown">
    <label class="search-row">
      <Search :size="21" />
      <input
        ref="searchInput"
        v-model="search"
        type="search"
        placeholder="Search clipboard history"
        autocomplete="off"
        autofocus
      />
      <button class="icon-button settings-button" type="button" title="Settings" @click="settingsOpen = !settingsOpen">
        <Settings :size="16" />
      </button>
    </label>

    <div v-if="store.error" class="error" role="alert">{{ store.error }}</div>

    <section v-if="settingsOpen" class="settings-panel" aria-label="Settings">
      <label class="setting-row">
        <span>Start with Windows</span>
        <input
          type="checkbox"
          :checked="store.autoStartEnabled"
          :disabled="store.autoStartLoading"
          @change="onAutoStartChange"
        />
      </label>
      <label class="setting-row">
        <span><Keyboard :size="15" /> Global shortcut</span>
        <button class="setting-button" type="button" :disabled="store.shortcutLoading" @click="startShortcutRecording">
          {{ recordingShortcut ? 'Press shortcut...' : store.globalShortcut }}
        </button>
      </label>
      <label class="setting-row">
        <span>Theme</span>
        <select
          :value="store.launcherSettings.theme"
          :disabled="store.settingsLoading"
          @change="updateSetting({ theme: ($event.target as HTMLSelectElement).value as any })"
        >
          <option value="system">Follow System</option>
          <option value="dark">Dark</option>
          <option value="light">Light</option>
        </select>
      </label>
      <label class="setting-row">
        <span>Popup position</span>
        <select
          :value="store.launcherSettings.popupPosition"
          :disabled="store.settingsLoading"
          @change="updateSetting({ popupPosition: ($event.target as HTMLSelectElement).value as any })"
        >
          <option value="mouse">Mouse Cursor</option>
          <option value="center">Screen Center</option>
        </select>
      </label>
      <label class="setting-row">
        <span>Paste Strategy</span>
        <select
          :value="store.launcherSettings.pasteStrategy"
          :disabled="store.settingsLoading"
          @change="updateSetting({ pasteStrategy: ($event.target as HTMLSelectElement).value as any })"
        >
          <option value="auto">Auto</option>
          <option value="browser">Browser Optimized</option>
          <option value="native">Native Standard</option>
        </select>
      </label>
      <label class="setting-row">
        <span>Paste Delay Mode</span>
        <select
          :value="store.launcherSettings.pasteDelayMs"
          :disabled="store.settingsLoading"
          @change="updateSetting({ pasteDelayMs: Number(($event.target as HTMLSelectElement).value) as any })"
        >
          <option value="100">Fast (100ms)</option>
          <option value="200">Balanced (200ms)</option>
          <option value="300">Compatible (300ms)</option>
          <option value="500">Legacy (500ms)</option>
        </select>
      </label>
      <label class="setting-row">
        <span>Width</span>
        <input
          type="number"
          min="420"
          max="900"
          step="10"
          :value="store.launcherSettings.width"
          :disabled="store.settingsLoading"
          @change="updateSetting({ width: Number(($event.target as HTMLInputElement).value) })"
        />
      </label>
      <label class="setting-row">
        <span>Height</span>
        <input
          type="number"
          min="320"
          max="720"
          step="10"
          :value="store.launcherSettings.height"
          :disabled="store.settingsLoading"
          @change="updateSetting({ height: Number(($event.target as HTMLInputElement).value) })"
        />
      </label>
      <label class="setting-row">
        <span>Transparency</span>
        <input
          type="range"
          min="70"
          max="100"
          step="1"
          :value="store.launcherSettings.transparency"
          :disabled="store.settingsLoading"
          @input="updateSetting({ transparency: Number(($event.target as HTMLInputElement).value) })"
        />
      </label>
    </section>

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
        <component :is="typeIcon(item.content_type)" class="type-icon" :size="18" />
        <span class="result-main">
          <span class="clip-text">{{ preview(item.text) }}</span>
          <span class="clip-meta">
            {{ item.content_type }} / {{ formatTime(item.updated_at) }}
            <template v-if="item.usage_count > 0"> / {{ item.usage_count }} uses</template>
          </span>
        </span>
        <span class="quick-actions">
          <button
            class="icon-button"
            type="button"
            :title="item.is_favorite ? 'Remove favorite' : 'Favorite'"
            @click="toggleFavorite($event, item)"
          >
            <StarOff v-if="item.is_favorite" :size="15" />
            <Star v-else :size="15" />
          </button>
          <button class="icon-button" type="button" title="Copy" @click="copyItem($event, item)">
            <Copy :size="15" />
          </button>
          <button
            class="icon-button danger"
            type="button"
            title="Delete"
            @click="deleteItem($event, item)"
          >
            <Trash2 :size="15" />
          </button>
        </span>
      </button>

      <div v-if="!store.loading && store.filteredItems.length === 0" class="empty-state">
        <Clipboard :size="24" />
        <span>No clips found</span>
      </div>
    </section>
  </main>
</template>
