<script lang="ts">
  import { onMount } from "svelte";
  import { writeText } from "@tauri-apps/plugin-clipboard-manager";
  import { save } from "@tauri-apps/plugin-dialog";
  import { api } from "$lib/api";
  import { errorMessage } from "$lib/errors";
  import { isAndroid } from "$lib/platform";
  import { app, t, toast, toastError } from "$lib/state.svelte";
  import {
    defaultTrack,
    formatSubtitles,
    formatTime,
    matchingCues,
    subtitlesFileName,
    toText,
    trackLabel,
    type SubtitleFormat,
  } from "$lib/subtitles";
  import type { Cue, SubtitleTrack } from "$lib/types";
  import Dialog from "./Dialog.svelte";
  import Icon from "./Icon.svelte";
  import SearchField from "./SearchField.svelte";
  import Select from "./Select.svelte";

  /** The subtitles of one video: read, find, copy, save. */
  let {
    url,
    title,
    tracks: known,
    onclose,
  }: {
    url: string;
    title: string;
    /** When the video was already looked up; else it is looked up here. */
    tracks?: SubtitleTrack[];
    onclose: () => void;
  } = $props();

  let tracks = $state<SubtitleTrack[] | null>(null);
  let lang = $state<string | null>(null);
  let cues = $state<Cue[]>([]);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let query = $state("");
  let withTimes = $state(false);
  let format = $state<SubtitleFormat>("txt");
  let selectedText = $state("");
  let list = $state<HTMLOListElement>();

  const track = $derived(tracks?.find((tr) => tr.lang === lang) ?? null);
  const matches = $derived(matchingCues(cues, query));
  const matched = $derived(new Set(matches));

  onMount(() => {
    if (known) {
      useTracks(known);
    } else {
      api
        .videoProbe(url)
        .then((r) => useTracks(r.type === "video" ? r.subtitles : []))
        .catch((err) => {
          error = errorMessage(err, t);
          loading = false;
        });
    }
    // Only what is selected inside the text counts.
    const onselection = () => {
      const sel = document.getSelection();
      selectedText =
        sel && !sel.isCollapsed && list?.contains(sel.anchorNode)
          ? sel.toString()
          : "";
    };
    document.addEventListener("selectionchange", onselection);
    return () => document.removeEventListener("selectionchange", onselection);
  });

  function useTracks(found: SubtitleTrack[]) {
    tracks = found;
    lang = defaultTrack(found, app.lang)?.lang ?? null;
    if (!lang) loading = false;
  }

  // The text of the picked language; an answer for a language left meanwhile is dropped.
  $effect(() => {
    const current = track;
    if (!current) return;
    let stale = false;
    loading = true;
    error = null;
    api
      .subtitlesFetch(current.url)
      .then((found) => {
        if (stale) return;
        cues = found;
        list?.scrollTo({ top: 0 });
      })
      .catch((err) => !stale && (error = errorMessage(err, t)))
      .finally(() => !stale && (loading = false));
    return () => {
      stale = true;
    };
  });

  // The first match comes into view.
  $effect(() => {
    const first = matches[0];
    if (first === undefined || !list) return;
    const li = list.children[first] as HTMLElement | undefined;
    li?.scrollIntoView({ block: "nearest" });
  });

  /** `text` split around matches of the query, for highlighting. */
  function parts(text: string): { text: string; mark: boolean }[] {
    const wanted = query.trim().toLocaleLowerCase();
    if (!wanted) return [{ text, mark: false }];
    const lower = text.toLocaleLowerCase();
    const out: { text: string; mark: boolean }[] = [];
    let at = 0;
    for (let i = lower.indexOf(wanted); i >= 0; i = lower.indexOf(wanted, at)) {
      if (i > at) out.push({ text: text.slice(at, i), mark: false });
      out.push({ text: text.slice(i, i + wanted.length), mark: true });
      at = i + wanted.length;
    }
    if (at < text.length) out.push({ text: text.slice(at), mark: false });
    return out;
  }

  async function copy(text: string) {
    try {
      await writeText(text);
      toast(t("subtitles.copied"));
    } catch (err) {
      toastError(err);
    }
  }

  async function saveFile() {
    if (!track) return;
    const name = subtitlesFileName(title, track.lang, format);
    const text = formatSubtitles(cues, format, withTimes);
    try {
      let path: string | null = null;
      if (!isAndroid) {
        const dir = app.settings?.downloadDir ?? "";
        const sep = dir.includes("\\") ? "\\" : "/";
        path = await save({
          defaultPath: dir ? `${dir}${sep}${name}` : name,
          filters: [{ name: format.toUpperCase(), extensions: [format] }],
        });
        if (!path) return;
      }
      const written = await api.subtitlesSave(name, text, path);
      toast(t("subtitles.saved", { path: written }));
    } catch (err) {
      toastError(err);
    }
  }
