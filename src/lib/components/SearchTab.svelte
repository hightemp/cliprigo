<script lang="ts">
  import { api } from "$lib/api";
  import { errorMessage } from "$lib/errors";
  import { formatDuration, formatViews, pluralCount } from "$lib/format";
  import { appendPage } from "$lib/history";
  import { isAndroid } from "$lib/platform";
  import { defaultFormat, duplicatesFor } from "$lib/selection";
  import {
    app,
    copyDetails,
    openInDownload,
    openYoutube,
    t,
    toast,
    toastError,
  } from "$lib/state.svelte";
  import {
    ACTIVE_STATUSES,
    type DownloadRecord,
    type PlaylistEntry,
  } from "$lib/types";
  import Icon from "./Icon.svelte";
  import PlayerDialog from "./PlayerDialog.svelte";
  import Thumb from "./Thumb.svelte";

  let query = $state("");
  let searched = $state("");
  let results = $state<PlaylistEntry[]>([]);
  let page = $state(1);
  let hasMore = $state(false);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let errorRaw: unknown = null;
  /** Downloads of the found videos, in any format, newest first. */
  let existing = $state<DownloadRecord[]>([]);
  let watching = $state<PlaylistEntry | null>(null);
  /** Drops answers to an older search when a new one started meanwhile. */
  let requestId = 0;

  /** Videos a quick add would download again: same file name. */
  const duplicates = $derived(
    duplicatesFor(
      existing,
      defaultFormat(app.settings),
      app.settings?.downloadDir ?? "",
    ),
  );

  async function search(next: boolean) {
    const text = next ? searched : query.trim();
    if (!text) return;
    const id = ++requestId;
    const wanted = next ? page + 1 : 1;
    loading = true;
    error = null;
    if (!next) {
      results = [];
      existing = [];
    }
    try {
      const found = await api.youtubeSearch(text, wanted);
      if (id !== requestId) return;
      const shown = next ? appendPage(results, found.entries) : found.entries;
      const records = await api
        .findDuplicates(shown.map((e) => e.id))
        .catch(() => []);
      if (id !== requestId) return;
      searched = text;
      page = wanted;
      results = shown;
      existing = records;
      hasMore = found.hasMore;
    } catch (err) {
      if (id === requestId) {
        error = errorMessage(err, t);
        errorRaw = err;
      }
    } finally {
      if (id === requestId) loading = false;
    }
  }

  /** "Downloaded" / "In the queue", whatever the format. */
  function status(entry: PlaylistEntry): string | null {
    const records = existing.filter((r) => r.videoId === entry.id);
    if (records.some((r) => ACTIVE_STATUSES.includes(r.status)))
      return t("search.inQueue");
    if (records.some((r) => r.status === "completed"))
      return t("search.downloaded");
    return null;
  }

  function meta(entry: PlaylistEntry): string {
    const views =
      entry.viewCount === null
        ? ""
        : t("search.views", {
            count: pluralCount(entry.viewCount),
            views: formatViews(entry.viewCount, app.lang),
          });
    return [entry.uploader, views, formatDuration(entry.duration)]
      .filter(Boolean)
      .join(" · ");
  }

  async function addToQueue(entry: PlaylistEntry) {
    const duplicate = duplicates[entry.id];
    if (duplicate?.status === "completed") {
      toast(t("search.alreadyHave"));
      return;
    }
    try {
      const added = await api.queueAdd([
        {
          url: entry.url,
          videoId: entry.id,
          title: entry.title,
          uploader: entry.uploader,
          duration: entry.duration,
          thumbnailUrl: entry.thumbnail,
          playlistId: null,
          playlistTitle: null,
          format: defaultFormat(app.settings),
          outputDir: null,
        },
      ]);
      existing = [...added, ...existing];
      toast(t("search.added"));
    } catch (err) {
      toastError(err);
    }
  }

  function download(entry: PlaylistEntry) {
    watching = null;
    openInDownload(entry.url);
  }
</script>

