<script lang="ts">
  import { onMount } from "svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { api, onComponentProgress } from "$lib/api";
  import { formatBytes, qualityLabel } from "$lib/format";
  import { componentLabel, isAndroid, isBundled } from "$lib/platform";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { mergeSaved } from "$lib/settings";
  import {
    TEMPLATE_DOCS,
    TEMPLATE_EXAMPLES,
    TEMPLATE_FIELDS,
    fixTemplate,
    missingExt,
    previewName,
    splitFields,
  } from "$lib/template";
  import {
    app,
    applySettings,
    copyDetails,
    t,
    toastError,
  } from "$lib/state.svelte";
  import { CODECS } from "$lib/types";
  import type {
    ComponentName,
    ComponentProgress,
    ComponentStatus,
    ProxyCheck,
    Settings,
    UpdateInfo,
  } from "$lib/types";
  import Icon from "./Icon.svelte";

  const QUALITIES = [2160, 1440, 1080, 720, 480, 360];

  // Settings are loaded before the tabs render, so they are always present here.
  let draft = $state<Settings>({ ...(app.settings as Settings) });
  let savedVisible = $state(false);
  let savedTimer: ReturnType<typeof setTimeout> | undefined;

  let components = $state<ComponentStatus[]>([]);
  let updates = $state<Record<string, UpdateInfo>>({});
  let checking = $state(false);
  let installing = $state<ComponentName | null>(null);
  let progress = $state<ComponentProgress | null>(null);

  async function save() {
    const sent = $state.snapshot(draft);
    try {
      const saved = await api.settingsSet(sent);
      applySettings(saved);
      draft = mergeSaved(sent, saved, $state.snapshot(draft));
      savedVisible = true;
      clearTimeout(savedTimer);
      savedTimer = setTimeout(() => (savedVisible = false), 1500);
    } catch (err) {
      toastError(err);
      // Show what is actually stored, not the rejected value.
      if (app.settings)
        draft = mergeSaved(sent, app.settings, $state.snapshot(draft));
    }
  }

  // Help shows names for a 720p video, the most common pick.
  const sample = $derived({
    title: t("settings.template.sampleTitle"),
    id: "dQw4w9WgXcQ",
    uploader: t("settings.template.sampleUploader"),
    upload_date: "20261024",
    ext: "mp4",
  });
  const preview = (template: string) => previewName(template, sample, " 720p");

  function setTemplate(template: string) {
    draft.filenameTemplate = template;
    save();
  }

  let proxyChecking = $state(false);
  let proxyResult = $state<ProxyCheck | null>(null);

  async function checkProxy() {
    proxyChecking = true;
    proxyResult = null;
    try {
      // The typed value, so a proxy can be tried before it is saved.
      proxyResult = await api.proxyCheck(draft.proxy);
    } catch (err) {
      toastError(err);
    } finally {
      proxyChecking = false;
    }
  }

  function proxyResultText(r: ProxyCheck): string {
    if (r.ok)
      return t(r.viaProxy ? "settings.proxyOk" : "settings.directOk", {
        ms: r.elapsedMs,
      });
    return t(`settings.proxyFailure.${r.failure ?? "unknown"}`, {
      detail: r.detail,
    });
  }

  async function chooseFolder() {
    try {
      const picked = await open({
        directory: true,
        defaultPath: draft.downloadDir || undefined,
      });
      if (typeof picked === "string") {
        draft.downloadDir = picked;
        save();
      }
    } catch (err) {
      toastError(err);
    }
  }

  async function refreshComponents() {
    components = await api.componentsStatus();
  }

  async function checkUpdates() {
    checking = true;
    try {
      const list = await api.componentsCheckUpdates();
      updates = Object.fromEntries(list.map((u) => [u.component, u]));
    } catch (err) {
      toastError(err);
    } finally {
      checking = false;
    }
  }

  async function install(name: ComponentName) {
    installing = name;
    progress = null;
    try {
      await api.componentsInstall(name);
      await refreshComponents();
      const info = updates[name];
      if (info)
        updates[name] = {
          ...info,
          current: info.latest,
          updateAvailable: false,
        };
    } catch (err) {
      toastError(err);
    } finally {
      installing = null;
    }
  }

  onMount(() => {
    refreshComponents();
    const unlisten = onComponentProgress((p) => (progress = p));
    return () => unlisten.then((u) => u());
  });
