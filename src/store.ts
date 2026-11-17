import { create } from 'zustand';
import { applyLanguage } from './i18n';
import { call, subscribe } from './bridge';
import type {
  AppError,
  AuthConnection,
  Bootstrap,
  CoreEvent,
  DownloadOptions,
  Job,
  Page,
  Settings,
  Source,
  ToolInfo,
} from './types';
export const defaultOptions: DownloadOptions = {
  maxHeight: 1080,
  format: 'mp4',
  audioFormat: 'mp3',
  compatibility: true,
  subtitles: false,
  thumbnail: false,
  skipExisting: true,
};
const defaults: Settings = {
  language: localStorage.getItem('yt-language') || 'system',
  theme: 'system',
  destination: '',
  concurrency: 2,
  retries: 3,
  proxyMode: 'system',
  proxyUrl: '',
  closeToTray: true,
  autoUpdate: true,
  options: defaultOptions,
};
interface Store {
  page: Page;
  settings: Settings;
  options: DownloadOptions;
  destination: string;
  auth: AuthConnection;
  jobs: Job[];
  sources: Source[];
  sourceId: string | null;
  tools: ToolInfo[];
  version: string;
  error: AppError | null;
  notice: string | null;
  selected: Set<string>;
  initialized: boolean;
  setPage: (page: Page) => void;
  setSource: (id: string) => void;
  select: (id: string, value: boolean) => void;
  selectMany: (ids: string[], value: boolean) => void;
  setOptions: (p: Partial<DownloadOptions>) => void;
  setDestination: (p: string) => void;
  setError: (e: unknown) => void;
  setNotice: (s: string | null) => void;
  event: (e: CoreEvent) => void;
  saveSettings: (s: Settings) => Promise<void>;
}
function upsert<T extends { id: string }>(items: T[], value: T) {
  const index = items.findIndex((i) => i.id === value.id);
  if (index < 0) return [value, ...items];
  const copy = items.slice();
  copy[index] = value;
  return copy;
}
export const useStore = create<Store>((set, get) => ({
  page: 'new',
  settings: defaults,
  options: defaultOptions,
  destination: '',
  auth: {
    method: 'none',
    browser: 'chrome',
    profile: '',
    status: 'disconnected',
    remember: false,
    verifiedAt: null,
    error: null,
  },
  jobs: [],
  sources: [],
  sourceId: null,
  tools: [],
  version: '0.1.0',
  error: null,
  notice: null,
  selected: new Set(),
  initialized: false,
  setPage: (page) => set({ page }),
  setSource: (id) => set({ sourceId: id, selected: new Set(), page: 'new' }),
  select: (id, value) =>
    set((s) => {
      const selected = new Set(s.selected);
      if (value) selected.add(id);
      else selected.delete(id);
      return { selected };
    }),
  selectMany: (ids, value) =>
    set((s) => {
      const selected = new Set(s.selected);
      for (const id of ids) {
        if (value) selected.add(id);
        else selected.delete(id);
      }
      return { selected };
    }),
  setOptions: (p) => set((s) => ({ options: { ...s.options, ...p } })),
  setDestination: (destination) => set({ destination }),
  setError: (error) =>
    set({
      error:
        error === null
          ? null
          : typeof error === 'object' && error !== null && 'code' in error
            ? (error as AppError)
            : { code: 'unknown', detail: String(error) },
    }),
  setNotice: (notice) => set({ notice }),
  event: (event) => {
    switch (event.type) {
      case 'job':
        set((s) => ({ jobs: upsert(s.jobs, event.data) }));
        break;
      case 'source':
        set((s) => ({ sources: upsert(s.sources, event.data) }));
        break;
      case 'auth':
        set({ auth: event.data });
        break;
      case 'tools':
        set({ tools: event.data });
        break;
      case 'notice':
        get().setError(event.data);
    }
  },
  saveSettings: async (settings) => {
    const saved = await call<Settings>('save_settings', { settings });
    applyLanguage(saved.language);
    set({ settings: saved, options: { ...saved.options }, destination: saved.destination });
  },
}));
let initialization: Promise<void> | null = null;
export function initialize() {
  if (initialization) return initialization;
  initialization = (async () => {
    const unlisten = await subscribe(useStore.getState().event);
    try {
      const data = await call<Bootstrap>('bootstrap');
      applyLanguage(data.settings.language);
      useStore.setState({
        ...data,
        options: { ...data.settings.options },
        destination: data.settings.destination,
        sourceId: data.sources[0]?.id || null,
        initialized: true,
      });
    } catch (error) {
      useStore.getState().setError(error);
      useStore.setState({ initialized: true });
    }
    return unlisten;
  })().then(() => {});
  return initialization;
}
export const itemKey = (item: { site: string; id: string }) => `${item.site}:${item.id}`;
export const activeStatuses = new Set([
  'queued',
  'downloading',
  'processing',
  'verifying',
  'retry_wait',
  'waiting_auth',
]);
