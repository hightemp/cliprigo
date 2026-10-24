/**
 * The settings form after a save returns: what the backend stored (it normalizes
 * values), except fields the user changed while the request was in flight.
 */
export function mergeSaved<T extends object>(sent: T, saved: T, current: T): T {
  const merged = { ...saved };
  for (const key of Object.keys(saved) as (keyof T)[]) {
    if (current[key] !== sent[key]) merged[key] = current[key];
  }
  return merged;
}
