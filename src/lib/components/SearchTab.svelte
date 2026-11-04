<script lang="ts">
  import { api } from "$lib/api";
  import { errorMessage } from "$lib/errors";
  import { appendPage } from "$lib/history";
  import {
    NO_FILTERS,
    activeFilters,
    suggestQueries,
    toggleFeature,
  } from "$lib/search";
  import {
    app,
    copyDetails,
    openInDownload,
    reloadSearchHistory,
    t,
    toastError,
  } from "$lib/state.svelte";
  import type {
    DownloadRecord,
    PlaylistEntry,
    SearchFeature,
    SearchFilters,
    SearchRecord,
  } from "$lib/types";
  import { queueVideos, videoMeta, videoStatus } from "$lib/videos.svelte";
  import Icon from "./Icon.svelte";
  import PlayerDialog from "./PlayerDialog.svelte";
  import SearchHistoryDialog from "./SearchHistoryDialog.svelte";
  import SearchField from "./SearchField.svelte";
  import Select from "./Select.svelte";
  import VideoRow from "./VideoRow.svelte";

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
  let filters = $state<SearchFilters>({ ...NO_FILTERS });
  let showFilters = $state(false);
  const filterCount = $derived(activeFilters(filters));
  const FEATURES: SearchFeature[] = ["hd", "4k", "hdr", "subtitles"];
  let showHistory = $state(false);
  const suggestions = $derived(
    suggestQueries(app.searchHistory, query).map((s) => s.query),
  );
  const recent = $derived(app.searchHistory.slice(0, 8));

  /** Searches `text` from the history or a suggestion, with `with` filters. */
  function searchFor(text: string, withFilters?: SearchFilters) {
    query = text;
    if (withFilters)
      filters = { ...withFilters, features: [...withFilters.features] };
    search(false);
  }

  async function forget(text: string) {
    const found = app.searchHistory.find((s) => s.query === text);
    if (!found) return;
    try {
      await api.searchHistoryRemove(found.id);
      app.searchHistory = app.searchHistory.filter((s) => s.id !== found.id);
    } catch (err) {
      toastError(err);
    }
  }

  function repeat(search: SearchRecord) {
    showHistory = false;
    searchFor(search.query, search.filters);
  }

  /** A changed filter searches the last text again at once. */
  function setFilters(next: SearchFilters) {
    filters = next;
    if (searched || query.trim()) search(false);
  }
  /** Drops answers to an older search when a new one started meanwhile. */
  let requestId = 0;

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
      const found = await api.youtubeSearch(
        text,
        $state.snapshot(filters),
        wanted,
      );
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
      if (!next) reloadSearchHistory();
    } catch (err) {
      if (id === requestId) {
        error = errorMessage(err, t);
        errorRaw = err;
      }
    } finally {
      if (id === requestId) loading = false;
    }
  }

  async function addToQueue(entry: PlaylistEntry) {
    const added = await queueVideos([entry]);
    existing = [...added, ...existing];
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
    <SearchField
      bind:value={query}
      placeholder={t("search.placeholder")}
      enterkeyhint="search"
      {suggestions}
      onpick={(text) => searchFor(text)}
      onremove={forget}
    />
    <button type="submit" class="btn primary" disabled={!query.trim()}>
      {t("search.submit")}
    </button>
    <button
      type="button"
      class="btn"
      class:on={showFilters}
      aria-expanded={showFilters}
      aria-controls="search-filters"
      onclick={() => (showFilters = !showFilters)}
    >
      <Icon name="filter" size={16} />
      {t("search.filters.button")}
      {#if filterCount > 0}<span class="count">{filterCount}</span>{/if}
    </button>
    <button
      type="button"
      class="btn"
      data-tip={t("search.history.title")}
      onclick={() => (showHistory = true)}
    >
      <Icon name="history" size={16} />
      {t("search.history.button")}
    </button>
  </form>

  {#if showFilters}
    <div class="card filters" id="search-filters">
      <label>
        <span class="muted">{t("search.filters.uploadDate")}</span>
        <Select
          value={filters.uploadDate}
          onchange={(uploadDate) => setFilters({ ...filters, uploadDate })}
          options={[
            { value: null, label: t("search.filters.any") },
            ...(["hour", "today", "week", "month", "year"] as const).map(
              (d) => ({ value: d, label: t(`search.filters.date.${d}`) }),
            ),
          ]}
        />
      </label>
      <label>
        <span class="muted">{t("search.filters.length")}</span>
        <Select
          value={filters.length}
          onchange={(length) => setFilters({ ...filters, length })}
          options={[
            { value: null, label: t("search.filters.any") },
            ...(["short", "medium", "long"] as const).map((l) => ({
              value: l,
              label: t(`search.filters.lengths.${l}`),
            })),
          ]}
        />
      </label>
      <label>
        <span class="muted">{t("search.filters.sort")}</span>
        <Select
          value={filters.sort}
          onchange={(sort) => setFilters({ ...filters, sort })}
          options={[
            { value: "relevance", label: t("search.filters.sorts.relevance") },
            { value: "views", label: t("search.filters.sorts.views") },
          ]}
        />
      </label>
      <fieldset>
        <legend class="muted">{t("search.filters.quality")}</legend>
        <div class="chips">
          {#each FEATURES as f (f)}
            <label class="chip" class:on={filters.features.includes(f)}>
              <input
                type="checkbox"
                checked={filters.features.includes(f)}
                onchange={() =>
                  setFilters({
                    ...filters,
                    features: toggleFeature(filters.features, f),
                  })}
              />
              {t(`search.filters.features.${f}`)}
            </label>
          {/each}
        </div>
      </fieldset>
      {#if filterCount > 0}
        <button
          type="button"
          class="link reset"
          onclick={() => setFilters({ ...NO_FILTERS, features: [] })}
        >
          {t("search.filters.reset")}
        </button>
      {/if}
    </div>
  {/if}

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
        <VideoRow
          {entry}
          badge={videoStatus(existing, entry.id)}
          onwatch={() => (watching = entry)}
          ondownload={() => download(entry)}
          onqueue={() => addToQueue(entry)}
        />
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
    {#if recent.length > 0}
      <ul class="recent" aria-label={t("search.history.recent")}>
        {#each recent as past (past.id)}
          <li>
            <button
              class="chip"
              onclick={() => searchFor(past.query, past.filters)}
            >
              <Icon name="history" size={14} />
              {past.query}
            </button>
          </li>
        {/each}
      </ul>
    {/if}
    <p class="muted empty">{t("search.hint")}</p>
  {/if}
</div>

{#if showHistory}
  <SearchHistoryDialog
    onclose={() => (showHistory = false)}
    onrepeat={repeat}
  />
{/if}

{#if watching}
  {@const entry = watching}
  <PlayerDialog
    {entry}
    meta={videoMeta(entry)}
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

  .search-row .btn {
    height: 38px;
  }

  .btn.on {
    border-color: var(--accent);
    color: var(--accent);
  }

  .count {
    min-width: 18px;
    padding: 0 5px;
    border-radius: 9px;
    background: var(--accent);
    color: var(--on-accent);
    font-size: 11px;
    line-height: 18px;
    text-align: center;
  }

  .filters {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 12px;
    padding: 12px;
    align-items: end;
  }

  .filters > label {
    display: grid;
    gap: 4px;
    font-size: 13px;
  }

  fieldset {
    grid-column: 1 / -1;
    margin: 0;
    padding: 0;
    border: 0;
    font-size: 13px;
  }

  legend {
    padding: 0;
    margin-bottom: 6px;
  }

  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    min-height: 32px;
    padding: 0 12px;
    border: 1px solid var(--border);
    border-radius: 999px;
    cursor: pointer;
    user-select: none;
  }

  .chip.on {
    border-color: var(--accent);
    background: var(--accent-soft);
    color: var(--accent);
  }

  .chip input {
    margin: 0;
  }

  @media (pointer: coarse) {
    .chip {
      min-height: 44px;
    }
  }

  .recent {
    display: flex;
    flex-wrap: wrap;
    justify-content: center;
    gap: 8px;
    padding-top: 24px;
  }

  .recent .chip {
    max-width: 100%;
    overflow: hidden;
    background: var(--surface);
    color: var(--text);
    font: inherit;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .recent .chip:hover {
    border-color: var(--accent);
    color: var(--accent);
  }

  .reset {
    grid-column: 1 / -1;
    justify-self: start;
    padding: 0;
    border: 0;
    background: none;
    color: var(--accent);
    cursor: pointer;
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

    .search-row > :global(.field) {
      flex-basis: 100%;
    }

    .filters {
      grid-template-columns: minmax(0, 1fr);
    }

    .search-row .btn {
      flex: 1;
      justify-content: center;
    }
  }
</style>
