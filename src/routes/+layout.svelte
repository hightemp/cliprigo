<script lang="ts">
  import "../app.css";
  import { onMount } from "svelte";
  import Tooltip from "$lib/components/Tooltip.svelte";
  import { browserShortcut } from "$lib/platform";

  let { children } = $props();

  // A desktop app, not a web page: no browser menu ("Back", "Reload") outside
  // text fields, and no reloading, which would lose the state of the window.
  onMount(() => {
    const menu = (e: MouseEvent) => {
      const field = (e.target as Element | null)?.closest?.(
        "input, textarea, [contenteditable]",
      );
      if (!field) e.preventDefault();
    };
    const keys = (e: KeyboardEvent) => {
      if (import.meta.env.PROD && browserShortcut(e)) e.preventDefault();
    };
    document.addEventListener("contextmenu", menu);
    document.addEventListener("keydown", keys);
    return () => {
      document.removeEventListener("contextmenu", menu);
      document.removeEventListener("keydown", keys);
    };
  });
</script>

{@render children()}
<Tooltip />