<div class="tab">
  <form
    class="search-row"
    onsubmit={(e) => {
      e.preventDefault();
      search(false);
    }}
  >
    <div class="search-input">
      <Icon name="search" size={16} />
      <input
        class="input"
        type="search"
        placeholder={t("search.placeholder")}
        aria-label={t("search.placeholder")}
        bind:value={query}
        enterkeyhint="search"
      />
    </div>
    <button type="submit" class="btn primary" disabled={!query.trim()}>
      {t("search.submit")}
    </button>
    {#if !isAndroid}
      <button
        type="button"
        class="btn"
        onclick={() => openYoutube("https://www.youtube.com/")}
      >
        <Icon name="external" size={16} />
        {t("search.openYoutube")}
      </button>
    {/if}
  </form>

  {#if error}
    <div class="note error">
      <p>{error}</p>
      <button
        class="btn small-btn"
        onclick={() => copyDetails(errorRaw, `search ${searched || query}`)}
      >
        <Icon name="paste" size={14} />
        {t("errors.copyDetails")}
      </button>
    </div>
  {/if}

  {#if results.length > 0}
    <ul>
      {#each results as entry (entry.id)}
        {@const badge = status(entry)}
        <li class="card">
          <button
            class="cover"
            onclick={() => (watching = entry)}
            aria-label={`${t("search.watch")}: ${entry.title}`}
          >
            <Thumb src={entry.thumbnail} width={168} />
            {#if entry.duration}
              <span class="duration">{formatDuration(entry.duration)}</span>
            {/if}
          </button>
          <div class="main">
            <span class="title" title={entry.title}>{entry.title}</span>
            <div class="muted small">{meta(entry)}</div>
            {#if entry.live || badge}
              <div class="badges">
                {#if entry.live}
                  <span class="badge danger">{t("search.live")}</span>
                {/if}
                {#if badge}<span class="badge warning">{badge}</span>{/if}
              </div>
            {/if}
          </div>
          <div class="actions">
            <button class="btn small-btn" onclick={() => (watching = entry)}>
              <Icon name="play" size={14} />
              {t("search.watch")}
            </button>
            {#if !entry.live}
              <button
                class="btn small-btn"
                title={t("search.downloadHint")}
                onclick={() => download(entry)}
              >
                <Icon name="download" size={14} />
                {t("search.download")}
              </button>
              <button
                class="btn small-btn"
                title={t("search.queueHint")}
                onclick={() => addToQueue(entry)}
              >
                {t("search.queue")}
              </button>
            {/if}
            <button
              class="icon-btn"
              title={t("search.openOnYoutube")}
              aria-label={t("search.openOnYoutube")}
              onclick={() => openYoutube(entry.url)}
            >
              <Icon name="external" size={16} />
            </button>
          </div>
        </li>
      {/each}
    </ul>
    {#if hasMore}
      <button class="btn more" disabled={loading} onclick={() => search(true)}>
        {loading ? t("common.loading") : t("search.more")}
      </button>
    {/if}
  {:else if loading}
    <div class="card status-card">
      <div class="progress indeterminate" role="progressbar">
        <div></div>
      </div>
      <span class="muted">{t("search.searching")}</span>
    </div>
  {:else if searched && !error}
    <p class="muted empty">{t("search.empty")}</p>
  {:else if !error}
    <p class="muted empty">{t("search.hint")}</p>
  {/if}
</div>

{#if watching}
  {@const entry = watching}
  <PlayerDialog
    {entry}
    meta={meta(entry)}
    onclose={() => (watching = null)}
    ondownload={() => download(entry)}
    onqueue={() => addToQueue(entry)}
  />
{/if}

<style>
  .tab {
    display: grid;
    gap: 12px;
  }

  .search-row {
    display: flex;
    gap: 8px;
  }

  .search-input {
    flex: 1;
    position: relative;
    display: flex;
    align-items: center;
    color: var(--muted);
  }

  .search-input :global(svg) {
    position: absolute;
    left: 10px;
    pointer-events: none;
  }

  .search-input input {
    width: 100%;
    height: 38px;
    padding-left: 34px;
  }

  .search-row .btn {
    height: 38px;
  }

  .note {
    margin: 0;
    padding: 10px 12px;
    border-radius: var(--radius-sm);
  }

  .note.error {
    display: grid;
    gap: 8px;
    justify-items: start;
    background: var(--danger-soft);
    color: var(--danger);
  }

  .note.error p {
    margin: 0;
    overflow-wrap: anywhere;
  }

  .status-card {
    display: grid;
    gap: 10px;
    padding: 16px;
  }

  .empty {
    margin: 0;
    padding: 32px;
    text-align: center;
  }

  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 8px;
  }

  /* Actions sit under the text, or under the whole card on a narrow screen. */
  li {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr);
    grid-template-areas:
      "thumb main"
      "thumb actions";
    align-content: start;
    column-gap: 12px;
    row-gap: 4px;
    padding: 10px;
    align-items: start;
  }

  .cover {
    grid-area: thumb;
    position: relative;
    padding: 0;
    border: 0;
    border-radius: var(--radius-sm);
    background: none;
    cursor: pointer;
  }

  .duration {
    position: absolute;
    right: 4px;
    bottom: 4px;
    padding: 0 4px;
    border-radius: 4px;
    background: rgb(0 0 0 / 0.75);
    color: #fff;
    font-size: 12px;
    line-height: 18px;
  }

  .main {
    grid-area: main;
    min-width: 0;
    display: grid;
    gap: 4px;
  }

  .title {
    font-weight: 500;
    line-height: 1.3;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }

  .small {
    font-size: 12px;
  }

  .badges {
    display: flex;
    gap: 6px;
  }

  .actions {
    grid-area: actions;
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 4px;
  }

  .small-btn {
    height: 28px;
    padding: 0 10px;
    font-size: 13px;
  }

  .more {
    justify-self: center;
  }

  @media (max-width: 600px) {
    .search-row {
      flex-wrap: wrap;
    }

    .search-input {
      flex-basis: 100%;
    }

    .search-row .btn {
      flex: 1;
      justify-content: center;
    }

    li {
      grid-template-areas:
        "thumb main"
        "actions actions";
    }

    .cover :global(.thumb) {
      width: 120px !important;
    }
  }
</style>
