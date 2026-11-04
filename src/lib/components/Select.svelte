<script lang="ts" generics="T extends string | number | null">
  import { listboxKey, typeahead } from "$lib/listbox";

  /** A drop-down list drawn by the app instead of the browser's own select. */
  let {
    value = $bindable(),
    options,
    id,
    label,
    onchange,
  }: {
    value: T;
    options: readonly { value: T; label: string }[];
    /** For a `<label for>`; a wrapping `<label>` works too. */
    id?: string;
    /** Accessible name when there is no visible label. */
    label?: string;
    onchange?: (value: T) => void;
  } = $props();

  const listId = $props.id();
  let button: HTMLButtonElement;
  let list: HTMLUListElement;
  let open = $state(false);
  let active = $state(0);
  let typed = "";
  let typedTimer: ReturnType<typeof setTimeout> | undefined;

  const selected = $derived(options.findIndex((o) => o.value === value));
  const current = $derived(options[selected]?.label ?? "");

  function show() {
    if (open) return;
    active = Math.max(selected, 0);
    open = true;
    place();
    list.showPopover?.();
    // The list may be wider than the button: keep it inside the window.
    placeLeft();
    reveal(active);
  }

  /** Under the button, or above it when the window ends first. */
  function place() {
    const r = button.getBoundingClientRect();
    const below = innerHeight - r.bottom;
    list.style.minWidth = `${r.width}px`;
    if (below < 200 && r.top > below) {
      list.style.top = "";
      list.style.bottom = `${innerHeight - r.top + 4}px`;
      list.style.maxHeight = `${Math.min(280, r.top - 12)}px`;
    } else {
      list.style.bottom = "";
      list.style.top = `${r.bottom + 4}px`;
      list.style.maxHeight = `${Math.min(280, below - 12)}px`;
    }
  }

  function placeLeft() {
    const r = button.getBoundingClientRect();
    list.style.left = `${Math.max(8, Math.min(r.left, innerWidth - list.offsetWidth - 8))}px`;
  }

  // The list is fixed in the window: it follows the button when the page (or a
  // dialog) scrolls or the window resizes. Scrolling the list itself moves nothing.
  $effect(() => {
    if (!open) return;
    let frame = 0;
    const follow = (e: Event) => {
      if (e.target === list) return;
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(() => {
        // Scrolled out of sight: close, as a native select does.
        const r = button.getBoundingClientRect();
        if (r.bottom < 0 || r.top > innerHeight) {
          hide();
          return;
        }
        place();
        placeLeft();
      });
    };
    addEventListener("scroll", follow, { capture: true, passive: true });
    addEventListener("resize", follow);
    return () => {
      cancelAnimationFrame(frame);
      removeEventListener("scroll", follow, { capture: true });
      removeEventListener("resize", follow);
    };
  });

  /** Scrolls the list (only the list, not the page) to show option `i`. */
  function reveal(i: number) {
    const li = list.children[i] as HTMLElement | undefined;
    if (!li) return;
    if (li.offsetTop < list.scrollTop) list.scrollTop = li.offsetTop;
    else if (
      li.offsetTop + li.offsetHeight >
      list.scrollTop + list.clientHeight
    )
      list.scrollTop = li.offsetTop + li.offsetHeight - list.clientHeight;
  }

  function hide() {
    if (!open) return;
    open = false;
    list.hidePopover?.();
  }

  function pick(index: number) {
    const option = options[index];
    hide();
    button.focus();
    if (!option || option.value === value) return;
    value = option.value;
    onchange?.(option.value);
  }

  function onkeydown(e: KeyboardEvent) {
    if (!open) {
      if (["ArrowDown", "ArrowUp", "Enter", " "].includes(e.key)) {
        e.preventDefault();
        show();
      }
      return;
    }
    const moved = listboxKey(options.length, active, e.key);
    if (moved !== null) {
      e.preventDefault();
      active = moved;
      reveal(active);
    } else if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      pick(active);
    } else if (e.key === "Escape") {
      e.preventDefault();
      // Inside a dialog, Esc closes the list, not the dialog.
      e.stopPropagation();
      hide();
    } else if (e.key === "Tab") {
      hide();
    } else if (e.key.length === 1 && !e.ctrlKey && !e.metaKey && !e.altKey) {
      clearTimeout(typedTimer);
      typed += e.key;
      typedTimer = setTimeout(() => (typed = ""), 600);
      const found = typeahead(
        options.map((o) => o.label),
        active,
        typed,
      );
      if (found !== null) active = found;
    }
  }
</script>

<!-- One box outside: the open list must not take part in the parent's layout. -->
<span class="wrap">
  <button
    bind:this={button}
    {id}
    type="button"
    class="input select"
    role="combobox"
    aria-label={label}
    aria-haspopup="listbox"
    aria-expanded={open}
    aria-controls={listId}
    aria-activedescendant={open ? `${listId}-${active}` : undefined}
    data-value={value}
    onclick={() => (open ? hide() : show())}
    {onkeydown}
  >
    <span class="current">{current}</span>
    <svg width="14" height="14" viewBox="0 0 24 24" aria-hidden="true">
      <path
        d="M6 9l6 6 6-6"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
        stroke-linecap="round"
        stroke-linejoin="round"
      />
    </svg>
  </button>

  <!-- A popover is in the top layer: not cut off by scrolling parents or dialogs.
     "auto" closes it on a click outside and on Esc. -->
  <ul
    bind:this={list}
    id={listId}
    role="listbox"
    popover="auto"
    class:open
    ontoggle={(e) => {
      if (e.newState === "closed") open = false;
    }}
  >
    {#each options as option, i (option.value)}
      <!-- Keys go to the combobox button (aria-activedescendant), as ARIA describes. -->
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <li
        id={`${listId}-${i}`}
        role="option"
        aria-selected={i === selected}
        class:active={i === active}
        data-value={option.value}
        onpointermove={() => (active = i)}
        onpointerdown={(e) => e.preventDefault()}
        onclick={(e) => {
          // Inside a <label>, a click would also reach the button and reopen.
          e.preventDefault();
          pick(i);
        }}
      >
        {option.label}
        {#if i === selected}
          <svg width="14" height="14" viewBox="0 0 24 24" aria-hidden="true">
            <path
              d="M5 12l5 5L19 8"
              fill="none"
              stroke="currentColor"
              stroke-width="2.2"
              stroke-linecap="round"
              stroke-linejoin="round"
            />
          </svg>
        {/if}
      </li>
    {/each}
  </ul>
</span>

<style>
  .wrap {
    display: flex;
    min-width: 0;
  }

  .select {
    flex: 1;
    display: inline-flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    min-width: 0;
    color: var(--text);
    text-align: left;
    cursor: pointer;
  }

  .select[aria-expanded="true"] {
    border-color: var(--accent);
  }

  .select svg {
    flex: none;
    color: var(--muted);
  }

  .current {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  ul {
    position: fixed;
    inset: auto;
    margin: 0;
    padding: 4px;
    list-style: none;
    overflow-y: auto;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--surface);
    color: var(--text);
    box-shadow: var(--shadow);
  }

  /* Also hides it where popovers are unsupported (a plain fixed element there). */
  ul:not(.open) {
    display: none;
  }

  li {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    min-height: 32px;
    padding: 0 10px;
    border-radius: 6px;
    white-space: nowrap;
    cursor: pointer;
  }

  li.active {
    background: var(--surface-2);
  }

  li[aria-selected="true"] {
    color: var(--accent);
    font-weight: 600;
  }

  @media (pointer: coarse) {
    li {
      min-height: 44px;
    }
  }
</style>
