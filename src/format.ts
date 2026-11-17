export function duration(seconds: number | null | undefined) {
  if (seconds == null || !Number.isFinite(seconds)) return '—';
  const s = Math.max(0, Math.round(seconds));
  return s >= 3600
    ? `${Math.floor(s / 3600)}:${String(Math.floor(s / 60) % 60).padStart(2, '0')}:${String(s % 60).padStart(2, '0')}`
    : `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`;
}
export function bytes(n: number | null | undefined) {
  if (n == null || !Number.isFinite(n)) return '—';
  if (n < 1024) return `${n} B`;
  const units = ['KB', 'MB', 'GB', 'TB'];
  let x = n / 1024,
    i = 0;
  while (x >= 1024 && i < 3) {
    x /= 1024;
    i++;
  }
  return `${x.toFixed(x < 10 ? 1 : 0)} ${units[i]}`;
}
export function timestamp(n: number) {
  return new Date(n * 1000).toLocaleString();
}
