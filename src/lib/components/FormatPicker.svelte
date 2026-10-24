<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import { qualityLabel } from "$lib/format";
  import { isAndroid } from "$lib/platform";
  import { qualityOptions, type AudioFormat, type Mode } from "$lib/selection";
  import { CODECS, type Codec } from "$lib/types";
  import { app, t } from "$lib/state.svelte";
  import Icon from "./Icon.svelte";

  let {
    heights,
    mode = $bindable(),
    quality = $bindable(),
    codec = $bindable(),
    audio = $bindable(),
    outputDir = $bindable(),
  }: {
    heights: number[];
    mode: Mode;
    quality: number | null;
    codec: Codec;
    audio: AudioFormat;
    outputDir: string | null;
  } = $props();

  const options = $derived(qualityOptions(heights));
  const folder = $derived(outputDir ?? app.settings?.downloadDir ?? "");

  async function chooseFolder() {
    const picked = await open({
      directory: true,
      defaultPath: folder || undefined,
    });
    if (typeof picked === "string") outputDir = picked;
  }
</script>

<div class="picker">
  <div class="segmented" role="radiogroup">
    <button
      role="radio"
      aria-checked={mode === "video"}
      class:on={mode === "video"}
      onclick={() => (mode = "video")}
    >
      <Icon name="video" size={16} />
      {t("download.video")}
    </button>
    <button
      role="radio"
      aria-checked={mode === "audio"}
      class:on={mode === "audio"}
      onclick={() => (mode = "audio")}
    >
      <Icon name="music" size={16} />
      {t("download.audio")}
    </button>
  </div>

  {#if mode === "video"}
    <label>
      <span class="muted">{t("download.quality")}</span>
      <select class="input" bind:value={quality}>
        {#each options as h (h)}
          <option value={h}
            >{h === null ? t("download.best") : qualityLabel(h)}</option
          >
        {/each}
      </select>
    </label>
    <label>
      <span class="muted">{t("download.codec")}</span>
      <select class="input" bind:value={codec}>
        {#each CODECS as c (c)}
          <option value={c}>{t(`codecs.name.${c}`)}</option>
        {/each}
      </select>
    </label>
  {:else}
    <label>
      <span class="muted">{t("download.audioFormat")}</span>
      <select class="input" bind:value={audio}>
        <option value="mp3">MP3</option>
        <option value="m4a">M4A</option>
      </select>
    </label>
  {/if}

  <div class="folder">
    <span class="muted">{t("download.folder")}</span>
    {#if isAndroid}
      <span class="ellipsis path" title={folder}><bdi>{folder}</bdi></span>
    {:else}
      <button class="btn" onclick={chooseFolder} title={folder}>
        <Icon name="folder" size={16} />
        <span class="ellipsis path"><bdi>{folder}</bdi></span>
      </button>
    {/if}
  </div>

  {#if mode === "video"}
    <p class="hint muted">{t(`codecs.hint.${codec}`)}</p>
  {/if}
</div>

<style>
  .picker {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-end;
    gap: 12px;
  }

  .segmented {
    display: inline-flex;
    padding: 3px;
    border-radius: var(--radius-sm);
    background: var(--surface-2);
  }

  .segmented button {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 28px;
    padding: 0 12px;
    border: none;
    border-radius: 4px;
    background: transparent;
    color: var(--muted);
  }

  .segmented button.on {
    background: var(--surface);
    color: var(--text);
    box-shadow: 0 1px 2px rgb(0 0 0 / 0.1);
  }

  label,
  .folder {
    display: grid;
    gap: 4px;
    font-size: 12px;
    min-width: 0;
  }

  label select {
    font-size: 14px;
  }

  .hint {
    flex-basis: 100%;
    margin: 0;
    font-size: 12px;
    line-height: 1.4;
  }

  .folder {
    flex: 1;
    min-width: 160px;
    /* Let the long path shrink instead of widening the column. */
    grid-template-columns: minmax(0, 1fr);
  }

  .folder .btn {
    justify-content: flex-start;
    font-size: 14px;
    max-width: 100%;
    min-width: 0;
  }

  /* RTL makes the ellipsis cut the start of a long path, keeping the folder name
     visible; <bdi> keeps the path itself left-to-right so "/" stays in place. */
  .path {
    min-width: 0;
    direction: rtl;
    text-align: left;
  }
</style>
