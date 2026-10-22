// Mirrors the serde types of the Rust backend (src-tauri/src).

export type DownloadStatus =
  | "queued"
  | "downloading"
  | "processing"
  | "completed"
  | "failed"
  | "canceled"
  | "interrupted";

export const ACTIVE_STATUSES: readonly DownloadStatus[] = [
  "queued",
  "downloading",
  "processing",
];

export interface DownloadRecord {
  id: number;
  videoId: string | null;
  url: string;
  title: string | null;
  uploader: string | null;
  duration: number | null;
  thumbnailUrl: string | null;
  /** `video`, `audio_mp3` or `audio_m4a`. */
  format: string;
  quality: number | null;
  filePath: string | null;
  fileSize: number | null;
  status: DownloadStatus;
  /** JSON `{ code, message }` for failures. */
  error: string | null;
  playlistId: string | null;
  playlistTitle: string | null;
  createdAt: number;
  completedAt: number | null;
  outputDir: string | null;
}

export interface HistoryItem extends DownloadRecord {
  fileExists: boolean;
}

export interface HistoryQuery {
  search?: string;
  status?: DownloadStatus;
  offset?: number;
  limit?: number;
}

export interface Snapshot {
  stage: "downloading" | "processing";
  part: number;
  percent: number | null;
  downloaded: number;
  total: number | null;
  speed: number | null;
  eta: number | null;
  postprocessor: string | null;
}

export interface QueueEntry extends DownloadRecord {
  progress: Snapshot | null;
}

export type QueueEvent =
  | { type: "status"; record: DownloadRecord }
  | { type: "progress"; id: number; snapshot: Snapshot };

export type Format =
  | { kind: "video"; maxHeight: number | null }
  | { kind: "audio_mp3" }
  | { kind: "audio_m4a" };

export interface QueueItemInput {
  url: string;
  videoId: string | null;
  title: string | null;
  uploader: string | null;
  duration: number | null;
  thumbnailUrl: string | null;
  playlistId: string | null;
  playlistTitle: string | null;
  format: Format;
  outputDir: string | null;
}

export type Language = "system" | "ru" | "en";

export interface Settings {
  downloadDir: string;
  defaultFormat: "video" | "audio_mp3" | "audio_m4a";
  defaultQuality: number | null;
  concurrency: number;
  filenameTemplate: string;
  language: Language;
  autoUpdateYtdlp: boolean;
  /** Empty: system settings and environment. */
  proxy: string;
}

export type ProxyFailure =
  | "proxy_unreachable"
  | "proxy_auth"
  | "proxy_refused"
  | "unreachable"
  | "timeout"
  | "http_status"
  | "unknown";

export interface ProxyCheck {
  ok: boolean;
  /** False when the field is empty: system settings apply. */
  viaProxy: boolean;
  elapsedMs: number;
  failure: ProxyFailure | null;
  /** Technical details, credentials hidden. */
  detail: string;
}

export type ComponentName = "ytdlp" | "ffmpeg" | "deno";

export interface ComponentStatus {
  component: ComponentName;
  installed: boolean;
  version: string | null;
}

export interface ComponentProgress {
  component: ComponentName;
  stage: "download" | "verify" | "install";
  file: string;
  downloaded: number;
  total: number | null;
}

export interface UpdateInfo {
  component: ComponentName;
  current: string | null;
  latest: string;
  updateAvailable: boolean;
}

export interface VideoInfo {
  id: string;
  url: string;
  title: string;
  uploader: string | null;
  duration: number | null;
  thumbnail: string | null;
  heights: number[];
}

export interface PlaylistEntry {
  id: string;
  url: string;
  title: string;
  uploader: string | null;
  duration: number | null;
  thumbnail: string | null;
  available: boolean;
}

export interface PlaylistInfo {
  id: string;
  url: string;
  title: string;
  uploader: string | null;
  entries: PlaylistEntry[];
}

export type ProbeResult =
  ({ type: "video" } & VideoInfo) | ({ type: "playlist" } & PlaylistInfo);

export interface UrlInfo {
  normalized: string;
  isYoutube: boolean;
  singleVideo: boolean;
}

/** Error shape of every failed command. */
export interface AppError {
  code: string;
  message: string;
}
