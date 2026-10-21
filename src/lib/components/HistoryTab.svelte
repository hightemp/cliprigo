<script lang="ts">
  import { writeText } from "@tauri-apps/plugin-clipboard-manager";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { api } from "$lib/api";
  import { errorMessage, parseStoredError } from "$lib/errors";
  import { formatBytes, formatDate, formatDuration } from "$lib/format";
  import { formatFromRecord } from "$lib/selection";
  import { app, copyDetails, t, toast, toastError } from "$lib/state.svelte";
  import {
    ACTIVE_STATUSES,
    type DownloadStatus,
    type HistoryItem,
  } from "$lib/types";
  import Dialog from "./Dialog.svelte";
  import Icon from "./Icon.svelte";
  import Thumb from "./Thumb.svelte";

  const PAGE = 50;
  const FILTERS: DownloadStatus[] = [
    "completed",
    "failed",
    "canceled",
    "interrupted",
  ];

  let items = $state<HistoryItem[]>([]);
  let search = $state("");
  let status = $state<DownloadStatus | "">("");
  let loading = $state(false);
  let hasMore = $state(false);
  let removing = $state<HistoryItem | null>(null);
  let deleteFile = $state(false);
  /** Drops responses of older requests when the filter changed meanwhile. */
  let requestId = 0;

  async function load(reset: boolean) {
    const id = ++requestId;
    loading = true;
    try {
      const page = await api.historyList({
        search: search.trim() || undefined,
        status: status || undefined,
        offset: reset ? 0 : items.length,
        limit: PAGE,
      });
      if (id !== requestId) return;
      items = reset ? page : [...items, ...page];
      hasMore = page.length === PAGE;
    } catch (err) {
      toastError(err);
    } finally {
      if (id === requestId) loading = false;
    }
  }

  // Reload on filter changes (debounced while typing) and when downloads finish.
  $effect(() => {
    void search;
    void status;
    void app.historyVersion;
    const timer = setTimeout(() => load(true), 250);
    return () => clearTimeout(timer);
  });

  async function run(action: () => Promise<unknown>, item?: HistoryItem) {
    try {
      await action();
    } catch (err) {
      if (item && (err as { code?: string })?.code === "file_missing") {
        item.fileExists = false;
      }
      toastError(err);
    }
  }

  async function downloadAgain(item: HistoryItem) {
    const format = formatFromRecord(item.format, item.quality);
    if (!format) return;
    await run(async () => {
      await api.queueAdd([
        {
          url: item.url,
          videoId: item.videoId,
          title: item.title,
          uploader: item.uploader,
          duration: item.duration,
          thumbnailUrl: item.thumbnailUrl,
          playlistId: item.playlistId,
          playlistTitle: item.playlistTitle,
          format,
          outputDir: item.outputDir,
        },
      ]);
      toast(t("history.added"));
    });
  }

  async function retry(item: HistoryItem) {
    await run(async () => {
      await api.queueRetry(item.id);
      toast(t("history.added"));
      load(true);
    });
  }

  async function confirmRemove() {
    const item = removing;
    removing = null;
    if (!item) return;
    await run(async () => {
      await api.historyDelete(item.id, deleteFile);
      items = items.filter((i) => i.id !== item.id);
    });
  }

  function meta(item: HistoryItem): string {
    return [
      item.uploader,
      formatDuration(item.duration),
      formatDate(item.completedAt ?? item.createdAt, app.lang),
      formatBytes(item.fileSize, app.lang),
      t(`formats.${item.format}`),
    ]
      .filter(Boolean)
      .join(" · ");
  }

  function failure(item: HistoryItem): string | null {
    if (item.status !== "failed") return null;
    return errorMessage(parseStoredError(item.error), t);
  }
</script>

