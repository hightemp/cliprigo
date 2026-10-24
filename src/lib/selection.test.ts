import { describe, expect, it } from "vitest";
import {
  duplicatesFor,
  sameFiles,
  buildFormat,
  codecOrAuto,
  formatFromRecord,
  initialSelection,
  qualityOptions,
  toggleAll,
} from "./selection";
import type { DownloadRecord, PlaylistEntry } from "./types";

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

describe("sameFiles", () => {
  const rec = (over: Partial<DownloadRecord>): DownloadRecord => ({
    id: 1,
    videoId: "v",
    url: "https://youtu.be/v",
    title: "V",
    uploader: null,
    duration: null,
    thumbnailUrl: null,
    format: "video",
    quality: 720,
    codec: "auto",
    filePath: "/dl/V [v] 720p.mp4",
    fileSize: null,
    status: "completed",
    error: null,
    playlistId: null,
    playlistTitle: null,
    createdAt: 0,
    completedAt: 0,
    outputDir: "/dl",
    ...over,
  });
  const v720 = { kind: "video", maxHeight: 720, codec: "auto" } as const;

  it("matches only the download that writes the same file", () => {
    expect(sameFiles(rec({}), v720, "/dl")).toBe(true);
    expect(sameFiles(rec({}), { ...v720, maxHeight: 1080 }, "/dl")).toBe(false);
    expect(sameFiles(rec({}), { ...v720, codec: "h264" }, "/dl")).toBe(false);
    expect(sameFiles(rec({}), { kind: "audio_mp3" }, "/dl")).toBe(false);
    expect(sameFiles(rec({}), v720, "/other")).toBe(false);
    expect(
      sameFiles(
        rec({ format: "audio_mp3", quality: null, codec: null }),
        {
          kind: "audio_mp3",
        },
        "/dl",
      ),
    ).toBe(true);
  });

  it("treats records from before codecs and folders as matching", () => {
    expect(sameFiles(rec({ codec: null, outputDir: null }), v720, "/dl")).toBe(
      true,
    );
  });

  it("indexes the newest matching record per video", () => {
    const records = [
      rec({ id: 3, quality: 1080 }),
      rec({ id: 2 }),
      rec({ id: 1 }),
      rec({ id: 4, videoId: "w" }),
    ];
    const found = duplicatesFor(records, v720, "/dl");
    expect(Object.keys(found).sort()).toEqual(["v", "w"]);
    expect(found.v.id).toBe(2);
  });
});
