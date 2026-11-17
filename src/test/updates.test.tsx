import { beforeEach, it, expect, vi } from 'vitest';
import { cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '../i18n';
import i18n from '../i18n';
import { useUpdates } from '../updates';
import { UpdateDialog } from '../components/UpdateDialog';
import { call } from '../bridge';
vi.mock('../bridge', () => ({ native: true, call: vi.fn() }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));
beforeEach(async () => {
  cleanup();
  vi.clearAllMocks();
  await i18n.changeLanguage('zh-CN');
  useUpdates.setState({
    status: 'idle',
    info: null,
    error: null,
    visible: false,
    downloaded: 0,
    total: null,
  });
});
it('checks a real backend result and displays release notes as text', async () => {
  vi.mocked(call).mockResolvedValue({
    version: '0.2.0',
    notes: '<script>alert(1)</script>',
    date: null,
  });
  await useUpdates.getState().check();
  render(<UpdateDialog />);
  expect(screen.getByRole('dialog')).toBeInTheDocument();
  expect(screen.getByText('<script>alert(1)</script>')).toBeInTheDocument();
  expect(document.querySelector('script')).toBeNull();
  expect(call).toHaveBeenCalledWith('check_update');
});
it('does not install during checking and keeps an install error visible', async () => {
  useUpdates.setState({ status: 'checking' });
  await useUpdates.getState().install();
  expect(call).not.toHaveBeenCalled();
  useUpdates.setState({
    status: 'available',
    visible: true,
    info: { version: '0.2.0', notes: null, date: null },
  });
  vi.mocked(call).mockRejectedValue({
    code: 'update_failed',
    detail: 'Signature verification failed',
  });
  render(<UpdateDialog />);
  await userEvent.click(screen.getByRole('button', { name: '下载更新并重启' }));
  expect(await screen.findByText('Signature verification failed')).toBeInTheDocument();
  expect(useUpdates.getState().status).toBe('error');
  expect(useUpdates.getState().downloaded).toBe(0);
});
it('reports no update without inventing a version', async () => {
  vi.mocked(call).mockResolvedValue(null);
  await useUpdates.getState().check();
  expect(useUpdates.getState().status).toBe('current');
  expect(useUpdates.getState().visible).toBe(false);
});
