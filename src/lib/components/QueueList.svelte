<script lang="ts">
  import { api } from "$lib/api";
  import { formatBytes, formatEta, formatSpeed } from "$lib/format";
  import { app, t, toastError } from "$lib/state.svelte";
  import type { QueueEntry } from "$lib/types";
  import Icon from "./Icon.svelte";
  import Thumb from "./Thumb.svelte";

  function processingLabel(entry: QueueEntry): string {
    switch (entry.progress?.postprocessor) {
      case "Merger":
        return t("queue.merging");
      case "ExtractAudio":
        return t("queue.converting");
      default:
        return t("queue.processing");
    }
  }

  function details(entry: QueueEntry): string {
    const p = entry.progress;
    if (entry.status === "queued") return t("queue.waiting");
    // yt-dlp spends a few seconds on metadata before the first progress line.
    if (!p) return t(`status.${entry.status}`);
    if (entry.status === "processing" || p.stage === "processing") {
      return processingLabel(entry);
    }
    const parts = [
      p.total
        ? `${formatBytes(p.downloaded, app.lang)} / ${formatBytes(p.total, app.lang)}`
        : formatBytes(p.downloaded, app.lang),
      formatSpeed(p.speed, app.lang),
      p.eta != null ? t("queue.left", { eta: formatEta(p.eta) }) : "",
      p.part > 1 ? t("queue.part", { part: p.part }) : "",
    ];
    return parts.filter(Boolean).join(" · ");
  }

  async function cancel(id: number) {
    try {
      await api.queueCancel(id);
    } catch (err) {
      toastError(err);
    }
  }
</script>

<section>
  <h2>
    {t("queue.title")}
    {#if app.queue.length}
      <span class="muted"
        >· {t("queue.count", { count: app.queue.length })}</span
      >
    {/if}
  </h2>

  {#if app.queue.length === 0}
    <p class="muted empty">{t("queue.empty")}</p>
  {:else}
    <ul>
      {#each app.queue as entry (entry.id)}
        {@const p = entry.progress}
        {@const busy =
          entry.status === "processing" || p?.stage === "processing"}
        <li class="card">
          <Thumb
            src={entry.thumbnailUrl}
            width={96}
            icon={entry.format === "video" ? "video" : "music"}
          />
          <div class="main">
            <div class="title ellipsis" title={entry.title ?? entry.url}>
              {entry.title ?? entry.url}
            </div>
            <div
              class="progress"
              class:indeterminate={entry.status !== "queued" &&
                (busy || p?.percent == null)}
              role="progressbar"
              aria-valuemin={0}
              aria-valuemax={100}
              aria-valuenow={entry.status === "queued"
                ? 0
                : (p?.percent ?? undefined)}
            >
              <div
                style:width="{entry.status === 'queued'
                  ? 0
                  : (p?.percent ?? 0)}%"
              ></div>
            </div>
            <div class="muted small ellipsis">{details(entry)}</div>
          </div>
          <button
            class="icon-btn"
            title={t("common.cancel")}
            aria-label={t("common.cancel")}
            onclick={() => cancel(entry.id)}
          >
            <Icon name="close" />
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</section>

<style>
  h2 {
    font-size: 15px;
    margin: 0 0 10px;
  }

  .empty {
    margin: 0;
    padding: 18px;
    text-align: center;
    border: 1px dashed var(--border);
    border-radius: var(--radius);
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
    align-items: center;
    gap: 12px;
    padding: 10px;
  }

  .main {
    flex: 1;
    min-width: 0;
    display: grid;
    gap: 6px;
  }

  .title {
    font-weight: 500;
  }

  .small {
    font-size: 12px;
  }
</style>
