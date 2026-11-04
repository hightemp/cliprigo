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

/** The link in text shared to the app ("Share" in the YouTube app on Android),
 * which may come with the video title around it. */
export function sharedLink(text: string | null): string | null {
  const match = text?.match(/https?:\/\/[^\s<>"']+/);
  // Punctuation that ends a sentence is not part of the link.
  return match ? match[0].replace(/[).,;!?]+$/, "") : null;
}

/** Keys that reload the page in a browser engine: F5, Ctrl/Cmd+R (with Shift too). */
export function browserShortcut(
  e: Pick<KeyboardEvent, "key" | "ctrlKey" | "metaKey">,
): boolean {
  return (
    e.key === "F5" || ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "r")
  );
}
