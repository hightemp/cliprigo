import type { Cue, SubtitleTrack } from "./types";

export type SubtitleFormat = "txt" | "srt" | "vtt";

const pad = (n: number, width = 2) => String(n).padStart(width, "0");

/** 01:02:03,004 (SRT), 01:02:03.004 (VTT) or 1:02:03 / 0:18 to show. */
export function formatTime(ms: number, style: "srt" | "vtt" | "short"): string {
  const total = Math.max(0, Math.floor(ms));
  const h = Math.floor(total / 3_600_000);
  const m = Math.floor(total / 60_000) % 60;
  const s = Math.floor(total / 1000) % 60;
  if (style === "short") {
    return h > 0 ? `${h}:${pad(m)}:${pad(s)}` : `${m}:${pad(s)}`;
  }
  const sep = style === "srt" ? "," : ".";
  return `${pad(h)}:${pad(m)}:${pad(s)}${sep}${pad(total % 1000, 3)}`;
}

export function toSrt(cues: readonly Cue[]): string {
  return cues
    .map(
      (c, i) =>
        `${i + 1}\n${formatTime(c.startMs, "srt")} --> ${formatTime(c.endMs, "srt")}\n${c.text}\n`,
    )
    .join("\n");
}

export function toVtt(cues: readonly Cue[]): string {
  return (
    "WEBVTT\n\n" +
    cues
      .map(
        (c) =>
          `${formatTime(c.startMs, "vtt")} --> ${formatTime(c.endMs, "vtt")}\n${c.text}\n`,
      )
      .join("\n")
  );
}

/** One phrase per line; `withTimes` puts [0:18] before each. */
export function toText(cues: readonly Cue[], withTimes: boolean): string {
  return cues
    .map((c) => {
      const line = c.text.replace(/\s*\n\s*/g, " ");
      return withTimes
        ? `[${formatTime(c.startMs, "short")}] ${line}\n`
        : `${line}\n`;
    })
    .join("");
}

export function formatSubtitles(
  cues: readonly Cue[],
  format: SubtitleFormat,
  withTimes: boolean,
): string {
  if (format === "srt") return toSrt(cues);
  if (format === "vtt") return toVtt(cues);
  return toText(cues, withTimes);
}

/** "Video title.en.srt"; the backend makes it safe for the file system. */
export function subtitlesFileName(
  title: string,
  lang: string,
  ext: SubtitleFormat,
): string {
  return `${title.trim() || "subtitles"}.${lang}.${ext}`;
}

const base = (lang: string) => lang.split("-")[0].toLowerCase();

/** The track to open first: the author's in the app language, else what is spoken
 * (the author's subtitles in that language, else recognized speech), else the
 * author's in any language, else a translation to the app language. */
export function defaultTrack(
  tracks: readonly SubtitleTrack[],
  appLang: string,
): SubtitleTrack | null {
  const manual = tracks.filter((t) => t.kind === "manual");
  const spoken = tracks.find((t) => t.kind === "auto");
  return (
    manual.find((t) => base(t.lang) === appLang) ??
    (spoken && manual.find((t) => base(t.lang) === base(spoken.lang))) ??
    spoken ??
    manual[0] ??
    tracks.find((t) => base(t.lang) === appLang) ??
    tracks[0] ??
    null
  );
}

export function trackLabel(
  track: SubtitleTrack,
  t: (key: string) => string,
): string {
  return track.kind === "manual"
    ? track.name
    : `${track.name} · ${t(`subtitles.kinds.${track.kind}`)}`;
}

/** Indexes of the phrases containing `text`, any case; line breaks count as spaces. */
export function matchingCues(cues: readonly Cue[], text: string): number[] {
  const wanted = text.trim().toLocaleLowerCase();
  if (!wanted) return [];
  return cues.flatMap((c, i) =>
    c.text.replace(/\s+/g, " ").toLocaleLowerCase().includes(wanted) ? [i] : [],
  );
}
