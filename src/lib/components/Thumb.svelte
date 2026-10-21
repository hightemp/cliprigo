<script lang="ts">
  import { convertFileSrc } from "@tauri-apps/api/core";
  import { api } from "$lib/api";
  import Icon, { type IconName } from "./Icon.svelte";

  let {
    src,
    width = 120,
    icon = "video",
  }: { src: string | null; width?: number; icon?: IconName } = $props();

  let resolved = $state<string | null>(null);
  let failed = $state(false);

  // Remote thumbnails go through the backend cache, so the history shows them
  // offline; if caching fails, the remote URL is used directly.
  $effect(() => {
    const url = src;
    failed = false;
    resolved = null;
    if (!url || !url.startsWith("https://")) {
      resolved = url;
      return;
    }
    let stale = false;
    api
      .thumbnailCached(url)
      .then((path) => !stale && (resolved = convertFileSrc(path)))
      .catch(() => !stale && (resolved = url));
    return () => {
      stale = true;
    };
  });
</script>

<div class="thumb" style:width="{width}px">
  {#if resolved && !failed}
    <img
      src={resolved}
      alt=""
      loading="lazy"
      referrerpolicy="no-referrer"
      onerror={() => (failed = true)}
    />
  {:else}
    <Icon name={icon} size={Math.round(width / 5)} />
  {/if}
</div>

<style>
  .thumb {
    flex: none;
    aspect-ratio: 16 / 9;
    border-radius: var(--radius-sm);
    background: var(--surface-2);
    color: var(--muted);
    display: grid;
    place-items: center;
    overflow: hidden;
  }

  img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
</style>
