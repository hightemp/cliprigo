const EXT = ".%(ext)s";

/** Same rule as the backend: the quality suffix goes before the last ".%(ext)s". */
export function missingExt(template: string): boolean {
  return !template.includes(EXT);
}

/** Ends the template with ".%(ext)s", dropping a fixed extension or a loose %(ext)s. */
export function fixTemplate(template: string): string {
  const base = template
    .trim()
    .replace(/[\s._-]*%\(ext\)s$/, "")
    .replace(/\.[A-Za-z0-9]{2,4}$/, "")
    .replace(/[\s._-]+$/, "");
  return base + EXT;
}

export const TEMPLATE_EXAMPLES = [
  "%(title)s [%(id)s].%(ext)s",
  "%(title)s.%(ext)s",
  "%(upload_date>%Y-%m-%d)s %(title)s.%(ext)s",
  "%(uploader)s/%(title)s [%(id)s].%(ext)s",
];

/** What the settings help explains, keyed by its text in `settings.template.fields`. */
export const TEMPLATE_FIELDS = [
  ["%(title)s", "title"],
  ["%(id)s", "id"],
  ["%(uploader)s", "uploader"],
  ["%(upload_date>%Y-%m-%d)s", "date"],
  ["%(ext)s", "ext"],
] as const;

export const TEMPLATE_DOCS = "https://github.com/yt-dlp/yt-dlp#output-template";

/** A help text cut into plain parts and template fields, so fields render as code. */
export function splitFields(text: string): { text: string; code: boolean }[] {
  return text
    .split(/(\.?%\([^)]*\)s)/)
    .filter(Boolean)
    .map((part) => ({
      text: part,
      code: /^\.?%\(/.test(part),
    }));
}

/**
 * The file name a template gives for `sample`, for the settings help. Covers the
 * fields shown there: plain `%(field)s` and dates as `%(field>%Y-%m-%d)s`.
 */
export function previewName(
  template: string,
  sample: Record<string, string>,
  suffix: string,
): string {
  const i = template.lastIndexOf(EXT);
  const withSuffix =
    i < 0 ? template : template.slice(0, i) + suffix + template.slice(i);
  return withSuffix.replace(
    /%\(([a-z_]+)(?:>([^)]*))?\)s/g,
    (_, field: string, format?: string) => {
      const value = sample[field];
      if (value === undefined) return "NA";
      if (!format) return value;
      // Dates are YYYYMMDD in yt-dlp.
      return format
        .replace("%Y", value.slice(0, 4))
        .replace("%m", value.slice(4, 6))
        .replace("%d", value.slice(6, 8));
    },
  );
}
