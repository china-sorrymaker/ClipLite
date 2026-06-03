import { defineStore } from 'pinia';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { ClipboardItem, LauncherSettings } from '../types';

export type HistoryTab = 'all' | 'favorites';

type HistoryUpdatedPayload = {
  item_id?: number;
};

export const useClipboardStore = defineStore('clipboard', {
  state: () => ({
    items: [] as ClipboardItem[],
    searchQuery: '',
    activeTab: 'all' as HistoryTab,
    loading: false,
    initialized: false,
    selectedId: null as number | null,
    autoStartEnabled: false,
    autoStartLoading: false,
    globalShortcut: 'Ctrl+Alt+V',
    shortcutLoading: false,
    launcherSettings: {
      theme: 'system',
      popupPosition: 'mouse',
      pasteStrategy: 'auto',
      pasteDelayMs: 200,
      width: 620,
      height: 460,
      transparency: 88
    } as LauncherSettings,
    settingsLoading: false,
    error: ''
  }),
  getters: {
    filteredItems(state) {
      if (state.activeTab === 'all') {
        return state.items;
      }

      return state.items.filter((item) => item.is_favorite);
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
      this.loading = true;
      this.error = '';

      try {
        const query = this.searchQuery.trim();
        this.items = query
          ? await invoke<ClipboardItem[]>('search_items', { query })
          : await invoke<ClipboardItem[]>('list_items');

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
      }
    },
    setTab(tab: HistoryTab) {
      this.activeTab = tab;
      this.selectedId = this.filteredItems[0]?.id ?? null;
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
      try {
        await invoke('toggle_favorite', { id });
        await this.refresh();
      } catch (error) {
        this.error = String(error);
      }
    },
    async deleteItem(id: number) {
      this.error = '';
      try {
        await invoke('delete_item', { id });
        await this.refresh();
      } catch (error) {
        this.error = String(error);
      }
    },
    async hideWindow() {
      await invoke('hide_window');
    },
    async refreshAutoStart() {
      this.autoStartLoading = true;
      this.error = '';

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
