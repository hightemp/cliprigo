<script lang="ts">
  import { readText } from "@tauri-apps/plugin-clipboard-manager";
  import { api } from "$lib/api";
  import { errorMessage } from "$lib/errors";
  import { formatDuration } from "$lib/format";
  import {
    buildFormat,
    codecOrAuto,
    initialSelection,
    toggleAll,
    type AudioFormat,
    type Mode,
  } from "$lib/selection";
  import { app, copyDetails, t, toast, toastError } from "$lib/state.svelte";
  import type {
    Codec,
    DownloadRecord,
    PlaylistEntry,
    ProbeResult,
    QueueItemInput,
    UrlInfo,
    VideoInfo,
  } from "$lib/types";
  import Dialog from "./Dialog.svelte";
  import FormatPicker from "./FormatPicker.svelte";
  import Icon from "./Icon.svelte";
  import QueueList from "./QueueList.svelte";
  import Thumb from "./Thumb.svelte";

  /** Playlists have no per-video format list; a max height still works for each item. */
  const PLAYLIST_HEIGHTS = [2160, 1440, 1080, 720, 480, 360];

  let url = $state("");
  let info = $state<UrlInfo | null>(null);
  let probing = $state(false);
  let probeError = $state<string | null>(null);
  /** Untranslated error, for "Copy details". */
  let probeErrorRaw: unknown = null;
  let probedUrl = "";
  let result = $state<ProbeResult | null>(null);
  /** Existing downloads by video id. */
  let duplicates = $state<Record<string, DownloadRecord>>({});
  let selected = $state(new Set<string>());

  let mode = $state<Mode>("video");
  let quality = $state<number | null>(null);
  let codec = $state<Codec>("auto");
  let audio = $state<AudioFormat>("mp3");
  let outputDir = $state<string | null>(null);

  let duplicateOf = $state<DownloadRecord | null>(null);
  let adding = $state(false);
  /** Ignores results of an older probe when a newer link was pasted meanwhile. */
  let probeToken = 0;

  const heights = $derived(
    result?.type === "video" ? result.heights : PLAYLIST_HEIGHTS,
  );
  const available = $derived(
    result?.type === "playlist"
      ? result.entries.filter((e) => e.available)
      : [],
  );

  function applyDefaults(videoHeights: number[]) {
    const s = app.settings;
    mode = !s || s.defaultFormat === "video" ? "video" : "audio";
    audio = s?.defaultFormat === "audio_m4a" ? "m4a" : "mp3";
    const q = s?.defaultQuality ?? null;
    quality = q !== null && videoHeights.includes(q) ? q : null;
    codec = codecOrAuto(s?.defaultCodec);
    outputDir = null;
  }

  async function probe(raw: string = url) {
    const value = raw.trim();
    if (!value) return;
    const token = ++probeToken;
    probing = true;
    probeError = null;
    result = null;
    try {
      info = await api.urlInspect(value);
      const probed = await api.videoProbe(value);
      if (token !== probeToken) return;
      const ids =
        probed.type === "video" ? [probed.id] : probed.entries.map((e) => e.id);
      const found = await api.findDuplicates(ids);
      if (token !== probeToken) return;
      duplicates = Object.fromEntries(
        found.filter((d) => d.videoId).map((d) => [d.videoId as string, d]),
      );
      applyDefaults(
        probed.type === "video" ? probed.heights : PLAYLIST_HEIGHTS,
      );
      if (probed.type === "playlist") {
        selected = initialSelection(
          probed.entries,
          new Set(Object.keys(duplicates)),
        );
      }
      result = probed;
    } catch (err) {
      if (token === probeToken) {
        probeError = errorMessage(err, t);
        probeErrorRaw = err;
        probedUrl = value;
      }
    } finally {
      if (token === probeToken) probing = false;
    }
  }

  async function pasteFromClipboard() {
    try {
      const text = (await readText())?.trim();
      if (text) {
        url = text;
        probe(text);
      }
    } catch (err) {
      toastError(err);
    }
  }

  function onPaste(e: ClipboardEvent) {
    const text = e.clipboardData?.getData("text")?.trim();
    if (!text) return;
    e.preventDefault();
    url = text;
    probe(text);
  }

  function toItem(
    v: Pick<VideoInfo, "id" | "url" | "title" | "uploader" | "duration"> & {
      thumbnail: string | null;
    },
    playlist: { id: string; title: string } | null,
  ): QueueItemInput {
    return {
      url: v.url,
      videoId: v.id,
      title: v.title,
      uploader: v.uploader,
      duration: v.duration,
      thumbnailUrl: v.thumbnail,
      playlistId: playlist?.id ?? null,
      playlistTitle: playlist?.title ?? null,
      format: buildFormat(mode, quality, codec, audio),
      outputDir,
    };
  }

  async function enqueue(items: QueueItemInput[]) {
    adding = true;
    const token = probeToken;
    try {
      await api.queueAdd(items);
      toast(t("download.added"));
      // Keep a link the user pasted while the items were being added.
      if (token === probeToken) {
        url = "";
        info = null;
        result = null;
      }
    } catch (err) {
      toastError(err);
    } finally {
      adding = false;
    }
  }

  function download(force = false) {
    if (!result) return;
    if (result.type === "video") {
      const existing = duplicates[result.id];
      if (existing && !force) {
        duplicateOf = existing;
        return;
      }
      enqueue([toItem(result, null)]);
    } else {
      const playlist = { id: result.id, title: result.title };
      const items = result.entries
        .filter((e) => selected.has(e.id))
        .map((e) => toItem(e, playlist));
      enqueue(items);
    }
  }

  function toggle(entry: PlaylistEntry) {
    const next = new Set(selected);
    if (next.has(entry.id)) next.delete(entry.id);
    else next.add(entry.id);
    selected = next;
  }

  function duplicateBadge(id: string): string | null {
    const d = duplicates[id];
    if (!d) return null;
    return d.status === "completed"
      ? t("playlist.alreadyDownloaded")
      : t("playlist.inQueue");
  }

  async function openExisting(record: DownloadRecord) {
    duplicateOf = null;
    try {
      await api.historyOpen(record.id);
    } catch (err) {
      toastError(err);
    }
  }
