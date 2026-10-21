import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { api } from "./api";
import { errorMessage } from "./errors";
import { errorReport } from "./report";
import type { Lang } from "./format";
import { resolveLanguage, translate, type Params } from "./i18n";
import type { QueueEntry, Settings } from "./types";

/** App-wide reactive state shared by the tabs. */
export const app = $state({
  lang: "en" as Lang,
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
