import { describe, expect, it } from "vitest";
import {
  buildFormat,
  codecOrAuto,
  formatFromRecord,
  initialSelection,
  qualityOptions,
  toggleAll,
} from "./selection";
import type { PlaylistEntry } from "./types";

function entry(id: string, available = true): PlaylistEntry {
  return {
    id,
    url: `https://www.youtube.com/watch?v=${id}`,
    title: id,
    uploader: null,
    duration: null,
    thumbnail: null,
    available,
  };
}

describe("initialSelection", () => {
  it("selects available entries that are not already downloaded", () => {
    const entries = [entry("a"), entry("b"), entry("c", false), entry("d")];
    const selected = initialSelection(entries, new Set(["b"]));
    expect([...selected].sort()).toEqual(["a", "d"]);
  });
});

describe("toggleAll", () => {
  const entries = [entry("a"), entry("b"), entry("c", false)];

  it("selects every available entry, including duplicates", () => {
    expect([...toggleAll(entries, new Set(["a"]))].sort()).toEqual(["a", "b"]);
  });

  it("clears when everything available is already selected", () => {
    expect(toggleAll(entries, new Set(["a", "b"])).size).toBe(0);
  });
});

describe("qualityOptions", () => {
  it("offers best plus available heights, highest first", () => {
    expect(qualityOptions([144, 1080, 720, 2160, 1080])).toEqual([
      null,
      2160,
      1080,
      720,
      144,
    ]);
    expect(qualityOptions([])).toEqual([null]);
  });
});

describe("buildFormat", () => {
  it("maps the picker state to the backend format", () => {
    expect(buildFormat("video", null, "auto", "mp3")).toEqual({
      kind: "video",
      maxHeight: null,
      codec: "auto",
    });
    expect(buildFormat("video", 720, "h264", "mp3")).toEqual({
      kind: "video",
      maxHeight: 720,
      codec: "h264",
    });
    expect(buildFormat("audio", 720, "av1", "mp3")).toEqual({
      kind: "audio_mp3",
    });
    expect(buildFormat("audio", null, "auto", "m4a")).toEqual({
      kind: "audio_m4a",
    });
  });
});

describe("formatFromRecord", () => {
  it("restores the backend format from a history record", () => {
    expect(formatFromRecord("video", 720, "vp9")).toEqual({
      kind: "video",
      maxHeight: 720,
      codec: "vp9",
    });
    // Records from before codecs, or with an unknown value: auto.
    for (const codec of [null, "hevc"]) {
      expect(formatFromRecord("video", null, codec)).toEqual({
        kind: "video",
        maxHeight: null,
        codec: "auto",
      });
    }
    expect(formatFromRecord("audio_mp3", null, null)).toEqual({
      kind: "audio_mp3",
    });
    expect(formatFromRecord("audio_m4a", 720, null)).toEqual({
      kind: "audio_m4a",
    });
    expect(formatFromRecord("webm", null, null)).toBeNull();
  });
});

describe("codecOrAuto", () => {
  it("accepts known codecs only", () => {
    expect(codecOrAuto("av1")).toBe("av1");
    expect(codecOrAuto("h264")).toBe("h264");
    expect(codecOrAuto("hevc")).toBe("auto");
    expect(codecOrAuto(undefined)).toBe("auto");
  });
});
