import type { Format, PlaylistEntry } from "./types";

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

export function buildFormat(
  mode: Mode,
  quality: number | null,
  audio: AudioFormat,
): Format {
  if (mode === "video") return { kind: "video", maxHeight: quality };
  return audio === "mp3" ? { kind: "audio_mp3" } : { kind: "audio_m4a" };
}

/** Inverse of the backend's `Format::db_name`/`quality`, for "download again". */
export function formatFromRecord(
  format: string,
  quality: number | null,
): Format | null {
  switch (format) {
    case "video":
      return { kind: "video", maxHeight: quality };
    case "audio_mp3":
      return { kind: "audio_mp3" };
    case "audio_m4a":
      return { kind: "audio_m4a" };
    default:
      return null;
  }
}
