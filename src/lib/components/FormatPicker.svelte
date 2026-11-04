<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import { qualityLabel } from "$lib/format";
  import { isAndroid } from "$lib/platform";
  import { radioKey } from "$lib/radio";
  import { qualityOptions, type AudioFormat, type Mode } from "$lib/selection";
  import { trackLabel } from "$lib/subtitles";
  import { CODECS, type Codec, type SubtitleTrack } from "$lib/types";
  import { app, t, toastError } from "$lib/state.svelte";
  import Icon from "./Icon.svelte";
  import Select from "./Select.svelte";

  let {
    heights,
    mode = $bindable(),
    quality = $bindable(),
    codec = $bindable(),
    audio = $bindable(),
    outputDir = $bindable(),
    subtitleTracks = [],
    subtitleLang = $bindable(null),
  }: {
    heights: number[];
    mode: Mode;
    quality: number | null;
    codec: Codec;
    audio: AudioFormat;
    outputDir: string | null;
    /** A single video's subtitles, to save one language next to it. */
    subtitleTracks?: SubtitleTrack[];
    subtitleLang?: string | null;
  } = $props();

  const options = $derived(qualityOptions(heights));
  const folder = $derived(outputDir ?? app.settings?.downloadDir ?? "");

  const MODES: Mode[] = ["video", "audio"];

  function modeKey(e: KeyboardEvent) {
    const next = radioKey(MODES, mode, e.key);
    if (!next) return;
    e.preventDefault();
    mode = next;
    const group = e.currentTarget as HTMLElement;
    group.querySelector<HTMLElement>(`[data-mode="${next}"]`)?.focus();
  }

  async function chooseFolder() {
    try {
      const picked = await open({
        directory: true,
        defaultPath: folder || undefined,
      });
      if (typeof picked === "string") outputDir = picked;
    } catch (err) {
      toastError(err);
    }
  }
</script>

<div class="picker">
  <div
    class="segmented"
    role="radiogroup"
    aria-label={t("download.mode")}
    tabindex="-1"
    onkeydown={modeKey}
  >
    {#each MODES as m (m)}
      <button
        role="radio"
        data-mode={m}
        aria-checked={mode === m}
        tabindex={mode === m ? 0 : -1}
        class:on={mode === m}
        onclick={() => (mode = m)}
      >
        <Icon name={m === "video" ? "video" : "music"} size={16} />
        {t(`download.${m}`)}
      </button>
    {/each}
  </div>

  {#if mode === "video"}
    <label>
      <span class="muted">{t("download.quality")}</span>
      <Select
        bind:value={quality}
        options={options.map((h) => ({
          value: h,
          label: h === null ? t("download.best") : qualityLabel(h),
        }))}
      />
    </label>
    <label>
      <span class="muted">{t("download.codec")}</span>
      <Select
        bind:value={codec}
        options={CODECS.map((c) => ({
          value: c,
          label: t(`codecs.name.${c}`),
        }))}
      />
    </label>
    {#if subtitleTracks.length > 0}
      <label>
        <span class="muted">{t("subtitles.title")}</span>
        <Select
          bind:value={subtitleLang}
          options={[
            { value: null, label: t("subtitles.dontSave") },
            ...subtitleTracks.map((tr) => ({
              value: tr.lang,
              label: trackLabel(tr, t),
            })),
          ]}
        />
      </label>
    {/if}
  {:else}
    <label>
      <span class="muted">{t("download.audioFormat")}</span>
      <Select
        bind:value={audio}
        options={[
          { value: "mp3", label: "MP3" },
          { value: "m4a", label: "M4A" },
        ]}
      />
    </label>
  {/if}

  <div class="folder">
    <span class="muted">{t("download.folder")}</span>
    {#if isAndroid}
      <span class="ellipsis path" data-tip={folder}><bdi>{folder}</bdi></span>
    {:else}
      <button class="btn" onclick={chooseFolder} data-tip={folder}>
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

  label :global(.select) {
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
