/** Option a key moves to in a list of `count` options, or null for other keys.
 * Like a native select, the ends don't wrap around. */
export function listboxKey(
  count: number,
  index: number,
  key: string,
): number | null {
  switch (key) {
    case "ArrowDown":
      return Math.min(index + 1, count - 1);
    case "ArrowUp":
      return Math.max(index - 1, 0);
    case "Home":
      return 0;
    case "End":
      return count - 1;
    default:
      return null;
  }
}

/** Option whose label starts with typed `text`. A single letter moves on to the
 * next match, so pressing it again cycles through the options with that letter. */
export function typeahead(
  labels: readonly string[],
  from: number,
  text: string,
): number | null {
  const wanted = text.toLowerCase();
  const start = text.length === 1 ? from + 1 : from;
  for (let i = 0; i < labels.length; i++) {
    const at = (start + i) % labels.length;
    if (labels[at].toLowerCase().startsWith(wanted)) return at;
  }
  return null;
}
