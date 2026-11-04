import { describe, expect, it } from "vitest";
import { translate } from "./i18n";
import {
  defaultTrack,
  formatTime,
  matchingCues,
  subtitlesFileName,
  toSrt,
  toText,
  toVtt,
  trackLabel,
} from "./subtitles";
import type { Cue, SubtitleTrack } from "./types";

const cues: Cue[] = [
  { startMs: 1360, endMs: 3040, text: "[Music]" },
  { startMs: 18640, endMs: 21880, text: "Second phrase\non two lines" },
  { startMs: 3_723_004, endMs: 3_725_000, text: "Third phrase" },
];

const track = (
  lang: string,
  kind: SubtitleTrack["kind"],
  name = lang,
): SubtitleTrack => ({
  lang,
  name,
  kind,
  url: `https://www.youtube.com/api/timedtext?lang=${lang}`,
});

describe("subtitle times", () => {
  it("are written as each format wants", () => {
    expect(formatTime(3_723_004, "srt")).toBe("01:02:03,004");
    expect(formatTime(3_723_004, "vtt")).toBe("01:02:03.004");
    expect(formatTime(18_640, "short")).toBe("0:18");
    expect(formatTime(3_723_004, "short")).toBe("1:02:03");
  });
});

describe("subtitle files", () => {
  it("SRT", () => {
    expect(toSrt(cues.slice(0, 2))).toBe(
      "1\n00:00:01,360 --> 00:00:03,040\n[Music]\n\n" +
        "2\n00:00:18,640 --> 00:00:21,880\nSecond phrase\non two lines\n",
    );
  });

  it("VTT", () => {
    expect(toVtt(cues.slice(0, 1))).toBe(
      "WEBVTT\n\n00:00:01.360 --> 00:00:03.040\n[Music]\n",
    );
  });

  it("plain text, one phrase per line, with or without times", () => {
    expect(toText(cues, false)).toBe(
      "[Music]\nSecond phrase on two lines\nThird phrase\n",
    );
    expect(toText(cues.slice(0, 2), true)).toBe(
      "[0:01] [Music]\n[0:18] Second phrase on two lines\n",
    );
  });

  it("are named after the video and the language", () => {
    expect(subtitlesFileName("Rick Astley - Never Gonna", "en", "srt")).toBe(
      "Rick Astley - Never Gonna.en.srt",
    );
    expect(subtitlesFileName("  ", "ru", "txt")).toBe("subtitles.ru.txt");
  });
});

describe("subtitle tracks", () => {
  it("pick the author's subtitles in the app language first", () => {
    const tracks = [
      track("en", "manual"),
      track("ru-RU", "manual"),
      track("en-orig", "auto"),
      track("ru", "translated"),
    ];
    expect(defaultTrack(tracks, "ru")?.lang).toBe("ru-RU");
  });

  it("else the video's own language over a translation", () => {
    const tracks = [
      track("de-DE", "manual"),
      track("en", "manual"),
      track("en-orig", "auto"),
      track("ru", "translated"),
    ];
    // English is what is spoken: its author's subtitles beat German ones.
    expect(defaultTrack(tracks, "ru")?.lang).toBe("en");
    expect(
      defaultTrack([track("en-orig", "auto"), track("ru", "translated")], "ru")
        ?.lang,
    ).toBe("en-orig");
    expect(defaultTrack([], "ru")).toBeNull();
  });

  it("say where they come from", () => {
    const t = (key: string) => translate("ru", key);
    expect(trackLabel(track("en", "manual", "English"), t)).toBe("English");
    expect(trackLabel(track("en-orig", "auto", "English (Original)"), t)).toBe(
      "English (Original) · автоматические",
    );
    expect(trackLabel(track("ru", "translated", "Russian"), t)).toBe(
      "Russian · автоперевод",
    );
  });
});

describe("finding in subtitles", () => {
  it("matches any case, across line breaks", () => {
    expect(matchingCues(cues, "PHRASE ON")).toEqual([1]);
    expect(matchingCues(cues, "  ")).toEqual([]);
  });
});