</script>

<Dialog title={t("subtitles.title")} {onclose} wide>
  <div class="content">
    <p class="video">{title}</p>
    {#if tracks && tracks.length > 0}
      <div class="tools">
        <label class="language">
          <span class="muted small">{t("subtitles.language")}</span>
          <Select
            bind:value={lang}
            label={t("subtitles.language")}
            options={tracks.map((tr) => ({
              value: tr.lang,
              label: trackLabel(tr, t),
            }))}
          />
        </label>
        <div class="find">
          <SearchField bind:value={query} placeholder={t("subtitles.find")} />
          {#if query.trim()}
            <span class="muted small count" aria-live="polite">
              {matches.length > 0
                ? t("subtitles.matches", { count: matches.length })
                : t("subtitles.noMatches")}
            </span>
          {/if}
        </div>
        <label class="check">
          <input type="checkbox" bind:checked={withTimes} />
          <span>{t("subtitles.times")}</span>
        </label>
      </div>
    {/if}

    {#if error}
      <p class="note error">{error}</p>
    {:else if loading}
      <div class="status">
        <div class="progress indeterminate" role="progressbar">
          <div></div>
        </div>
        <span class="muted">{t("subtitles.loading")}</span>
      </div>
    {:else if !track}
      <p class="muted none">{t("subtitles.none")}</p>
    {:else}
      <ol class="cues selectable" bind:this={list}>
        {#each cues as cue, i (i)}
          <li class:hit={matched.has(i)}>
            {#if withTimes}
              <span class="time">{formatTime(cue.startMs, "short")}</span>
            {/if}
            <span class="text"
              >{#each parts(cue.text) as part, j (j)}{#if part.mark}<mark
                    >{part.text}</mark
                  >{:else}{part.text}{/if}{/each}</span
            >
          </li>
        {/each}
      </ol>
    {/if}
  </div>

  {#snippet actions()}
    {#if track && cues.length > 0}
      <button class="btn act" onclick={() => copy(toText(cues, withTimes))}>
        <Icon name="paste" size={16} />
        {t("subtitles.copyAll")}
      </button>
      <button
        class="btn act"
        disabled={!selectedText}
        onclick={() => copy(selectedText)}
      >
        {t("subtitles.copySelected")}
      </button>
      <span class="spacer"></span>
      <span class="format">
        <Select
          bind:value={format}
          label={t("subtitles.format")}
          options={(["txt", "srt", "vtt"] as const).map((f) => ({
            value: f,
            label: t(`subtitles.formats.${f}`),
          }))}
        />
      </span>
      <button class="btn primary act" onclick={saveFile}>
        <Icon name="download" size={16} />
        {t("subtitles.save")}
      </button>
    {/if}
  {/snippet}
</Dialog>

<style>
  .content {
    display: grid;
    gap: 12px;
    color: var(--text);
  }

  .video {
    margin: 0;
    overflow: hidden;
    color: var(--muted);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .tools {
    display: grid;
    grid-template-columns: minmax(180px, 260px) minmax(0, 1fr) auto;
    gap: 12px;
    align-items: end;
  }

  .language {
    display: grid;
    gap: 4px;
  }

  .find {
    display: grid;
    gap: 4px;
  }

  .find .count {
    order: -1;
  }

  .check {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 38px;
    white-space: nowrap;
  }

  .cues {
    height: min(52vh, 520px);
    margin: 0;
    padding: 4px 8px 4px 0;
    overflow-y: auto;
    list-style: none;
    line-height: 1.55;
  }

  .cues li {
    display: flex;
    gap: 12px;
    padding: 2px 6px;
    border-radius: 6px;
  }

  .cues li.hit {
    background: var(--accent-soft);
  }

  .time {
    flex: none;
    min-width: 3.5em;
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }

  .text {
    white-space: pre-line;
  }

  mark {
    border-radius: 3px;
    background: color-mix(in srgb, #f59e0b 45%, transparent);
    color: inherit;
  }

  .status {
    display: grid;
    gap: 10px;
    padding: 16px 0;
  }

  .none {
    margin: 0;
    padding: 32px;
    text-align: center;
  }

  .spacer {
    flex: 1;
  }

  .format {
    display: flex;
    min-width: 150px;
  }

  .format > :global(.wrap) {
    flex: 1;
  }

  @media (max-width: 600px) {
    .tools {
      grid-template-columns: minmax(0, 1fr);
    }

    /* The tools and the buttons below take most of a phone screen. */
    .cues {
      height: max(160px, calc(100dvh - 620px));
    }

    .check {
      min-height: 44px;
    }

    /* Two buttons a row. */
    .act,
    .format {
      flex: 1 1 calc(50% - 4px);
      justify-content: center;
    }

    .spacer {
      display: none;
    }
  }
</style>
