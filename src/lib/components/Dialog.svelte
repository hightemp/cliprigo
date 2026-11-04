<script lang="ts">
  import type { Snippet } from "svelte";
  import { t } from "$lib/state.svelte";
  import Icon from "./Icon.svelte";

  let {
    title,
    onclose,
    children,
    actions,
    wide = false,
  }: {
    title: string;
    onclose: () => void;
    children: Snippet;
    actions: Snippet;
    /** Room for a video player. */
    wide?: boolean;
  } = $props();

  let dialog: HTMLDialogElement;
  const titleId = $props.id();

  $effect(() => {
    dialog.showModal();
  });
</script>

<!-- The native dialog gives focus trapping and Esc handling for free. -->
<dialog
  bind:this={dialog}
  oncancel={(e) => {
    e.preventDefault();
    onclose();
  }}
  aria-labelledby={titleId}
  class:wide
>
  <header>
    <h2 id={titleId}>{title}</h2>
    <button
      type="button"
      class="icon-btn close"
      aria-label={t("common.closeWindow")}
      data-tip={t("common.close")}
      onclick={onclose}
    >
      <Icon name="close" size={16} />
    </button>
  </header>
  <div class="body">{@render children()}</div>
  <div class="actions">{@render actions()}</div>
</dialog>

<style>
  dialog {
    width: min(440px, calc(100vw - 32px));
    padding: 20px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
    color: var(--text);
    box-shadow: var(--shadow);
  }

  dialog.wide {
    width: min(960px, calc(100vw - 32px));
  }

  dialog::backdrop {
    background: rgb(0 0 0 / 0.4);
  }

  header {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    margin: 0 0 8px;
  }

  h2 {
    flex: 1;
    min-width: 0;
    margin: 0;
    padding-top: 4px;
    font-size: 16px;
  }

  /* Pulled into the corner, so it doesn't push the title down. */
  .close {
    flex: none;
    margin: -6px -8px 0 0;
  }

  .body {
    color: var(--muted);
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    flex-wrap: wrap;
    gap: 8px;
    margin-top: 20px;
  }
</style>
