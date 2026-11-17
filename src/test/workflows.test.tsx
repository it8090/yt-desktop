import { beforeEach, describe, it, expect, vi } from 'vitest';
import { cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '../i18n';
import i18n from '../i18n';
import NewDownload from '../pages/NewDownload';
import Auth from '../pages/Auth';
import Downloads from '../pages/Downloads';
import { defaultOptions, itemKey, useStore } from '../store';
import { call } from '../bridge';
import type { MediaItem } from '../types';
vi.mock('../bridge', () => ({
  native: true,
  call: vi.fn(),
  chooseCookie: vi.fn(),
  chooseDirectory: vi.fn(),
  chooseCsv: vi.fn(),
  subscribe: vi.fn(),
}));
// Virtualization needs a viewport. The production virtualizer remains in use.
vi.mock('@tanstack/react-virtual', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@tanstack/react-virtual')>();
  return {
    ...actual,
    useVirtualizer: (options: Parameters<typeof actual.useVirtualizer>[0]) =>
      actual.useVirtualizer({
        ...options,
        initialRect: { width: 800, height: 500 },
        observeElementRect: (_, callback) => {
          callback({ width: 800, height: 500 });
          return () => {};
        },
        observeElementOffset: (_, callback) => {
          callback(0, false);
          return () => {};
        },
      }),
  };
});
const entries: MediaItem[] = Array.from({ length: 100 }, (_, i) => ({
  id: String(i),
  title: `Video ${i}`,
  url: `https://example.com/${i}.mp4`,
  thumbnail: null,
  duration: 30,
  uploader: 'Test creator',
  site: 'example.com',
}));
beforeEach(async () => {
  cleanup();
  vi.clearAllMocks();
  await i18n.changeLanguage('zh-CN');
  useStore.setState({
    selected: new Set(),
    sources: [
      {
        id: 'source',
        urls: ['https://example.com/list'],
        title: 'Test source',
        status: 'ready',
        entries,
        total: 100,
        error: null,
        updatedAt: 1,
      },
    ],
    sourceId: 'source',
    jobs: [],
    options: { ...defaultOptions },
    destination: '/tmp/output',
    auth: {
      method: 'none',
      browser: 'chrome',
      profile: '',
      status: 'disconnected',
      remember: false,
      verifiedAt: null,
      error: null,
    },
  });
  vi.mocked(call).mockResolvedValue({ jobs: [], skipped: 0, batchId: 'batch' });
});
describe('download workflow', () => {
  it('single download only enqueues its row, even with other selections', async () => {
    useStore.getState().select(itemKey(entries[1]), true);
    render(<NewDownload />);
    await userEvent.click(screen.getAllByRole('button', { name: /^下载$/ })[0]);
    expect(call).toHaveBeenCalledWith(
      'enqueue',
      expect.objectContaining({ request: expect.objectContaining({ items: [entries[0]] }) }),
    );
  });
  it('select all includes offscreen results and filtered bulk download uses the filter', async () => {
    render(<NewDownload />);
    await userEvent.click(screen.getByLabelText('全选当前结果'));
    expect(useStore.getState().selected.size).toBe(100);
    await userEvent.type(screen.getByLabelText('搜索视频标题'), 'Video 99');
    await userEvent.click(screen.getByRole('button', { name: '下载当前结果' }));
    expect(call).toHaveBeenCalledWith(
      'enqueue',
      expect.objectContaining({ request: expect.objectContaining({ items: [entries[99]] }) }),
    );
  });
  it('never shows a made-up percentage when total size is unknown', () => {
    useStore.setState({
      jobs: [
        {
          id: 'job',
          batchId: 'batch',
          item: entries[0],
          options: defaultOptions,
          destination: '/tmp/output',
          status: 'downloading',
          progress: null,
          bytesDownloaded: 1024,
          bytesTotal: null,
          speed: null,
          eta: null,
          attempt: 1,
          retryAt: null,
          error: null,
          filePath: null,
          fileSize: null,
          duration: null,
          resolution: null,
          createdAt: 1,
          updatedAt: 1,
          fallbackUsed: false,
        },
      ],
    });
    render(<Downloads />);
    expect(screen.getByRole('progressbar')).not.toHaveAttribute('aria-valuenow');
    expect(screen.getByText(/大小待获取/)).toBeInTheDocument();
  });
});
it('requires explicit cookie access consent before connection verification', async () => {
  render(<Auth />);
  const button = screen.getByRole('button', { name: '验证并连接' });
  expect(button).toBeDisabled();
  await userEvent.click(screen.getByLabelText(/我同意读取/));
  expect(button).toBeEnabled();
});
it('Chinese and English translations have matching keys', () => {
  function keys(o: object, p = ''): string[] {
    return Object.entries(o).flatMap(([k, v]) =>
      typeof v === 'object' ? keys(v, `${p}${k}.`) : [`${p}${k}`],
    );
  }
  expect(keys(i18n.getResourceBundle('zh-CN', 'translation')).sort()).toEqual(
    keys(i18n.getResourceBundle('en', 'translation')).sort(),
  );
});
