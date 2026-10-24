import {
  CODECS,
  type Codec,
  type DownloadRecord,
  type Format,
  type PlaylistEntry,
} from "./types";

export type Mode = "video" | "audio";
export type AudioFormat = "mp3" | "m4a";

/** Playlist entries checked by default: downloadable and not already downloaded. */
export function initialSelection(
  entries: readonly PlaylistEntry[],
  duplicates: ReadonlySet<string>,
): Set<string> {
  return new Set(
    entries
      .filter((e) => e.available && !duplicates.has(e.id))
      .map((e) => e.id),
  );
}

/** "Select all" toggle: checks every available entry, or clears if all are checked. */
export function toggleAll(
  entries: readonly PlaylistEntry[],
  selected: ReadonlySet<string>,
): Set<string> {
  const available = entries.filter((e) => e.available).map((e) => e.id);
  const allSelected = available.every((id) => selected.has(id));
  return allSelected ? new Set() : new Set(available);
}

/** `null` (best) followed by distinct heights, highest first. */
export function qualityOptions(heights: readonly number[]): (number | null)[] {
  const distinct = [...new Set(heights)].sort((a, b) => b - a);
  return [null, ...distinct];
}

/** A stored or saved codec name, falling back to "auto" for unknown values. */
export function codecOrAuto(value: string | null | undefined): Codec {
  return CODECS.find((c) => c === value) ?? "auto";
}

export function buildFormat(
  mode: Mode,
  quality: number | null,
  codec: Codec,
  audio: AudioFormat,
): Format {
  if (mode === "video") return { kind: "video", maxHeight: quality, codec };
  return audio === "mp3" ? { kind: "audio_mp3" } : { kind: "audio_m4a" };
}

/** Inverse of the backend's `Format::db_name`/`quality`, for "download again". */
export function formatFromRecord(
  format: string,
  quality: number | null,
  codec: string | null,
): Format | null {
  switch (format) {
    case "video":
      return { kind: "video", maxHeight: quality, codec: codecOrAuto(codec) };
    case "audio_mp3":
      return { kind: "audio_mp3" };
    case "audio_m4a":
      return { kind: "audio_m4a" };
    default:
      return null;
  }
}

/** `Format::db_name` of the backend. */
function dbName(format: Format): string {
  return format.kind;
}

/**
 * True when downloading `format` into `outputDir` writes the file of `record`
 * (the name carries the quality). Old records without codec or folder match.
 */
export function sameFiles(
  record: DownloadRecord,
  format: Format,
  outputDir: string,
): boolean {
  if (record.format !== dbName(format)) return false;
  if (record.outputDir !== null && record.outputDir !== outputDir) return false;
  if (format.kind !== "video") return true;
  return (
    record.quality === format.maxHeight &&
    codecOrAuto(record.codec) === format.codec
  );
}

/** Newest record per video that the chosen format would download again. */
export function duplicatesFor(
  records: readonly DownloadRecord[],
  format: Format,
  outputDir: string,
): Record<string, DownloadRecord> {
  const found: Record<string, DownloadRecord> = {};
  for (const r of [...records].sort((a, b) => b.id - a.id)) {
    if (r.videoId && !(r.videoId in found) && sameFiles(r, format, outputDir)) {
      found[r.videoId] = r;
    }
  }
  return found;
}
