<script lang="ts">
  import { onMount } from "svelte";

  /** Tooltips for every element with `data-tip`, drawn by the app instead of the
   * browser's `title`. Shown on mouse hover and keyboard focus, not on touch. */
  const DELAY = 500;

  let tip: HTMLDivElement;
  let text = $state("");
  let target: HTMLElement | null = null;
  let timer: ReturnType<typeof setTimeout> | undefined;

  function place(of: HTMLElement) {
    const r = of.getBoundingClientRect();
    const t = tip.getBoundingClientRect();
    const left = Math.min(
      Math.max(8, r.left + r.width / 2 - t.width / 2),
      innerWidth - t.width - 8,
    );
    const below = r.bottom + 6;
    const top =
      below + t.height < innerHeight - 8 ? below : r.top - t.height - 6;
    tip.style.left = `${left}px`;
    tip.style.top = `${Math.max(8, top)}px`;
  }

  function show(el: HTMLElement) {
    clearTimeout(timer);
    target = el;
    timer = setTimeout(() => {
      const value = el.dataset.tip;
      if (!value || !el.isConnected || target !== el) return;
      text = value;
      tip.showPopover?.();
      tip.classList.add("open");
      // The text is in place now, so the size is known.
      requestAnimationFrame(() => place(el));
    }, DELAY);
  }

  function hide() {
    clearTimeout(timer);
    target = null;
    tip.classList.remove("open");
    tip.hidePopover?.();
  }

  const tipped = (e: Event) =>
    (e.target as Element | null)?.closest?.<HTMLElement>("[data-tip]") ?? null;

  onMount(() => {
    const over = (e: PointerEvent) => {
      if (e.pointerType !== "mouse") return;
      const el = tipped(e);
      if (el && el !== target) show(el);
      else if (!el) hide();
    };
    const focus = (e: FocusEvent) => {
      const el = tipped(e);
      if (el?.matches(":focus-visible")) show(el);
    };
    const events: [string, EventListener][] = [
      ["pointerover", over as EventListener],
      ["focusin", focus as EventListener],
      ["focusout", hide],
      ["pointerdown", hide],
      ["keydown", hide],
      ["scroll", hide],
    ];
    for (const [name, fn] of events) document.addEventListener(name, fn, true);
    return () => {
      for (const [name, fn] of events)
        document.removeEventListener(name, fn, true);
    };
  });
</script>

<div bind:this={tip} class="tip" role="tooltip" popover="manual">{text}</div>

<style>
  .tip {
    position: fixed;
    inset: auto;
    margin: 0;
    max-width: min(420px, calc(100vw - 16px));
    padding: 6px 10px;
    border: none;
    border-radius: 6px;
    background: var(--text);
    color: var(--bg);
    font-size: 12px;
    line-height: 1.4;
    overflow-wrap: anywhere;
    pointer-events: none;
    box-shadow: var(--shadow);
  }

  .tip:not(:global(.open)) {
    display: none;
  }
</style>
