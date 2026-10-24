/** The option a key selects in a radio group (ARIA pattern), or null for other keys. */
export function radioKey<T>(
  options: readonly T[],
  current: T,
  key: string,
): T | null {
  const i = options.indexOf(current);
  const n = options.length;
  switch (key) {
    case "ArrowRight":
    case "ArrowDown":
      return options[(i + 1) % n];
    case "ArrowLeft":
    case "ArrowUp":
      return options[(i - 1 + n) % n];
    case "Home":
      return options[0];
    case "End":
      return options[n - 1];
    default:
      return null;
  }
}
