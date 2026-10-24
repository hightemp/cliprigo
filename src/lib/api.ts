import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  ComponentName,
  ComponentProgress,
  ComponentStatus,
  DownloadRecord,
  HistoryItem,
  HistoryQuery,
  ProbeResult,
  ProxyCheck,
  QueueEntry,
  QueueEvent,
  QueueItemInput,
  Settings,
  UpdateInfo,
  UrlInfo,
} from "./types";

/** Typed wrappers over the Tauri commands in src-tauri/src/commands.rs. */
export const api = {
  historyList: (query: HistoryQuery) =>
    invoke<HistoryItem[]>("history_list", { query }),
  historyDelete: (id: number, deleteFile: boolean) =>
    invoke<void>("history_delete", { id, deleteFile }),
  historyOpen: (id: number) => invoke<void>("history_open", { id }),
  historyReveal: (id: number) => invoke<void>("history_reveal", { id }),
  openLogs: () => invoke<void>("open_logs"),
  keepAlive: (text: string | null) => invoke<void>("keep_alive", { text }),
  diagnostics: () => invoke<string>("diagnostics"),

  settingsGet: () => invoke<Settings>("settings_get"),
  settingsSet: (settings: Settings) =>
    invoke<Settings>("settings_set", { settings }),
  proxyCheck: (proxy: string) => invoke<ProxyCheck>("proxy_check", { proxy }),

  componentsStatus: () => invoke<ComponentStatus[]>("components_status"),
  componentsInstall: (component: ComponentName) =>
    invoke<string>("components_install", { component }),
  componentsCheckUpdates: () =>
    invoke<UpdateInfo[]>("components_check_updates"),

  urlInspect: (url: string) => invoke<UrlInfo>("url_inspect", { url }),
  thumbnailCached: (url: string) => invoke<string>("thumbnail_cached", { url }),
  videoProbe: (url: string) => invoke<ProbeResult>("video_probe", { url }),

  queueAdd: (items: QueueItemInput[]) =>
    invoke<DownloadRecord[]>("queue_add", { items }),
  queueCancel: (id: number) => invoke<void>("queue_cancel", { id }),
  queueRetry: (id: number) => invoke<DownloadRecord>("queue_retry", { id }),
  queueList: () => invoke<QueueEntry[]>("queue_list"),
  findDuplicates: (videoIds: string[]) =>
    invoke<DownloadRecord[]>("find_duplicates", { videoIds }),
  appQuit: () => invoke<void>("app_quit"),
};

export async function onQueueEvent(
  handler: (event: QueueEvent) => void,
): Promise<UnlistenFn> {
  const unlisten = await Promise.all([
    listen<QueueEvent>("download://status", (e) => handler(e.payload)),
    listen<QueueEvent>("download://progress", (e) => handler(e.payload)),
  ]);
  return () => unlisten.forEach((u) => u());
}

export const onComponentProgress = (
  handler: (p: ComponentProgress) => void,
): Promise<UnlistenFn> =>
  listen<ComponentProgress>("component://progress", (e) => handler(e.payload));

export const onComponentUpdated = (
  handler: (p: { component: ComponentName; version: string }) => void,
): Promise<UnlistenFn> =>
  listen<{ component: ComponentName; version: string }>(
    "component://updated",
    (e) => handler(e.payload),
  );

export const onCloseRequested = (handler: () => void): Promise<UnlistenFn> =>
  listen("app://close-requested", () => handler());
