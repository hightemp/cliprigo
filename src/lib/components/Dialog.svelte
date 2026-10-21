<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    title,
    onclose,
    children,
    actions,
  }: {
    title: string;
    onclose: () => void;
    children: Snippet;
    actions: Snippet;
  } = $props();

  let dialog: HTMLDialogElement;

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
  aria-labelledby="dialog-title"
>
  <h2 id="dialog-title">{title}</h2>
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

  dialog::backdrop {
    background: rgb(0 0 0 / 0.4);
  }

  h2 {
    margin: 0 0 8px;
    font-size: 16px;
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
