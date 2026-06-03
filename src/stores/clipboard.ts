import { defineStore } from 'pinia';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { ClipboardItem, LauncherSettings } from '../types';

const DEBUG_CLIPBOARD_TIMING = false;

export type HistoryTab = 'recent' | 'favorites' | 'frequent';

type HistoryUpdatedPayload = {
  item_id?: number;
};

type CleanHistoryResult = {
  cleanedCount: number;
  mode: 'time' | 'count' | 'never';
};

type DeleteItemResult = {
  id: number;
  existedBefore: boolean;
  affectedRows: number;
  existsAfter: boolean;
};

export const useClipboardStore = defineStore('clipboard', {
  state: () => ({
    items: [] as ClipboardItem[],
    searchQuery: '',
    activeTab: 'recent' as HistoryTab,
    loading: false,
    initialized: false,
    selectedId: null as number | null,
    autoStartEnabled: false,
    autoStartLoading: false,
    globalShortcut: 'Ctrl+Alt+V',
    shortcutLoading: false,
    launcherSettings: {
      language: 'system',
      theme: 'system',
      popupPosition: 'mouse',
      pasteStrategy: 'auto',
      pasteDelayMs: 200,
      width: 620,
      height: 460,
      transparency: 88,
      historyCleanupMode: 'count',
      historyRetentionDays: 30,
      historyRetention: 1000
    } as LauncherSettings,
    settingsLoading: false,
    message: '',
    error: ''
  }),
  getters: {
    filteredItems(state) {
      return state.items;
    },
    selectedItem(state) {
      return state.items.find((item) => item.id === state.selectedId) ?? null;
    }
  },
  actions: {
    async initialize() {
      if (this.initialized) {
        return;
      }

      this.initialized = true;
      await this.refresh();
      await this.refreshAutoStart();
      await this.refreshGlobalShortcut();
      await this.refreshLauncherSettings();

      await listen<HistoryUpdatedPayload>('history-updated', async () => {
        await this.refresh();
      });
    },
    async refresh() {
      const refreshStart = performance.now();
      this.loading = true;

      try {
        const query = this.searchQuery.trim();
        this.items = query
          ? await invoke<ClipboardItem[]>('search_items', { query, tab: this.activeTab })
          : await invoke<ClipboardItem[]>('list_items', { tab: this.activeTab });

        if (this.filteredItems.length > 0 && !this.selectedId) {
          this.selectedId = this.filteredItems[0].id;
        }

        if (this.selectedId && !this.filteredItems.some((item) => item.id === this.selectedId)) {
          this.selectedId = this.filteredItems[0]?.id ?? null;
        }
      } catch (error) {
        this.error = String(error);
      } finally {
        this.loading = false;
        if (DEBUG_CLIPBOARD_TIMING) {
          console.log('[clipboard] UI refresh time', `${(performance.now() - refreshStart).toFixed(1)}ms`);
        }
      }
    },
    async setTab(tab: HistoryTab) {
      this.activeTab = tab;
      this.selectedId = null;
      this.items = [];
      await this.refresh();
    },
    async setSearch(query: string) {
      this.searchQuery = query;
      await this.refresh();
    },
    selectItem(id: number) {
      this.selectedId = id;
    },
    selectNext() {
      const index = this.filteredItems.findIndex((item) => item.id === this.selectedId);
      const next = this.filteredItems[Math.min(index + 1, this.filteredItems.length - 1)];
      if (next) {
        this.selectedId = next.id;
      }
    },
    selectPrevious() {
      const index = this.filteredItems.findIndex((item) => item.id === this.selectedId);
      const previous = this.filteredItems[Math.max(index - 1, 0)];
      if (previous) {
        this.selectedId = previous.id;
      }
    },
    async copyItem(id: number, hideAfterCopy = false) {
      this.error = '';
      this.message = '';
      try {
        await invoke(hideAfterCopy ? 'paste_item' : 'copy_item', { id });
        this.selectedId = id;
        await this.refresh();
      } catch (error) {
        this.error = String(error);
      }
    },
    async toggleFavorite(id: number) {
      this.error = '';
      this.message = '';
      try {
        await invoke('toggle_favorite', { id });
        await this.refresh();
      } catch (error) {
        this.error = String(error);
      }
    },
    async deleteItem(id: number) {
      this.error = '';
      this.message = '';
      try {
        const commandName = 'delete_item';
        console.log('[delete_item] invoke command', commandName);
        console.log('[delete_item] item id', id);
        const response = await invoke<DeleteItemResult>(commandName, { id });
        console.log('[delete_item] invoke response', response);
        this.items = this.items.filter((item) => item.id !== id);
        if (this.selectedId === id) {
          this.selectedId = this.items[0]?.id ?? null;
        }
        await this.refresh();
      } catch (error) {
        this.error = String(error);
      }
    },
    async cleanHistoryNow() {
      this.settingsLoading = true;
      this.error = '';
      this.message = '';

      try {
        const result = await invoke<CleanHistoryResult>('clean_history_now', {
          settings: this.launcherSettings
        });
        await this.refresh();
        if (result.mode === 'never') {
          this.message = 'Auto cleanup is disabled';
        } else if (result.cleanedCount > 0) {
          this.message = `Cleaned ${result.cleanedCount} records`;
        } else {
          this.message = 'No records to clean';
        }
      } catch (error) {
        this.error = String(error);
      } finally {
        this.settingsLoading = false;
      }
    },
    async hideWindow(reason = 'command') {
      await invoke('hide_window', { reason });
    },
    async refreshAutoStart() {
      this.autoStartLoading = true;
      this.error = '';
      this.message = '';

      try {
        this.autoStartEnabled = await invoke<boolean>('get_autostart_enabled');
      } catch (error) {
        this.error = String(error);
      } finally {
        this.autoStartLoading = false;
      }
    },
    async setAutoStart(enabled: boolean) {
      this.autoStartLoading = true;
      this.error = '';
      this.message = '';

      try {
        this.autoStartEnabled = await invoke<boolean>('set_autostart_enabled', { enabled });
      } catch (error) {
        this.error = String(error);
      } finally {
        this.autoStartLoading = false;
      }
    },
    async refreshGlobalShortcut() {
      this.shortcutLoading = true;
      this.error = '';
      this.message = '';

      try {
        this.globalShortcut = await invoke<string>('get_global_shortcut');
      } catch (error) {
        this.error = String(error);
      } finally {
        this.shortcutLoading = false;
      }
    },
    async setGlobalShortcut(shortcut: string) {
      this.shortcutLoading = true;
      this.error = '';
      this.message = '';

      try {
        this.globalShortcut = await invoke<string>('set_global_shortcut', { shortcut });
      } catch (error) {
        this.error = String(error);
      } finally {
        this.shortcutLoading = false;
      }
    },
    async refreshLauncherSettings() {
      this.settingsLoading = true;
      this.error = '';

      try {
        this.launcherSettings = await invoke<LauncherSettings>('get_launcher_settings');
      } catch (error) {
        this.error = String(error);
      } finally {
        this.settingsLoading = false;
      }
    },
    async setLauncherSettings(settings: LauncherSettings) {
      this.settingsLoading = true;
      this.error = '';
      this.message = '';

      try {
        this.launcherSettings = await invoke<LauncherSettings>('set_launcher_settings', { settings });
      } catch (error) {
        this.error = String(error);
      } finally {
        this.settingsLoading = false;
      }
    }
  }
});
