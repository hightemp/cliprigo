<script lang="ts">
  import { api } from "$lib/api";
  import { app, openInBrowser, t, toastError } from "$lib/state.svelte";
  import { isFavorite, toggleFavorite } from "$lib/videos.svelte";
  import type { PlaylistEntry } from "$lib/types";
  import Dialog from "./Dialog.svelte";
  import Icon from "./Icon.svelte";
  import SubtitlesDialog from "./SubtitlesDialog.svelte";

  let {
    entry,
    meta,
    onclose,
    ondownload,
    onqueue,
  }: {
    entry: PlaylistEntry;
    /** Channel, views and duration, as the search list shows them. */
    meta: string;
    onclose: () => void;
    ondownload: () => void;
    onqueue: () => void;
  } = $props();

  // The player page comes from the app's local server (see src-tauri/src/player.rs):
  // YouTube refuses to play embeds on the app's own tauri:// pages.
  let src = $state<string | null>(null);
  let subtitles = $state(false);
  $effect(() => {
    const id = entry.id;
    let stale = false;
    src = null;
    api
      .playerUrl(id, app.lang)
      .then((url) => !stale && (src = url))
      .catch((err) => !stale && toastError(err));
    return () => {
      stale = true;
    };
  });
</script>

<Dialog title={entry.title} {onclose} wide>
  <div class="frame">
    {#if src}
      <iframe
        {src}
        title={entry.title}
        allow="autoplay; encrypted-media; picture-in-picture; fullscreen"
        allowfullscreen
      ></iframe>
    {/if}
  </div>
  <p class="meta">{meta}</p>
  {#snippet actions()}
    <button class="btn" onclick={() => openInBrowser(entry.url)}>
      <Icon name="external" size={16} />
      {t("search.openOnYoutube")}
    </button>
    <button
      class="btn"
      aria-pressed={isFavorite(entry.id)}
      onclick={() => toggleFavorite(entry)}
    >
      <Icon name="star" size={16} />
      {isFavorite(entry.id) ? t("favorites.saved") : t("favorites.save")}
    </button>
    <button class="btn" onclick={() => (subtitles = true)}>
      <Icon name="captions" size={16} />
      {t("subtitles.button")}
    </button>
    <span class="spacer"></span>
    <button class="btn" onclick={onclose}>{t("common.close")}</button>
    {#if !entry.live}
      <button class="btn" data-tip={t("search.queueHint")} onclick={onqueue}>
        {t("search.queue")}
      </button>
      <button class="btn primary" onclick={ondownload}>
        <Icon name="download" size={16} />
        {t("search.download")}
      </button>
    {/if}
  {/snippet}
</Dialog>

{#if subtitles}
  <SubtitlesDialog
    url={entry.url}
    title={entry.title}
    onclose={() => (subtitles = false)}
  />
{/if}

<style>
  .frame {
    aspect-ratio: 16 / 9;
    max-height: 60vh;
    margin: 4px auto 0;
    border-radius: var(--radius-sm);
    overflow: hidden;
    background: #000;
  }

  iframe {
    display: block;
    width: 100%;
    height: 100%;
    border: 0;
  }

  .meta {
    margin: 10px 0 0;
    font-size: 13px;
  }

  .spacer {
    flex: 1;
  }
</style>
