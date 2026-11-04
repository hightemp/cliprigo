import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { openUrl } from "@tauri-apps/plugin-opener";
import { api } from "./api";
import { errorMessage } from "./errors";
import { errorReport } from "./report";
import type { Lang } from "./format";
import { resolveLanguage, translate, type Params } from "./i18n";
import { isAndroid } from "./platform";
import type { QueueEntry, Settings } from "./types";

/** App-wide reactive state shared by the tabs. */
export type Tab = "download" | "search" | "history" | "settings";

export const app = $state({
  lang: "en" as Lang,
  tab: "download" as Tab,
  /** A link for the Download tab to open (from search, the YouTube window or a share). */
  pendingUrl: null as string | null,
  settings: null as Settings | null,
  queue: [] as QueueEntry[],
  /** Bumped when a download finishes, so the history reloads. */
  historyVersion: 0,
  toast: null as { text: string; error: boolean } | null,
});

/** Reactive translation: templates re-render when the language changes. */
export function t(key: string, params?: Params): string {
  return translate(app.lang, key, params);
}

export function applySettings(settings: Settings) {
  app.settings = settings;
  app.lang = resolveLanguage(settings.language, navigator.language);
  document.documentElement.lang = app.lang;
}

let toastTimer: ReturnType<typeof setTimeout> | undefined;

export function toast(text: string, error = false) {
  app.toast = { text, error };
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => (app.toast = null), error ? 6000 : 2500);
}

export function toastError(err: unknown) {
  toast(errorMessage(err, t), true);
}

/** Copies the error and a diagnostics report, for sending to the developer. */
export async function copyDetails(err: unknown, action?: string) {
  try {
    const diagnostics = await api
      .diagnostics()
      .catch(() => "(diagnostics unavailable)");
    await writeText(errorReport(err, diagnostics, action));
    toast(t("errors.copied"));
  } catch (e) {
    toastError(e);
  }
}

/** Shows a link in the Download tab, ready to pick a format. */
export function openInDownload(url: string) {
  app.pendingUrl = url;
  app.tab = "download";
}

/** Opens a link in the system browser, or the app that handles it (YouTube on Android). */
export async function openInBrowser(url: string) {
  try {
    await openUrl(url);
  } catch (err) {
    toastError(err);
  }
}

/** Browses YouTube in the app's YouTube window, which has a download button; on
 * Android, where there is no such window, in the YouTube app. */
export async function openYoutube(url: string) {
  try {
    if (isAndroid) await openUrl(url);
    else await api.youtubeOpen(url, t("youtube.download"));
  } catch (err) {
    toastError(err);
  }
}
