<script lang="ts">
  import { api } from "$lib/api";
  import { formatDate } from "$lib/format";
  import { filterLabels } from "$lib/search";
  import { app, openInDownload, t, toastError } from "$lib/state.svelte";
  import type { DownloadRecord, PlaylistEntry, SearchRecord } from "$lib/types";
  import { queueVideos, videoMeta, videoStatus } from "$lib/videos.svelte";
  import Dialog from "./Dialog.svelte";
  import Icon from "./Icon.svelte";
  import PlayerDialog from "./PlayerDialog.svelte";
  import VideoRow from "./VideoRow.svelte";

  /** Past searches on the left, the results saved for the picked one on the right. */
  let {
    onclose,
    onrepeat,
  }: {
    onclose: () => void;
    /** Run this search again in the Search tab. */
    onrepeat: (search: SearchRecord) => void;
  } = $props();

  // A phone shows one pane at a time: the searches until one is picked.
  const narrow = matchMedia("(max-width: 600px)").matches;
  let selectedId = $state<number | null>(
    narrow ? null : (app.searchHistory[0]?.id ?? null),
  );
  const selected = $derived(
    app.searchHistory.find((s) => s.id === selectedId) ?? null,
  );
  let results = $state<PlaylistEntry[]>([]);
  let loading = $state(false);
  /** Downloads of the shown videos, for the badges. */
  let existing = $state<DownloadRecord[]>([]);
  let watching = $state<PlaylistEntry | null>(null);

  $effect(() => {
    const id = selectedId;
    results = [];
    existing = [];
    if (id === null) return;
    let stale = false;
    loading = true;
    api
      .searchHistoryResults(id)
      .then(async (found) => {
        if (stale) return;
        results = found;
        const records = await api
          .findDuplicates(found.map((e) => e.id))
          .catch(() => []);
        if (!stale) existing = records;
      })
      .catch((err) => !stale && toastError(err))
      .finally(() => !stale && (loading = false));
    return () => {
      stale = true;
    };
  });

  async function remove(search: SearchRecord) {
    try {
      await api.searchHistoryRemove(search.id);
    } catch (err) {
      toastError(err);
      return;
    }
    const at = app.searchHistory.indexOf(search);
    app.searchHistory = app.searchHistory.filter((s) => s.id !== search.id);
    if (selectedId === search.id) {
      const next =
        app.searchHistory[Math.min(at, app.searchHistory.length - 1)];
      selectedId = narrow ? null : (next?.id ?? null);
    }
  }

  async function clear() {
    try {
      await api.searchHistoryClear();
      app.searchHistory = [];
      selectedId = null;
    } catch (err) {
      toastError(err);
    }
  }

  async function queue(entry: PlaylistEntry) {
    const added = await queueVideos([entry]);
    existing = [...added, ...existing];
  }

  function download(entry: PlaylistEntry) {
    watching = null;
    onclose();
    openInDownload(entry.url);
  }

  function summary(search: SearchRecord): string {
    return [
      ...filterLabels(search.filters, t),
      formatDate(search.searchedAt, app.lang),
      t("search.history.results", { count: search.resultCount }),
    ].join(" · ");
  }
</script>

