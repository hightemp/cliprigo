import type { ComponentName } from "./types";

/** Android's WebView names the OS in its user agent. */
export function detectAndroid(userAgent: string): boolean {
  return /\bAndroid\b/.test(userAgent);
}

/** The Android app saves to Download/Cliprigo and has no file manager to reveal in. */
export const isAndroid =
  typeof navigator !== "undefined" && detectAndroid(navigator.userAgent);

/** i18n key under `components.`: on Android the JavaScript runtime is QuickJS. */
export function componentLabel(
  component: ComponentName,
  android = isAndroid,
): string {
  return android && component === "deno" ? "quickjs" : component;
}

/** Shipped inside the Android app (version set by the backend): never downloaded. */
export function isBundled(status: {
  installed: boolean;
  version: string | null;
}): boolean {
  return status.installed && status.version === "bundled";
}
