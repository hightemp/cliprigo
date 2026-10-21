export type Lang = "ru" | "en";

const UNITS: Record<Lang, string[]> = {
  en: ["B", "KB", "MB", "GB", "TB"],
  ru: ["Б", "КБ", "МБ", "ГБ", "ТБ"],
};

const PER_SECOND: Record<Lang, string> = { en: "/s", ru: "/с" };

function number(value: number, lang: Lang): string {
  return new Intl.NumberFormat(lang, { maximumFractionDigits: 1 }).format(
    value,
  );
}

/** Human-readable size using binary multiples, e.g. "1.4 MB" / "1,4 МБ". */
export function formatBytes(
  bytes: number | null | undefined,
  lang: Lang,
): string {
  if (bytes == null || !Number.isFinite(bytes) || bytes < 0) return "";
  const units = UNITS[lang];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit++;
  }
  return `${number(unit === 0 ? Math.round(value) : value, lang)} ${units[unit]}`;
}

export function formatSpeed(
  bytesPerSecond: number | null | undefined,
  lang: Lang,
): string {
  const size = formatBytes(bytesPerSecond, lang);
  return size && `${size}${PER_SECOND[lang]}`;
}

/** `m:ss` or `h:mm:ss`. */
export function formatDuration(seconds: number | null | undefined): string {
  if (seconds == null || !Number.isFinite(seconds) || seconds < 0) return "";
  const total = Math.round(seconds);
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = String(total % 60).padStart(2, "0");
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${s}` : `${m}:${s}`;
}

export const formatEta = formatDuration;

export function formatDate(unixSeconds: number, lang: Lang): string {
  return new Intl.DateTimeFormat(lang, {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(new Date(unixSeconds * 1000));
}

const NAMED_HEIGHTS: Record<number, string> = {
  4320: "8K",
  2160: "4K",
  1440: "2K",
};

export function qualityLabel(height: number): string {
  const name = NAMED_HEIGHTS[height];
  return name ? `${name} (${height}p)` : `${height}p`;
}