<div class="tab">
  <div class="filters">
    <div class="search">
      <Icon name="search" size={16} />
      <input
        class="input"
        type="search"
        placeholder={t("history.search")}
        bind:value={search}
      />
    </div>
    <select class="input" bind:value={status}>
      <option value="">{t("history.all")}</option>
      {#each FILTERS as s (s)}
        <option value={s}>{t(`status.${s}`)}</option>
      {/each}
    </select>
  </div>

  {#if items.length === 0 && !loading}
    <p class="muted empty">
      {search || status ? t("history.noResults") : t("history.empty")}
    </p>
  {:else}
    <ul>
      {#each items as item (item.id)}
        {@const active = ACTIVE_STATUSES.includes(item.status)}
        {@const canOpen = item.status === "completed" && item.fileExists}
        {@const error = failure(item)}
        <li class="card">
          <Thumb
            src={item.thumbnailUrl}
            width={112}
            icon={item.format === "video" ? "video" : "music"}
          />
          <div class="main">
            <div class="title-row">
              <span class="title ellipsis" title={item.title ?? item.url}
                >{item.title ?? item.url}</span
              >
              {#if item.status !== "completed"}
                <span class="badge" class:danger={item.status === "failed"}
                  >{t(`status.${item.status}`)}</span
                >
              {:else if !item.fileExists}
                <span class="badge warning">{t("history.fileMissing")}</span>
              {/if}
            </div>
            <div class="muted small ellipsis">{meta(item)}</div>
            {#if error}
              <div class="error small">{error}</div>
            {/if}
            <div class="actions">
              {#if canOpen}
                <button
                  class="btn small-btn"
                  onclick={() => run(() => api.historyOpen(item.id), item)}
                >
                  <Icon name="play" size={14} />
                  {t("history.open")}
                </button>
                <button
                  class="icon-btn"
                  title={t("history.showInFolder")}
                  aria-label={t("history.showInFolder")}
                  onclick={() => run(() => api.historyReveal(item.id), item)}
                >
                  <Icon name="folder" size={16} />
                </button>
              {/if}
              {#if item.status === "failed"}
                <button
                  class="icon-btn"
                  title={t("errors.copyDetails")}
                  aria-label={t("errors.copyDetails")}
                  onclick={() =>
                    copyDetails(
                      parseStoredError(item.error),
                      `download ${item.url} (${item.format}${item.quality ? ` ${item.quality}p` : ""})`,
                    )}
                >
                  <Icon name="paste" size={16} />
                </button>
              {/if}
              {#if ["failed", "canceled", "interrupted"].includes(item.status)}
                <button class="btn small-btn" onclick={() => retry(item)}>
                  <Icon name="retry" size={14} />
                  {t("history.retry")}
                </button>
              {:else if item.status === "completed"}
                <button
                  class="icon-btn"
                  title={t("history.downloadAgain")}
                  aria-label={t("history.downloadAgain")}
                  onclick={() => downloadAgain(item)}
                >
                  <Icon name="download" size={16} />
                </button>
              {/if}
              <button
                class="icon-btn"
                title={t("history.copyLink")}
                aria-label={t("history.copyLink")}
                onclick={() =>
                  run(async () => {
                    await writeText(item.url);
                    toast(t("history.copied"));
                  })}
              >
                <Icon name="link" size={16} />
              </button>
              <button
                class="icon-btn"
                title={t("history.openYoutube")}
                aria-label={t("history.openYoutube")}
                onclick={() => run(() => openUrl(item.url))}
              >
                <Icon name="external" size={16} />
              </button>
              {#if !active}
                <button
                  class="icon-btn"
                  title={t("history.remove")}
                  aria-label={t("history.remove")}
                  onclick={() => {
                    deleteFile = false;
                    removing = item;
                  }}
                >
                  <Icon name="trash" size={16} />
                </button>
              {/if}
            </div>
          </div>
        </li>
      {/each}
    </ul>
    {#if hasMore}
      <button class="btn more" disabled={loading} onclick={() => load(false)}>
        {loading ? t("common.loading") : t("history.loadMore")}
      </button>
    {/if}
  {/if}
</div>

{#if removing}
  {@const item = removing}
  <Dialog title={t("history.remove")} onclose={() => (removing = null)}>
    <p>{t("history.confirmRemove", { title: item.title ?? item.url })}</p>
    {#if item.filePath && item.fileExists}
      <label class="check">
        <input type="checkbox" bind:checked={deleteFile} />
        {t("history.deleteFile")}
      </label>
    {/if}
    {#snippet actions()}
      <button class="btn" onclick={() => (removing = null)}
        >{t("common.cancel")}</button
      >
      <button class="btn danger" onclick={confirmRemove}>
        <Icon name="trash" size={16} />
        {t("history.remove")}
      </button>
    {/snippet}
  </Dialog>
{/if}

<style>
  .tab {
    display: grid;
    gap: 12px;
  }

  .filters {
    display: flex;
    gap: 8px;
  }

  .search {
    flex: 1;
    position: relative;
    display: flex;
    align-items: center;
    color: var(--muted);
  }

  .search :global(svg) {
    position: absolute;
    left: 10px;
    pointer-events: none;
  }

  .search input {
    width: 100%;
    padding-left: 32px;
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

  li {
    display: flex;
    gap: 12px;
    padding: 10px;
    align-items: flex-start;
  }

  .main {
    flex: 1;
    min-width: 0;
    display: grid;
    gap: 4px;
  }

  .title-row {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }

  .title {
    font-weight: 500;
    min-width: 0;
  }

  .small {
    font-size: 12px;
  }

  .error {
    color: var(--danger);
  }

  .actions {
    display: flex;
    align-items: center;
    gap: 2px;
    margin-top: 2px;
  }

  .small-btn {
    height: 28px;
    padding: 0 10px;
    margin-right: 4px;
    font-size: 13px;
  }

  .more {
    justify-self: center;
  }

  .check {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    color: var(--text);
  }

  p {
    margin: 0 0 12px;
  }
</style>
