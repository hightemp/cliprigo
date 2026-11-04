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

/** Count that picks the plural of a word after `formatViews`: once shortened, the
 * word follows the abbreviation ("339 тыс. просмотров"), not the full number. */
export function pluralCount(count: number): number {
  return count >= 1000 ? 1000 : count;
}

/** View count in short form: "1.2M", "339K" / "1,2 млн", "339 тыс.". */
export function formatViews(
  count: number | null | undefined,
  lang: Lang,
): string {
  if (count == null || !Number.isFinite(count) || count < 0) return "";
  return new Intl.NumberFormat(lang, { notation: "compact" }).format(count);
}

export function formatDate(unixSeconds: number, lang: Lang): string {
  return new Intl.DateTimeFormat(lang, {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(new Date(unixSeconds * 1000));
}

/** The largest unit first, so 400 days is "1 year", not "57 weeks". */
const AGO_UNITS: [Intl.RelativeTimeFormatUnit, number][] = [
  ["year", 365 * 86_400],
  ["month", 30 * 86_400],
  ["week", 7 * 86_400],
  ["day", 86_400],
  ["hour", 3_600],
  ["minute", 60],
];

/** "3 days ago" (or "in 2 days" for a scheduled stream), as YouTube shows dates in
 * lists; those dates are only that precise anyway. */
export function formatAgo(
  unixSeconds: number,
  nowSeconds: number,
  lang: Lang,
): string {
  const diff = unixSeconds - nowSeconds;
  const [unit, size] =
    AGO_UNITS.find(([, size]) => Math.abs(diff) >= size) ?? AGO_UNITS[5];
  const amount = Math.max(1, Math.floor(Math.abs(diff) / size));
  return new Intl.RelativeTimeFormat(lang, { numeric: "always" }).format(
    diff < 0 ? -amount : amount,
    unit,
  );
}

/** The day a video came out. YouTube dates are days in UTC. */
export function formatDay(unixSeconds: number, lang: Lang): string {
  return new Intl.DateTimeFormat(lang, {
    dateStyle: "medium",
    timeZone: "UTC",
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
