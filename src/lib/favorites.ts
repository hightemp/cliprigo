import type { Favorite, FavoriteInput, PlaylistEntry } from "./types";

/** A saved video as a search result, so the same row and player show it. */
export function asEntry(f: Favorite): PlaylistEntry {
  return {
    id: f.id,
    url: f.url,
    title: f.title,
    uploader: f.uploader,
    duration: f.duration,
    thumbnail: f.thumbnail,
    viewCount: f.viewCount,
    published: f.published,
    available: true,
    live: false,
  };
}

export function favoriteInput(e: PlaylistEntry): FavoriteInput {
  return {
    id: e.id,
    url: e.url,
    title: e.title,
    uploader: e.uploader,
    duration: e.duration,
    thumbnail: e.thumbnail,
    viewCount: e.viewCount,
    published: e.published,
  };
}

/** Saved videos whose title or channel contains `text`, any case. */
export function filterFavorites(list: Favorite[], text: string): Favorite[] {
  const wanted = text.trim().toLocaleLowerCase();
  if (!wanted) return list;
  return list.filter((f) =>
    [f.title, f.uploader].some((s) => s?.toLocaleLowerCase().includes(wanted)),
  );
}
