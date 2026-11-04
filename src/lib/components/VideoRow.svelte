<script lang="ts">
  import { formatDuration } from "$lib/format";
  import { openInBrowser, t } from "$lib/state.svelte";
  import type { PlaylistEntry } from "$lib/types";
  import { isFavorite, toggleFavorite, videoMeta } from "$lib/videos.svelte";
  import Icon from "./Icon.svelte";
  import SubtitlesDialog from "./SubtitlesDialog.svelte";
  import Thumb from "./Thumb.svelte";

  /** A found or saved video with its actions. */
  let {
    entry,
    badge,
    onwatch,
    ondownload,
    onqueue,
  }: {
    entry: PlaylistEntry;
    /** "Downloaded" / "In the queue". */
    badge: string | null;
    onwatch: () => void;
    ondownload: () => void;
    onqueue: () => void;
  } = $props();

  const saved = $derived(isFavorite(entry.id));
  let subtitles = $state(false);
</script>

<li class="card">
  <button
    class="cover"
    onclick={onwatch}
    aria-label={`${t("search.watch")}: ${entry.title}`}
  >
    <Thumb src={entry.thumbnail} width={168} />
    {#if entry.duration}
      <span class="duration">{formatDuration(entry.duration)}</span>
    {/if}
  </button>
  <div class="main">
    <span class="title" data-tip={entry.title}>{entry.title}</span>
    <div class="muted small">{videoMeta(entry)}</div>
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
    <button
      class="btn small-btn watch"
      aria-label={t("search.watch")}
      onclick={onwatch}
    >
      <Icon name="play" size={14} />
      <span class="label">{t("search.watch")}</span>
    </button>
    {#if !entry.live}
      <button
        class="btn small-btn"
        data-tip={t("search.downloadHint")}
        onclick={ondownload}
      >
        <Icon name="download" size={14} />
        {t("search.download")}
      </button>
      <button
        class="btn small-btn"
        data-tip={t("search.queueHint")}
        onclick={onqueue}
      >
        {t("search.queue")}
      </button>
    {/if}
    <button
      class="icon-btn star"
      class:on={saved}
      aria-pressed={saved}
      aria-label={t("favorites.save")}
      data-tip={saved ? t("favorites.saved") : t("favorites.save")}
      onclick={() => toggleFavorite(entry)}
    >
      <Icon name="star" size={16} />
    </button>
    <button
      class="icon-btn"
      aria-label={t("subtitles.button")}
      data-tip={t("subtitles.button")}
      onclick={() => (subtitles = true)}
    >
      <Icon name="captions" size={16} />
    </button>
    <button
      class="icon-btn"
      data-tip={t("search.openOnYoutube")}
      aria-label={t("search.openOnYoutube")}
      onclick={() => openInBrowser(entry.url)}
    >
      <Icon name="external" size={16} />
    </button>
  </div>
  {#if subtitles}
    <SubtitlesDialog
      url={entry.url}
      title={entry.title}
      onclose={() => (subtitles = false)}
    />
  {/if}
</li>

<style>
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

  /* A filled star for a saved video. */
  .star.on {
    color: #e0a400;
  }

  .star.on :global(svg) {
    fill: currentColor;
  }

  @media (max-width: 600px) {
    li {
      grid-template-areas:
        "thumb main"
        "actions actions";
    }

    .cover :global(.thumb) {
      width: 120px !important;
    }

    /* One row of actions on a phone. */
    .actions {
      gap: 2px;
    }

    .small-btn {
      padding: 0 8px;
    }

    /* The cover plays the video too; the button keeps its name for readers. */
    .watch .label {
      display: none;
    }

    .watch {
      min-width: 44px;
      justify-content: center;
    }
  }
</style>
