<script lang="ts">
  import { listboxKey } from "$lib/listbox";
  import { t } from "$lib/state.svelte";
  import Icon from "./Icon.svelte";

  /** A search box with the app's own clear button instead of the browser's.
   * With `suggestions` it is a combobox offering them under the field. */
  let {
    value = $bindable(),
    placeholder,
    enterkeyhint,
    suggestions,
    onpick,
    onremove,
  }: {
    value: string;
    /** Also the field's accessible name. */
    placeholder: string;
    enterkeyhint?: "search";
    suggestions?: readonly string[];
    /** A suggestion was chosen; the field already shows it. */
    onpick?: (text: string) => void;
    /** Shift+Delete or the cross on a suggestion. */
    onremove?: (text: string) => void;
  } = $props();

  const listId = $props.id();
  let input: HTMLInputElement;
  let open = $state(false);
  let active = $state(-1);

  const shown = $derived(open && suggestions && suggestions.length > 0);

  // The list changes as one types: nothing stays picked by the arrows.
  $effect(() => {
    void suggestions;
    active = -1;
  });

  function pick(text: string) {
    value = text;
    open = false;
    onpick?.(text);
  }

  function onkeydown(e: KeyboardEvent) {
    if (!suggestions) return;
    const count = suggestions.length;
    if (e.key === "Escape" && shown) {
      e.preventDefault();
      open = false;
    } else if (e.key === "Enter") {
      if (shown && active >= 0) {
        e.preventDefault();
        pick(suggestions[active]);
      } else {
        // The form searches the typed text.
        open = false;
      }
    } else if (e.key === "Delete" && e.shiftKey && shown && active >= 0) {
      e.preventDefault();
      const removed = suggestions[active];
      active = Math.min(active, count - 2);
      onremove?.(removed);
    } else if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      if (!count) return;
      e.preventDefault();
      open = true;
      active = listboxKey(count, active, e.key) ?? active;
    }
  }
</script>

<div class="field">
  <Icon name="search" size={16} />
  <input
    bind:this={input}
    class="input"
    type="search"
    {placeholder}
    aria-label={placeholder}
    role={suggestions ? "combobox" : undefined}
    aria-autocomplete={suggestions ? "list" : undefined}
    aria-expanded={suggestions ? Boolean(shown) : undefined}
    aria-controls={suggestions ? listId : undefined}
    aria-activedescendant={shown && active >= 0
      ? `${listId}-${active}`
      : undefined}
    bind:value
    {enterkeyhint}
    spellcheck="false"
    autocomplete="off"
    onfocus={() => (open = true)}
    onclick={() => (open = true)}
    oninput={() => (open = true)}
    onblur={() => (open = false)}
    {onkeydown}
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
  {#if suggestions}
    <!-- Keys stay in the field (aria-activedescendant); pointer presses must not
       take the focus from it, or the list would close before the click. -->
    <ul
      id={listId}
      role="listbox"
      aria-label={t("search.history.recent")}
      hidden={!shown}
      onpointerdown={(e) => e.preventDefault()}
    >
      {#each suggestions as text, i (text)}
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <li
          id={`${listId}-${i}`}
          role="option"
          aria-selected={i === active}
          class:active={i === active}
          onpointermove={() => (active = i)}
          onclick={() => pick(text)}
        >
          <Icon name="history" size={14} />
          <span class="text">{text}</span>
          {#if onremove}
            <!-- A list option can't hold a button; Shift+Delete does the same. -->
            <span
              class="remove"
              aria-hidden="true"
              data-tip={t("search.history.remove")}
              onclick={(e) => {
                e.stopPropagation();
                onremove(text);
              }}
            >
              <Icon name="close" size={14} />
            </span>
          {/if}
        </li>
      {/each}
    </ul>
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

  ul {
    position: absolute;
    top: calc(100% + 4px);
    left: 0;
    right: 0;
    z-index: 20;
    margin: 0;
    padding: 4px;
    list-style: none;
    max-height: 320px;
    overflow-y: auto;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--surface);
    color: var(--text);
    box-shadow: var(--shadow);
  }

  li {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 34px;
    padding: 0 4px 0 10px;
    border-radius: 6px;
    cursor: pointer;
  }

  li > :global(svg) {
    flex: none;
    color: var(--muted);
  }

  li.active {
    background: var(--surface-2);
  }

  .text {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .remove {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    border-radius: 6px;
    color: var(--muted);
  }

  .remove:hover {
    background: var(--border);
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

    li {
      min-height: 44px;
    }

    .remove {
      width: 44px;
      height: 44px;
    }
  }
</style>