<Dialog title={t("search.history.title")} {onclose} wide>
  {#if app.searchHistory.length === 0}
    <div class="empty">
      <Icon name="history" size={28} />
      <p>{t("search.history.empty")}</p>
    </div>
  {:else}
    <div class="panes" class:picked={selected !== null}>
      <ul class="searches" aria-label={t("search.history.searches")}>
        {#each app.searchHistory as search (search.id)}
          <li class:on={search.id === selectedId}>
            <button
              class="pick"
              aria-current={search.id === selectedId}
              onclick={() => (selectedId = search.id)}
            >
              <span class="query">{search.query}</span>
              <span class="muted small">{summary(search)}</span>
            </button>
            <button
              class="remove"
              aria-label={t("search.history.remove")}
              data-tip={t("search.history.remove")}
              onclick={() => remove(search)}
            >
              <Icon name="close" size={14} />
            </button>
          </li>
        {/each}
      </ul>

      <section class="results">
        {#if selected}
          <div class="head">
            {#if narrow}
              <button class="btn" onclick={() => (selectedId = null)}>
                {t("search.history.back")}
              </button>
            {/if}
            <h3>{selected.query}</h3>
            <button class="btn primary" onclick={() => onrepeat(selected)}>
              <Icon name="search" size={16} />
              {t("search.history.again")}
            </button>
          </div>
          {#if results.length > 0}
            <ul class="videos">
              {#each results as entry (entry.id)}
                <VideoRow
                  {entry}
                  badge={videoStatus(existing, entry.id)}
                  onwatch={() => (watching = entry)}
                  ondownload={() => download(entry)}
                  onqueue={() => queue(entry)}
                />
              {/each}
            </ul>
          {:else if !loading}
            <p class="muted none">{t("search.history.noResults")}</p>
          {/if}
        {/if}
      </section>
    </div>
  {/if}

  {#snippet actions()}
    {#if app.searchHistory.length > 0}
      <button class="btn danger-text" onclick={clear}>
        <Icon name="trash" size={16} />
        {t("search.history.clear")}
      </button>
    {/if}
  {/snippet}
</Dialog>

{#if watching}
  {@const entry = watching}
  <PlayerDialog
    {entry}
    meta={videoMeta(entry)}
    onclose={() => (watching = null)}
    ondownload={() => download(entry)}
    onqueue={() => queue(entry)}
  />
{/if}

<style>
  .panes {
    display: grid;
    grid-template-columns: minmax(200px, 280px) minmax(0, 1fr);
    gap: 16px;
    height: min(64vh, 640px);
    color: var(--text);
  }

  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }

  .searches {
    overflow-y: auto;
    border-right: 1px solid var(--border);
    padding-right: 8px;
  }

  .searches li {
    display: flex;
    align-items: center;
    border-radius: var(--radius-sm);
  }

  .searches li.on {
    background: var(--accent-soft);
  }

  .pick {
    flex: 1;
    min-width: 0;
    display: grid;
    gap: 2px;
    padding: 8px 10px;
    border: 0;
    background: none;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .query {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: 600;
  }

  li.on .query {
    color: var(--accent);
  }

  .remove {
    flex: none;
    display: grid;
    place-items: center;
    width: 30px;
    height: 30px;
    margin-right: 4px;
    padding: 0;
    border: 0;
    border-radius: 6px;
    background: none;
    color: var(--muted);
    cursor: pointer;
  }

  .remove:hover {
    background: var(--surface-2);
    color: var(--text);
  }

  .results {
    display: flex;
    flex-direction: column;
    gap: 12px;
    min-height: 0;
  }

  .head {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  h3 {
    flex: 1;
    min-width: 0;
    margin: 0;
    overflow: hidden;
    font-size: 15px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .videos {
    display: grid;
    gap: 8px;
    overflow-y: auto;
  }

  .none,
  .empty {
    margin: 0;
    padding: 32px;
    text-align: center;
  }

  .empty {
    display: grid;
    justify-items: center;
    gap: 8px;
    color: var(--muted);
  }

  .danger-text {
    margin-right: auto;
    color: var(--danger);
  }

  @media (pointer: coarse) {
    .remove {
      width: 44px;
      height: 44px;
    }
  }

  @media (max-width: 600px) {
    .panes {
      grid-template-columns: minmax(0, 1fr);
      height: min(70vh, 640px);
    }

    .searches {
      border-right: 0;
      padding-right: 0;
    }

    .panes.picked .searches,
    .panes:not(.picked) .results {
      display: none;
    }

    /* The query on its own line, the buttons under it. */
    .head {
      flex-wrap: wrap;
    }

    h3 {
      flex-basis: 100%;
      order: -1;
      white-space: normal;
    }

    .head .primary {
      flex: 1;
      justify-content: center;
    }
  }
</style>
