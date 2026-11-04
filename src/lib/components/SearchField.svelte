<script lang="ts">
  import { t } from "$lib/state.svelte";
  import Icon from "./Icon.svelte";

  /** A search box with the app's own clear button instead of the browser's. */
  let {
    value = $bindable(),
    placeholder,
    enterkeyhint,
  }: {
    value: string;
    /** Also the field's accessible name. */
    placeholder: string;
    enterkeyhint?: "search";
  } = $props();

  let input: HTMLInputElement;
</script>

<div class="field">
  <Icon name="search" size={16} />
  <input
    bind:this={input}
    class="input"
    type="search"
    {placeholder}
    aria-label={placeholder}
    bind:value
    {enterkeyhint}
    spellcheck="false"
    autocomplete="off"
  />
  {#if value}
    <button
      type="button"
      class="clear"
      aria-label={t("common.clear")}
      data-tip={t("common.clear")}
      onclick={() => {
        value = "";
        input.focus();
      }}
    >
      <Icon name="close" size={14} />
    </button>
  {/if}
</div>

<style>
  .field {
    flex: 1;
    position: relative;
    display: flex;
    align-items: center;
    min-width: 0;
    color: var(--muted);
  }

  .field > :global(svg) {
    position: absolute;
    left: 10px;
    pointer-events: none;
  }

  input {
    width: 100%;
    height: 38px;
    padding: 0 34px;
  }

  input::-webkit-search-cancel-button {
    display: none;
  }

  .clear {
    position: absolute;
    right: 4px;
    display: grid;
    place-items: center;
    width: 30px;
    height: 30px;
    padding: 0;
    border: none;
    border-radius: 6px;
    background: none;
    color: var(--muted);
    cursor: pointer;
  }

  .clear:hover {
    background: var(--surface-2);
    color: var(--text);
  }

  @media (pointer: coarse) {
    input {
      height: 44px;
    }

    .clear {
      right: 0;
      width: 44px;
      height: 44px;
    }
  }
</style>
