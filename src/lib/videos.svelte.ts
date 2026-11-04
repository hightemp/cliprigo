// Actions on videos found in search or saved to Favorites.
import { api } from "./api";
import { favoriteInput } from "./favorites";
import { formatAgo, formatDuration, formatViews, pluralCount } from "./format";
import { defaultFormat, duplicatesFor } from "./selection";
import { app, t, toast, toastError } from "./state.svelte";
import {
  ACTIVE_STATUSES,
  type DownloadRecord,
  type PlaylistEntry,
} from "./types";

/** Channel, views and duration. */
export function videoMeta(entry: PlaylistEntry): string {
  const views =
    entry.viewCount === null
      ? ""
      : t("search.views", {
          count: pluralCount(entry.viewCount),
          views: formatViews(entry.viewCount, app.lang),
        });
  const ago =
    entry.published === null
      ? ""
      : formatAgo(entry.published, Date.now() / 1000, app.lang);
  return [entry.uploader, views, ago, formatDuration(entry.duration)]
    .filter(Boolean)
    .join(" · ");
}

/** "Downloaded" / "In the queue" from a video's downloads, whatever the format. */
export function videoStatus(
  records: DownloadRecord[],
  id: string,
): string | null {
  const own = records.filter((r) => r.videoId === id);
  if (own.some((r) => ACTIVE_STATUSES.includes(r.status)))
    return t("search.inQueue");
  if (own.some((r) => r.status === "completed")) return t("search.downloaded");
  return null;
}

/** Queues videos in the default format from Settings, skipping those already
 * downloaded in it (the same file). Returns the new queue records. */
export async function queueVideos(
  entries: PlaylistEntry[],
): Promise<DownloadRecord[]> {
  const format = defaultFormat(app.settings);
  try {
    const existing = await api.findDuplicates(entries.map((e) => e.id));
    const done = duplicatesFor(
      existing.filter((r) => r.status === "completed"),
      format,
      app.settings?.downloadDir ?? "",
    );
    const wanted = entries.filter((e) => !e.live && !done[e.id]);
    if (wanted.length === 0) {
      toast(
        entries.length === 1
          ? t("search.alreadyHave")
          : t("favorites.allDownloaded"),
      );
      return [];
    }
    const added = await api.queueAdd(
      wanted.map((e) => ({
        url: e.url,
        videoId: e.id,
        title: e.title,
        uploader: e.uploader,
        duration: e.duration,
        thumbnailUrl: e.thumbnail,
        playlistId: null,
        playlistTitle: null,
        format,
        outputDir: null,
      })),
    );
    toast(
      entries.length === 1
        ? t("search.added")
        : t("favorites.added", { count: added.length }),
    );
    return added;
  } catch (err) {
    toastError(err);
    return [];
  }
}

export function isFavorite(id: string): boolean {
  return app.favorites.some((f) => f.id === id);
}

/** Saves a video to Favorites, or removes it if it is there. */
export async function toggleFavorite(entry: PlaylistEntry) {
  try {
    if (isFavorite(entry.id)) {
      await api.favoritesRemove(entry.id);
      app.favorites = app.favorites.filter((f) => f.id !== entry.id);
    } else {
      const saved = await api.favoritesAdd(favoriteInput(entry));
      app.favorites = [
        saved,
        ...app.favorites.filter((f) => f.id !== saved.id),
      ];
    }
  } catch (err) {
    toastError(err);
  }
}
