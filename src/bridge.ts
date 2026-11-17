import { invoke, isTauri } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { open, save } from '@tauri-apps/plugin-dialog';
import type { CoreEvent } from './types';
export const native = isTauri();
export async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (!native) throw { code: 'native_required', detail: '' };
  return invoke<T>(command, args);
}
export async function subscribe(handler: (event: CoreEvent) => void) {
  return native ? listen<CoreEvent>('core-event', (e) => handler(e.payload)) : () => {};
}
export async function chooseDirectory() {
  return native ? await open({ directory: true, multiple: false }) : null;
}
export async function chooseCookie() {
  return native
    ? await open({ multiple: false, filters: [{ name: 'Cookies', extensions: ['txt'] }] })
    : null;
}
export async function chooseCsv() {
  return native
    ? await save({ defaultPath: 'YT-Desktop.csv', filters: [{ name: 'CSV', extensions: ['csv'] }] })
    : null;
}
