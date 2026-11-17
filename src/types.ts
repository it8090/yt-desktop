export interface AppError {
  code: string;
  detail: string;
}
export interface DownloadOptions {
  maxHeight: number;
  format: string;
  audioFormat: string;
  compatibility: boolean;
  subtitles: boolean;
  thumbnail: boolean;
  skipExisting: boolean;
}
export interface Settings {
  language: string;
  theme: string;
  destination: string;
  concurrency: number;
  retries: number;
  proxyMode: string;
  proxyUrl: string;
  closeToTray: boolean;
  autoUpdate: boolean;
  options: DownloadOptions;
}
export interface MediaItem {
  id: string;
  title: string;
  url: string;
  thumbnail: string | null;
  duration: number | null;
  uploader: string | null;
  site: string;
}
export interface Source {
  id: string;
  urls: string[];
  title: string;
  status: string;
  entries: MediaItem[];
  total: number | null;
  error: AppError | null;
  updatedAt: number;
}
export interface AuthConnection {
  method: string;
  browser: string;
  profile: string;
  status: string;
  remember: boolean;
  verifiedAt: number | null;
  error: AppError | null;
}
export interface Job {
  id: string;
  batchId: string;
  item: MediaItem;
  options: DownloadOptions;
  destination: string;
  status: string;
  progress: number | null;
  bytesDownloaded: number;
  bytesTotal: number | null;
  speed: number | null;
  eta: number | null;
  attempt: number;
  retryAt: number | null;
  error: AppError | null;
  filePath: string | null;
  fileSize: number | null;
  duration: number | null;
  resolution: string | null;
  createdAt: number;
  updatedAt: number;
  fallbackUsed: boolean;
}
export interface ToolInfo {
  name: string;
  path: string | null;
  version: string | null;
  managed: boolean;
}
export interface Bootstrap {
  settings: Settings;
  auth: AuthConnection;
  jobs: Job[];
  sources: Source[];
  tools: ToolInfo[];
  version: string;
}
export interface AuthRequest {
  method: string;
  browser: string;
  profile: string;
  cookiePath: string | null;
  remember: boolean;
  consent: boolean;
  testUrl: string | null;
}
export interface EnqueueResult {
  jobs: Job[];
  skipped: number;
  batchId: string;
}
export type CoreEvent =
  | { type: 'job'; data: Job }
  | { type: 'source'; data: Source }
  | { type: 'auth'; data: AuthConnection }
  | { type: 'tools'; data: ToolInfo[] }
  | { type: 'notice'; data: AppError };
export type Page = 'new' | 'queue' | 'completed' | 'auth' | 'settings';