</script>

{#snippet withFields(text: string)}
  {#each splitFields(text) as part, i (i)}
    {#if part.code}<code class="field">{part.text}</code>{:else}{part.text}{/if}
  {/each}
{/snippet}

<div class="tab">
  <section class="card">
    <div class="row">
      <span>{t("settings.downloadDir")}</span>
      <!-- Android has no folder picker for plain paths; files go to Download/Cliprigo. -->
      {#if isAndroid}
        <span class="ellipsis path muted"><bdi>{draft.downloadDir}</bdi></span>
      {:else}
        <button
          class="btn folder"
          onclick={chooseFolder}
          title={draft.downloadDir}
        >
          <Icon name="folder" size={16} />
          <span class="ellipsis path"><bdi>{draft.downloadDir}</bdi></span>
        </button>
      {/if}
    </div>

    <label class="row">
      <span>{t("settings.defaultFormat")}</span>
      <select class="input" bind:value={draft.defaultFormat} onchange={save}>
        <option value="video">{t("formats.video")}</option>
        <option value="audio_mp3">{t("formats.audio_mp3")}</option>
        <option value="audio_m4a">{t("formats.audio_m4a")}</option>
      </select>
    </label>

    {#if draft.defaultFormat === "video"}
      <label class="row">
        <span>{t("settings.defaultQuality")}</span>
        <select class="input" bind:value={draft.defaultQuality} onchange={save}>
          <option value={null}>{t("download.best")}</option>
          {#each QUALITIES as q (q)}
            <option value={q}>{qualityLabel(q)}</option>
          {/each}
        </select>
      </label>

      <div class="row column">
        <div class="row">
          <label for="default-codec">{t("settings.defaultCodec")}</label>
          <select
            id="default-codec"
            class="input"
            bind:value={draft.defaultCodec}
            onchange={save}
          >
            {#each CODECS as c (c)}
              <option value={c}>{t(`codecs.name.${c}`)}</option>
            {/each}
          </select>
        </div>
        <span class="muted small">{t(`codecs.hint.${draft.defaultCodec}`)}</span
        >
      </div>
    {/if}

    <label class="row">
      <span>{t("settings.concurrency")}</span>
      <select class="input" bind:value={draft.concurrency} onchange={save}>
        {#each [1, 2, 3, 4, 5] as n (n)}
          <option value={n}>{n}</option>
        {/each}
      </select>
    </label>

    <div class="row column">
      <label for="filename-template">{t("settings.filenameTemplate")}</label>
      <input
        id="filename-template"
        class="input"
        bind:value={draft.filenameTemplate}
        onchange={save}
        spellcheck="false"
        autocapitalize="off"
        autocomplete="off"
      />
      <span class="muted small preview">
        {t("settings.template.preview")}
        <bdi>{preview(draft.filenameTemplate)}</bdi>
      </span>
      {#if missingExt(draft.filenameTemplate)}
        <div class="template-warning small" role="alert">
          <Icon name="warning" size={14} />
          <span>{@render withFields(t("settings.template.noExt"))}</span>
          <button
            class="btn small-btn"
            onclick={() => setTemplate(fixTemplate(draft.filenameTemplate))}
          >
            {t("settings.template.fix")}
          </button>
        </div>
      {/if}
      <span class="muted small"
        >{@render withFields(t("settings.filenameHint"))}</span
      >
      <details class="template-help small">
        <summary>{t("settings.template.help")}</summary>
        <h4>{t("settings.template.fieldsTitle")}</h4>
        <dl>
          {#each TEMPLATE_FIELDS as [code, key] (key)}
            <dt><code>{code}</code></dt>
            <dd>{t(`settings.template.fields.${key}`)}</dd>
          {/each}
        </dl>
        <h4>{t("settings.template.examplesTitle")}</h4>
        <ul class="examples">
          {#each TEMPLATE_EXAMPLES as example (example)}
            <li>
              <button
                class="example"
                class:current={example === draft.filenameTemplate}
                onclick={() => setTemplate(example)}
                ><code>{example}</code>
                <span class="muted">→ {preview(example)}</span></button
              >
            </li>
          {/each}
        </ul>
        <p>{@render withFields(t("settings.template.notes"))}</p>
        <button
          class="link"
          onclick={() => openUrl(TEMPLATE_DOCS).catch(toastError)}
        >
          <Icon name="external" size={14} />
          {t("settings.template.docs")}
        </button>
      </details>
    </div>

    <div class="row column">
      <label for="proxy">{t("settings.proxy")}</label>
      <div class="inline">
        <input
          id="proxy"
          class="input"
          bind:value={draft.proxy}
          oninput={() => (proxyResult = null)}
          onchange={save}
          placeholder="http://127.0.0.1:8888"
          spellcheck="false"
          autocomplete="off"
        />
        <button class="btn" disabled={proxyChecking} onclick={checkProxy}>
          {proxyChecking ? t("settings.checking") : t("settings.checkProxy")}
        </button>
      </div>
      {#if proxyResult}
        {@const r = proxyResult}
        <div class="proxy-result small" class:ok={r.ok} aria-live="polite">
          <Icon name={r.ok ? "check" : "close"} size={14} />
          <span>{proxyResultText(r)}</span>
          {#if !r.ok}
            <button
              class="btn small-btn"
              onclick={() =>
                copyDetails(
                  { code: `proxy_check_${r.failure}`, message: r.detail },
                  "proxy check",
                )}
            >
              <Icon name="paste" size={14} />
              {t("errors.copyDetails")}
            </button>
          {/if}
        </div>
      {/if}
      <span class="muted small">{t("settings.proxyHint")}</span>
    </div>

    <label class="row">
      <span>{t("settings.language")}</span>
      <select class="input" bind:value={draft.language} onchange={save}>
        <option value="system">{t("settings.languageSystem")}</option>
        <option value="ru">Русский</option>
        <option value="en">English</option>
      </select>
    </label>

    <label class="row check">
      <input
        type="checkbox"
        bind:checked={draft.autoUpdateYtdlp}
        onchange={save}
      />
      <span>{t("settings.autoUpdate")}</span>
    </label>

    <div
      class="saved muted small"
      class:visible={savedVisible}
      aria-live="polite"
    >
      <Icon name="check" size={14} />
      {t("settings.saved")}
    </div>
  </section>

  <section class="card">
    <div class="section-head">
      <h2>{t("settings.components")}</h2>
      <button
        class="btn"
        disabled={checking || installing !== null}
        onclick={checkUpdates}
      >
        <Icon name="retry" size={16} />
        {checking ? t("settings.checking") : t("settings.checkUpdates")}
      </button>
    </div>
    <ul>
      {#each components as c (c.component)}
        {@const update = updates[c.component]}
        <li>
          <div class="component">
            <strong>{t(`components.${componentLabel(c.component)}`)}</strong>
            <span class="muted small"
              >{isBundled(c)
                ? t("settings.bundled")
                : c.installed
                  ? c.version
                  : t("settings.notInstalled")}</span
            >
          </div>
          {#if isBundled(c)}
            <!-- Updated with the app. -->
          {:else if installing === c.component}
            <div class="installing muted small">
              {#if progress?.component === c.component && progress.stage === "download"}
                {formatBytes(progress.downloaded, app.lang)}
                {#if progress.total}/ {formatBytes(
                    progress.total,
                    app.lang,
                  )}{/if}
              {:else}
                {t("common.loading")}
              {/if}
            </div>
          {:else if update?.updateAvailable}
            <button
              class="btn primary"
              disabled={installing !== null}
              onclick={() => install(c.component)}
            >
              {t("settings.update", { version: update.latest })}
            </button>
          {:else}
            {#if update}<span class="muted small">{t("settings.upToDate")}</span
              >{/if}
            <button
              class="btn"
              disabled={installing !== null}
              onclick={() => install(c.component)}
            >
              {t("settings.reinstall")}
            </button>
          {/if}
        </li>
      {/each}
    </ul>
    <div class="buttons">
      <button class="btn" onclick={() => copyDetails(null, "diagnostics")}>
        <Icon name="paste" size={16} />
        {t("settings.copyDiagnostics")}
      </button>
      {#if !isAndroid}
        <button class="btn" onclick={() => api.openLogs().catch(toastError)}>
          <Icon name="folder" size={16} />
          {t("settings.openLogs")}
        </button>
      {/if}
    </div>
  </section>
</div>

<style>
  /* minmax(0, …): a long folder path is cut with an ellipsis instead of widening
     the page. */
  .tab {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    gap: 16px;
  }

  section {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    gap: 14px;
    padding: 16px;
  }

  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
  }

  .row > span:first-child {
    flex: none;
  }

  .row.column {
    display: grid;
    gap: 6px;
  }

  .inline {
    display: flex;
    gap: 8px;
  }

  .inline input {
    flex: 1;
    min-width: 0;
  }

  .proxy-result {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 6px;
    color: var(--danger);
  }

  .small-btn {
    height: 28px;
    padding: 0 10px;
    font-size: 13px;
  }

  .proxy-result.ok {
    color: var(--success);
  }

  .row.check {
    justify-content: flex-start;
    gap: 8px;
  }

  .row select {
    min-width: 180px;
  }

  .folder {
    max-width: 60%;
    min-width: 0;
    justify-content: flex-start;
  }

  /* RTL makes the ellipsis cut the start of a long path, keeping the folder name
     visible; <bdi> keeps the path itself left-to-right so "/" stays in place. */
  .path {
    min-width: 0;
    direction: rtl;
    text-align: left;
  }

  .small {
    font-size: 12px;
  }

  .template-warning {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 6px 8px;
    padding: 8px 10px;
    border-radius: 8px;
    color: var(--warning);
    background: var(--warning-soft);
  }

  .template-warning span {
    flex: 1;
    min-width: 12em;
  }

  .template-help summary {
    cursor: pointer;
    color: var(--accent);
    width: fit-content;
  }

  .template-help h4 {
    margin: 10px 0 4px;
    font-size: 12px;
  }

  .template-help dl {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr);
    gap: 4px 12px;
    margin: 0;
  }

  .template-help dd {
    margin: 0;
    color: var(--muted);
  }

  .template-help code {
    overflow-wrap: anywhere;
  }

  /* Fields in a sentence stay whole instead of breaking at "." or "(". */
  .field {
    white-space: nowrap;
  }

  .examples {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 2px;
  }

  .example {
    max-width: 100%;
    padding: 4px 8px;
    border: 1px solid transparent;
    border-radius: 6px;
    background: none;
    color: var(--text);
    text-align: left;
    font: inherit;
    cursor: pointer;
  }

  .example {
    display: grid;
    gap: 2px;
  }

  .preview bdi {
    color: var(--text);
    overflow-wrap: anywhere;
  }

  .example:hover {
    background: var(--surface-2);
  }

  .example.current {
    border-color: var(--accent);
    background: var(--accent-soft);
  }

  .template-help p {
    margin: 10px 0 6px;
    color: var(--muted);
  }

  .link {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 0;
    border: none;
    background: none;
    color: var(--accent);
    font: inherit;
    cursor: pointer;
  }

  .link:hover {
    text-decoration: underline;
  }

  .saved {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    opacity: 0;
    transition: opacity 0.2s;
    margin-top: -6px;
  }

  .saved.visible {
    opacity: 1;
  }

  .section-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  h2 {
    margin: 0;
    font-size: 15px;
  }

  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 10px;
  }

  li {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .buttons {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  .component {
    flex: 1;
    display: grid;
  }

  @media (max-width: 600px) {
    .row:not(.check) {
      flex-direction: column;
      align-items: stretch;
      gap: 6px;
    }

    .row select {
      min-width: 0;
    }

    .folder {
      max-width: 100%;
    }
  }
</style>
