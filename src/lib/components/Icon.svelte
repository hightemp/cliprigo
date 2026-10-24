<script lang="ts" module>
  // Simple 24×24 stroke icons drawn for Cliprigo.
  const PATHS = {
    download: "M12 4v11M7 10l5 5 5-5M5 20h14",
    history: "M12 7v5l3 2M3 12a9 9 0 1 0 2.6-6.4M3 4v4h4",
    settings: "M4 7h9M17 7h3M4 17h3M11 17h9M15 5v4M9 15v4M4 12h14M20 12h0",
    paste: "M9 4h6v3H9zM7 5H5v15h14V5h-2M9 12h6M9 16h4",
    folder: "M3 6h6l2 2h10v11H3z",
    close: "M6 6l12 12M18 6L6 18",
    retry: "M20 12a8 8 0 1 1-2.3-5.6M20 4v5h-5",
    trash: "M4 7h16M10 11v6M14 11v6M9 7V4h6v3M6 7l1 13h10l1-13",
    link: "M10 14a4 4 0 0 0 5.7 0l3-3a4 4 0 0 0-5.7-5.7l-1 1M14 10a4 4 0 0 0-5.7 0l-3 3a4 4 0 0 0 5.7 5.7l1-1",
    external: "M14 4h6v6M20 4l-9 9M18 14v6H4V6h6",
    play: "M8 5l11 7-11 7z",
    search: "M11 18a7 7 0 1 0 0-14 7 7 0 0 0 0 14zM20 20l-4-4",
    check: "M5 12l5 5L19 8",
    warning: "M12 4l9 16H3zM12 10v4M12 17h0",
    video: "M4 6h12v12H4zM16 10l4-2v8l-4-2",
    music:
      "M9 18V6l10-2v12M9 18a3 3 0 1 1-6 0 3 3 0 0 1 6 0zM19 16a3 3 0 1 1-6 0 3 3 0 0 1 6 0z",
  } as const;

  export type IconName = keyof typeof PATHS;
</script>

<script lang="ts">
  let { name, size = 18 }: { name: IconName; size?: number } = $props();
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="1.8"
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
>
  <path d={PATHS[name]} />
</svg>
