import { create } from 'zustand';
import { listen } from '@tauri-apps/api/event';
import { call, native } from './bridge';
import type { AppError } from './types';
export interface UpdateInfo {
  version: string;
  notes: string | null;
  date: string | null;
}
interface UpdateState {
  status: 'idle' | 'checking' | 'available' | 'current' | 'downloading' | 'installing' | 'error';
  info: UpdateInfo | null;
  downloaded: number;
  total: number | null;
  error: AppError | null;
  visible: boolean;
  show: () => void;
  hide: () => void;
  check: () => Promise<void>;
  install: () => Promise<void>;
}
export const useUpdates = create<UpdateState>((set, get) => ({
  status: 'idle',
  info: null,
  downloaded: 0,
  total: null,
  error: null,
  visible: false,
  show: () => set({ visible: true }),
  hide: () => set({ visible: false }),
  check: async () => {
    if (['checking', 'downloading', 'installing'].includes(get().status)) return;
    set({ status: 'checking', error: null });
    try {
      const info = await call<UpdateInfo | null>('check_update');
      set({ info, status: info ? 'available' : 'current', visible: !!info });
    } catch (error) {
      set({ status: 'error', error: asError(error) });
    }
  },
  install: async () => {
    if (get().status !== 'available' && get().status !== 'error') return;
    set({ status: 'downloading', downloaded: 0, total: null, error: null });
    let unlisten = () => {};
    try {
      unlisten = await listen<{
        phase: 'downloading' | 'installing';
        downloaded: number;
        total: number | null;
      }>('update-progress', (event) =>
        set({
          status: event.payload.phase,
          downloaded: event.payload.downloaded,
          total: event.payload.total,
        }),
      );
      await call('install_update');
    } catch (error) {
      set({ status: 'error', error: asError(error) });
    } finally {
      unlisten();
    }
  },
}));
function asError(error: unknown): AppError {
  return typeof error === 'object' && error !== null && 'code' in error
    ? (error as AppError)
    : { code: 'update_failed', detail: String(error) };
}
let startupChecked = false;
export function startupUpdateCheck(enabled: boolean) {
  if (!native || !enabled || startupChecked) return;
  startupChecked = true;
  void useUpdates.getState().check();
}
