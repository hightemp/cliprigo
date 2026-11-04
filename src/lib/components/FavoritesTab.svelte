<script lang="ts">
  import { api } from "$lib/api";
  import { asEntry, filterFavorites } from "$lib/favorites";
  import { app, openInDownload, t } from "$lib/state.svelte";
  import type { DownloadRecord, PlaylistEntry } from "$lib/types";
  import { queueVideos, videoMeta, videoStatus } from "$lib/videos.svelte";
  import Icon from "./Icon.svelte";
  import PlayerDialog from "./PlayerDialog.svelte";
  import SearchField from "./SearchField.svelte";
  import VideoRow from "./VideoRow.svelte";

  let filter = $state("");
  let watching = $state<PlaylistEntry | null>(null);
  /** Downloads of the saved videos, for the "Downloaded" badges. */
  let existing = $state<DownloadRecord[]>([]);

  const shown = $derived(filterFavorites(app.favorites, filter).map(asEntry));

  // Badges follow the saved list and finished downloads.
  $effect(() => {
    const ids = app.favorites.map((f) => f.id);
    void app.historyVersion;
    if (ids.length === 0) return;
    let stale = false;
    api
      .findDuplicates(ids)
      .then((r) => !stale && (existing = r))
      .catch(() => {});
    return () => {
      stale = true;
    };
  });

  async function queue(entries: PlaylistEntry[]) {
    const added = await queueVideos(entries);
    existing = [...added, ...existing];
  }

  function download(entry: PlaylistEntry) {
    watching = null;
    openInDownload(entry.url);
  }
</script>

<div class="tab">
  {#if app.favorites.length === 0}
    <div class="empty">
      <Icon name="star" size={28} />
      <p>{t("favorites.empty")}</p>
    </div>
  {:else}
    <div class="toolbar">
      <SearchField bind:value={filter} placeholder={t("favorites.filter")} />
      <button
        class="btn"
        data-tip={t("search.queueHint")}
        disabled={shown.length === 0}
        onclick={() => queue(shown)}
      >
        <Icon name="download" size={16} />
        {filter.trim()
          ? t("favorites.queueShown", { count: shown.length })
          : t("favorites.queueAll")}
      </button>
    </div>
    {#if shown.length === 0}
      <p class="muted none">{t("search.empty")}</p>
    {:else}
      <ul>
        {#each shown as entry (entry.id)}
          <VideoRow
            {entry}
            badge={videoStatus(existing, entry.id)}
            onwatch={() => (watching = entry)}
            ondownload={() => download(entry)}
            onqueue={() => queue([entry])}
          />
        {/each}
      </ul>
    {/if}
  {/if}
</div>

{#if watching}
  {@const entry = watching}
  <PlayerDialog
    {entry}
    meta={videoMeta(entry)}
    onclose={() => (watching = null)}
    ondownload={() => download(entry)}
    onqueue={() => queue([entry])}
  />
{/if}

<style>
  .tab {
    display: grid;
    gap: 12px;
  }

  .toolbar {
    display: flex;
    gap: 8px;
  }

  .toolbar .btn {
    height: 38px;
  }

  .empty {
    display: grid;
    justify-items: center;
    gap: 12px;
    padding: 48px 24px;
    color: var(--muted);
    text-align: center;
  }

  .empty p {
    margin: 0;
    max-width: 30em;
  }

  .none {
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

  @media (max-width: 600px) {
    .toolbar {
      flex-wrap: wrap;
    }

    .toolbar > :global(.field) {
      flex-basis: 100%;
    }

    .toolbar .btn {
      flex: 1;
      justify-content: center;
    }
  }
</style>
