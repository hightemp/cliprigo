<script lang="ts">
  import { onMount } from "svelte";
  import { api, onComponentProgress } from "$lib/api";
  import { errorMessage } from "$lib/errors";
  import { formatBytes } from "$lib/format";
  import { componentLabel, isAndroid } from "$lib/platform";
  import { app, copyDetails, t } from "$lib/state.svelte";
  import type { ComponentName, ComponentProgress } from "$lib/types";
  import Icon from "./Icon.svelte";

  let { ondone }: { ondone: () => void } = $props();

  type Row = {
    name: ComponentName;
    state: "waiting" | "working" | "done" | "error";
    progress: ComponentProgress | null;
    error: string | null;
    raw: unknown;
  };

  let rows = $state<Row[]>([]);
  let running = $state(false);

  async function installMissing() {
    running = true;
    for (const row of rows) {
      if (row.state === "done") continue;
      row.state = "working";
      row.error = null;
      try {
        await api.componentsInstall(row.name);
        row.state = "done";
      } catch (err) {
        row.state = "error";
        row.error = errorMessage(err, t);
        row.raw = err;
        // Later components would most likely fail the same way (network, rate limit).
        break;
      }
    }
    running = false;
    if (rows.every((r) => r.state === "done")) ondone();
  }

  function label(row: Row): string {
    const name = t(`components.${componentLabel(row.name)}`);
    if (row.state === "done") return t("setup.installed");
    if (row.state === "error") return t("setup.failed", { name });
    if (row.state === "waiting") return t("setup.waiting");
    const stage = row.progress?.stage ?? "download";
    const key = {
      download: "downloading",
      verify: "verifying",
      install: "installing",
    }[stage];
    return t(`setup.${key}`, { name });
  }

  function percent(row: Row): number | null {
    const p = row.progress;
    if (row.state === "done") return 100;
    if (!p || p.stage !== "download" || !p.total) return null;
    return (p.downloaded / p.total) * 100;
  }

  let statusError = $state<string | null>(null);

  async function loadStatus() {
    statusError = null;
    try {
      const status = await api.componentsStatus();
      rows = status.map((s) => ({
        name: s.component,
        state: s.installed ? "done" : "waiting",
        progress: null,
        error: null,
        raw: null,
      }));
      installMissing();
    } catch (err) {
      statusError = errorMessage(err, t);
    }
  }

  onMount(() => {
    const unlisten = onComponentProgress((p) => {
      const row = rows.find((r) => r.name === p.component);
      if (row) row.progress = p;
    });
    loadStatus();
    return () => unlisten.then((u) => u());
  });
</script>

<div class="setup">
  <div class="card panel">
    <h1>{t("setup.title")}</h1>
    <p class="muted">
      {t(isAndroid ? "setup.descriptionAndroid" : "setup.description")}
    </p>

    <ul>
      {#each rows as row (row.name)}
        {@const pct = percent(row)}
        <li>
          <div class="line">
            <strong>{t(`components.${componentLabel(row.name)}`)}</strong>
            <span class:error={row.state === "error"} class="muted status">
              {#if row.state === "done"}<Icon name="check" size={16} />{/if}
              {label(row)}
            </span>
          </div>
          {#if row.state === "working"}
            <div
              class="progress"
              class:indeterminate={pct === null}
              role="progressbar"
              aria-valuemin={0}
              aria-valuemax={100}
              aria-valuenow={pct ?? undefined}
            >
              <div style:width="{pct ?? 0}%"></div>
            </div>
            {#if row.progress?.stage === "download" && row.progress.downloaded > 0}
              <div class="muted small">
                {formatBytes(row.progress.downloaded, app.lang)}
                {#if row.progress.total}/ {formatBytes(
                    row.progress.total,
                    app.lang,
                  )}{/if}
              </div>
            {/if}
          {/if}
          {#if row.error}
            <div class="error small">{row.error}</div>
            <button
              class="btn copy"
              onclick={() => copyDetails(row.raw, `install ${row.name}`)}
            >
              {t("errors.copyDetails")}
            </button>
          {/if}
        </li>
      {/each}
    </ul>

    {#if statusError}
      <p class="error small">{statusError}</p>
      <button class="btn primary" onclick={loadStatus}>
        <Icon name="retry" size={16} />
        {t("common.retry")}
      </button>
    {/if}

    {#if !running && rows.some((r) => r.state === "error")}
      <button class="btn primary" onclick={installMissing}>
        <Icon name="retry" size={16} />
        {t("common.retry")}
      </button>
    {/if}
  </div>
</div>

<style>
  .setup {
    min-height: 100vh;
    display: grid;
    place-items: center;
    padding: 24px;
  }

  .panel {
    width: min(480px, 100%);
    padding: 28px;
    box-shadow: var(--shadow);
  }

  h1 {
    margin: 0 0 6px;
    font-size: 20px;
  }

  ul {
    list-style: none;
    padding: 0;
    margin: 20px 0;
    display: grid;
    gap: 16px;
  }

  .line {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 12px;
    margin-bottom: 6px;
  }

  .status {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    text-align: right;
  }

  .small {
    font-size: 12px;
    margin-top: 4px;
  }

  .error {
    color: var(--danger);
    overflow-wrap: anywhere;
  }

  .copy {
    height: 28px;
    margin-top: 6px;
    font-size: 13px;
  }
</style>
