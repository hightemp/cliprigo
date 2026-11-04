import { describe, expect, it } from "vitest";
import { asEntry, favoriteInput, filterFavorites } from "./favorites";
import type { Favorite, PlaylistEntry } from "./types";

const saved = (
  id: string,
  title: string,
  uploader: string | null,
): Favorite => ({
  id,
  url: `https://www.youtube.com/watch?v=${id}`,
  title,
  uploader,
  duration: 213,
  thumbnail: null,
  viewCount: 5,
  published: 1_762_300_800,
  addedAt: 1,
});

describe("favorites", () => {
  it("are search results to the rest of the UI", () => {
    const entry = asEntry(saved("aaaaaaaaaaa", "Sunset", "Nature"));
    expect(entry).toEqual({
      id: "aaaaaaaaaaa",
      url: "https://www.youtube.com/watch?v=aaaaaaaaaaa",
      title: "Sunset",
      uploader: "Nature",
      duration: 213,
      thumbnail: null,
      viewCount: 5,
      published: 1_762_300_800,
      available: true,
      live: false,
    });
  });

  it("are saved from a search result", () => {
    const entry: PlaylistEntry = {
      ...asEntry(saved("aaaaaaaaaaa", "Sunset", null)),
      live: false,
    };
    expect(favoriteInput(entry)).toEqual({
      id: "aaaaaaaaaaa",
      url: "https://www.youtube.com/watch?v=aaaaaaaaaaa",
      title: "Sunset",
      uploader: null,
      duration: 213,
      thumbnail: null,
      viewCount: 5,
      published: 1_762_300_800,
    });
  });

  it("filter by title or channel, ignoring case", () => {
    const list = [
      saved("aaaaaaaaaaa", "Закат в горах", "Природа"),
      saved("bbbbbbbbbbb", "Jazz", "Cafe"),
    ];
    expect(filterFavorites(list, "  ЗАКАТ ").map((f) => f.id)).toEqual([
      "aaaaaaaaaaa",
    ]);
    expect(filterFavorites(list, "cafe").map((f) => f.id)).toEqual([
      "bbbbbbbbbbb",
    ]);
    expect(filterFavorites(list, "")).toHaveLength(2);
  });
});
