<script lang="ts">
  import { onMount } from "svelte";
  import {
    api,
    onCloseRequested,
    onComponentUpdated,
    onQueueEvent,
  } from "$lib/api";
  import { errorMessage } from "$lib/errors";
  import { applyEvent } from "$lib/queue";
  import { app, applySettings, t, toast } from "$lib/state.svelte";
  import { ACTIVE_STATUSES } from "$lib/types";
  import Dialog from "$lib/components/Dialog.svelte";
  import DownloadTab from "$lib/components/DownloadTab.svelte";
  import HistoryTab from "$lib/components/HistoryTab.svelte";
  import Icon, { type IconName } from "$lib/components/Icon.svelte";
  import SettingsTab from "$lib/components/SettingsTab.svelte";
  import Setup from "$lib/components/Setup.svelte";
  import { isAndroid } from "$lib/platform";

  type Tab = "download" | "history" | "settings";
  const TABS: { id: Tab; icon: IconName }[] = [
    { id: "download", icon: "download" },
    { id: "history", icon: "history" },
    { id: "settings", icon: "settings" },
  ];

  let phase = $state<"loading" | "setup" | "ready" | "error">("loading");
  let startupError = $state("");
  let tab = $state<Tab>("download");
  let confirmClose = $state(false);
  let quitting = $state(false);

  // Android freezes a backgrounded app with its downloads unless a notification
  // says it is working. $derived: progress events must not restart it.
  const active = $derived(app.queue.length);
  $effect(() => {
    if (!isAndroid) return;
    const text = active > 0 ? t("app.keepAlive", { count: active }) : null;
    api.keepAlive(text).catch(() => {});
  });

  async function start() {
    phase = "loading";
    try {
      applySettings(await api.settingsGet());
      const status = await api.componentsStatus();
      app.queue = await api.queueList();
      phase = status.every((s) => s.installed) ? "ready" : "setup";
    } catch (err) {
      startupError = errorMessage(err, t);
      phase = "error";
    }
  }

  async function quit() {
    quitting = true;
    await api.appQuit();
  }

  onMount(() => {
    const unlisten = [
      onQueueEvent((event) => {
        app.queue = applyEvent(app.queue, event);
        if (
          event.type === "status" &&
          !ACTIVE_STATUSES.includes(event.record.status)
        ) {
          app.historyVersion++;
        }
      }),
      onCloseRequested(() => (confirmClose = true)),
      onComponentUpdated((u) =>
        toast(t("app.ytdlpUpdated", { version: u.version })),
      ),
    ];
    Promise.all(unlisten).finally(start);
    return () => unlisten.forEach((p) => p.then((u) => u()));
  });
</script>

{#if phase === "setup"}
  <Setup ondone={() => (phase = "ready")} />
{:else if phase === "error"}
  <div class="center">
    <p class="error">{startupError}</p>
    <button class="btn" onclick={start}>{t("common.retry")}</button>
  </div>
{:else if phase === "ready"}
  <div class="shell">
    <header>
      <div class="brand">Cliprigo</div>
      <div class="tabs" role="tablist">
        {#each TABS as item (item.id)}
          <button
            role="tab"
            aria-selected={tab === item.id}
            class:active={tab === item.id}
            onclick={() => (tab = item.id)}
          >
            <Icon name={item.icon} size={16} />
            {t(`tabs.${item.id}`)}
            {#if item.id === "download" && app.queue.length > 0}
              <span class="count">{app.queue.length}</span>
            {/if}
          </button>
        {/each}
      </div>
    </header>
    <main>
      <!-- Tabs stay mounted so a half-filled link or a scroll position survives switching. -->
      <div hidden={tab !== "download"}><DownloadTab /></div>
      <div hidden={tab !== "history"}><HistoryTab /></div>
      {#if tab === "settings"}<SettingsTab />{/if}
    </main>
  </div>
{/if}

{#if app.toast}
  <div class="toast" class:error={app.toast.error} role="status">
    {app.toast.text}
  </div>
{/if}

{#if confirmClose}
  <Dialog title={t("close.title")} onclose={() => (confirmClose = false)}>
    {t("close.text", { count: app.queue.length })}
    {#snippet actions()}
      <button
        class="btn"
        disabled={quitting}
        onclick={() => (confirmClose = false)}
      >
        {t("close.stay")}
      </button>
      <button class="btn danger" disabled={quitting} onclick={quit}
        >{t("close.quit")}</button
      >
    {/snippet}
  </Dialog>
{/if}

<style>
  .shell {
    min-height: 100vh;
    display: flex;
    flex-direction: column;
  }

  header {
    position: sticky;
    top: 0;
    z-index: 10;
    display: flex;
    align-items: center;
    gap: 24px;
    /* Android draws the app under the status bar. */
    padding: max(10px, env(safe-area-inset-top)) 20px 10px;
    background: var(--surface);
    border-bottom: 1px solid var(--border);
  }

  .brand {
    font-weight: 700;
    font-size: 16px;
    letter-spacing: -0.01em;
  }

  .tabs {
    display: flex;
    gap: 4px;
  }

  .tabs button {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 32px;
    padding: 0 12px;
    border: none;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--muted);
  }

  .tabs button:hover {
    background: var(--surface-2);
    color: var(--text);
  }

  .tabs button.active {
    background: var(--accent-soft);
    color: var(--accent);
    font-weight: 600;
  }

  .count {
    min-width: 18px;
    padding: 0 5px;
    border-radius: 9px;
    background: var(--accent);
    color: var(--on-accent);
    font-size: 11px;
    line-height: 18px;
    text-align: center;
  }

  main {
    flex: 1;
    width: 100%;
    max-width: 880px;
    margin: 0 auto;
    padding: 20px 20px max(20px, env(safe-area-inset-bottom));
  }

  @media (max-width: 600px) {
    header {
      padding: max(6px, env(safe-area-inset-top)) 8px 6px;
    }

    .brand {
      display: none;
    }

    .tabs {
      flex: 1;
    }

    .tabs button {
      flex: 1;
      justify-content: center;
      height: 40px;
      padding: 0 6px;
    }

    main {
      padding: 12px 12px max(12px, env(safe-area-inset-bottom));
    }
  }

  .center {
    min-height: 100vh;
    display: grid;
    place-content: center;
    justify-items: center;
    gap: 12px;
    padding: 24px;
  }

  .error {
    color: var(--danger);
  }

  .toast {
    position: fixed;
    left: 50%;
    bottom: max(20px, env(safe-area-inset-bottom));
    transform: translateX(-50%);
    max-width: min(560px, calc(100vw - 32px));
    padding: 10px 16px;
    border-radius: var(--radius-sm);
    background: var(--text);
    color: var(--bg);
    box-shadow: var(--shadow);
    z-index: 100;
  }

  .toast.error {
    background: var(--danger);
    color: #fff;
  }
</style>