</script>

<div class="tab">
  <!-- novalidate: links typed without https:// are fine, the backend adds it. -->
  <form
    class="url-row"
    novalidate
    onsubmit={(e) => {
      e.preventDefault();
      probe();
    }}
  >
    <div class="url-input">
      <Icon name="link" size={16} />
      <input
        class="input"
        type="url"
        placeholder={t("download.placeholder")}
        bind:value={url}
        onpaste={onPaste}
        spellcheck="false"
        autocomplete="off"
      />
    </div>
    <button type="button" class="btn" onclick={pasteFromClipboard}>
      <Icon name="paste" size={16} />
      {t("download.paste")}
    </button>
    <button type="submit" class="btn primary" disabled={!url.trim() || probing}>
      {t("download.probe")}
    </button>
  </form>

  {#if info && !info.isYoutube}
    <p class="note warning">{t("download.notYoutube")}</p>
  {/if}

  {#if probing}
    <div class="card status-card">
      <div class="progress indeterminate"><div></div></div>
      <span class="muted">{t("download.probing")}</span>
    </div>
  {:else if probeError}
    <div class="note error">
      <p>{probeError}</p>
      <button
        class="btn small-btn"
        onclick={() => copyDetails(probeErrorRaw, `probe ${probedUrl}`)}
      >
        <Icon name="paste" size={14} />
        {t("errors.copyDetails")}
      </button>
    </div>
  {:else if result}
    <div class="card result">
      {#if result.type === "video"}
        <div class="video">
          <Thumb src={result.thumbnail} width={200} />
          <div class="meta">
            <h3>{result.title}</h3>
            <div class="muted">
              {[result.uploader, formatDuration(result.duration)]
                .filter(Boolean)
                .join(" · ")}
            </div>
            {#if duplicateBadge(result.id)}
              <span class="badge warning">{duplicateBadge(result.id)}</span>
            {/if}
          </div>
        </div>
      {:else}
        <div class="playlist-head">
          <div class="meta">
            <h3>{result.title}</h3>
            <div class="muted">
              {[
                result.uploader,
                t("playlist.count", { count: result.entries.length }),
              ]
                .filter(Boolean)
                .join(" · ")}
            </div>
          </div>
          <label class="check">
            <input
              type="checkbox"
              checked={available.length > 0 &&
                available.every((e) => selected.has(e.id))}
              onchange={() =>
                result?.type === "playlist" &&
                (selected = toggleAll(result.entries, selected))}
            />
            {t("playlist.selectAll")}
          </label>
        </div>
        <ul class="entries">
          {#each result.entries as entry (entry.id)}
            {@const badge = duplicateBadge(entry.id)}
            <li class:disabled={!entry.available}>
              <label>
                <input
                  type="checkbox"
                  disabled={!entry.available}
                  checked={selected.has(entry.id)}
                  onchange={() => toggle(entry)}
                />
                <Thumb src={entry.thumbnail} width={64} />
                <span class="ellipsis entry-title">{entry.title}</span>
                {#if !entry.available}
                  <span class="badge">{t("playlist.unavailable")}</span>
                {:else if badge}
                  <span class="badge warning">{badge}</span>
                {/if}
                <span class="muted duration"
                  >{formatDuration(entry.duration)}</span
                >
              </label>
            </li>
          {/each}
        </ul>
        <div class="muted small">
          {t("playlist.selected", {
            selected: selected.size,
            total: available.length,
          })}
        </div>
      {/if}

      <FormatPicker
        {heights}
        bind:mode
        bind:quality
        bind:codec
        bind:audio
        bind:outputDir
      />

      <div class="submit">
        <button
          class="btn primary"
          disabled={adding ||
            (result.type === "playlist" && selected.size === 0)}
          onclick={() => download()}
        >
          <Icon name="download" size={16} />
          {result.type === "video"
            ? t("download.start")
            : t("download.startCount", { count: selected.size })}
        </button>
      </div>
    </div>
  {/if}

  <QueueList />
</div>

{#if duplicateOf}
  {@const dup = duplicateOf}
  <Dialog title={t("duplicate.title")} onclose={() => (duplicateOf = null)}>
    {dup.status === "completed"
      ? t("duplicate.completed", { title: dup.title ?? dup.url })
      : t("duplicate.active", { title: dup.title ?? dup.url })}
    {#snippet actions()}
      <button class="btn" onclick={() => (duplicateOf = null)}
        >{t("common.cancel")}</button
      >
      {#if dup.status === "completed"}
        <button class="btn" onclick={() => openExisting(dup)}
          >{t("duplicate.open")}</button
        >
      {/if}
      <button
        class="btn primary"
        onclick={() => {
          duplicateOf = null;
          download(true);
        }}>{t("duplicate.again")}</button
      >
    {/snippet}
  </Dialog>
{/if}

<style>
  .tab {
    display: grid;
    gap: 16px;
  }

  .url-row {
    display: flex;
    gap: 8px;
  }

  .url-input {
    flex: 1;
    position: relative;
    display: flex;
    align-items: center;
    color: var(--muted);
  }

  .url-input :global(svg) {
    position: absolute;
    left: 10px;
    pointer-events: none;
  }

  .url-input input {
    width: 100%;
    height: 38px;
    padding-left: 34px;
  }

  .url-row .btn {
    height: 38px;
  }

  .note {
    margin: 0;
    padding: 10px 12px;
    border-radius: var(--radius-sm);
  }

  .note.warning {
    background: var(--warning-soft);
    color: var(--warning);
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

  .small-btn {
    height: 28px;
    padding: 0 10px;
    font-size: 13px;
  }

  .status-card {
    display: grid;
    gap: 10px;
    padding: 16px;
  }

  .result {
    display: grid;
    gap: 16px;
    padding: 16px;
  }

  .video {
    display: flex;
    gap: 16px;
    align-items: flex-start;
  }

  .meta {
    display: grid;
    gap: 6px;
    min-width: 0;
    align-content: start;
    justify-items: start;
  }

  h3 {
    margin: 0;
    font-size: 16px;
    line-height: 1.3;
  }

  .playlist-head {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 16px;
  }

  .check {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    white-space: nowrap;
  }

  .entries {
    list-style: none;
    margin: 0;
    padding: 4px;
    max-height: 320px;
    overflow-y: auto;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
  }

  .entries label {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px;
    border-radius: var(--radius-sm);
    cursor: pointer;
  }

  .entries label:hover {
    background: var(--surface-2);
  }

  .entries li.disabled label {
    cursor: default;
    opacity: 0.6;
  }

  .entry-title {
    flex: 1;
    min-width: 0;
  }

  .duration,
  .small {
    font-size: 12px;
  }

  .submit {
    display: flex;
    justify-content: flex-end;
  }

  @media (max-width: 600px) {
    .url-row {
      flex-wrap: wrap;
    }

    .url-input {
      flex-basis: 100%;
    }

    .url-row .btn {
      flex: 1;
      justify-content: center;
    }
  }
</style>
