/** Appends the next page, skipping rows already shown: new downloads are inserted
 * at the top of the history and search results shift, so offset paging repeats rows
 * and keyed lists then crash. */
export function appendPage<T extends { id: number | string }>(
  shown: T[],
  page: T[],
): T[] {
  const ids = new Set(shown.map((r) => r.id));
  return [...shown, ...page.filter((r) => !ids.has(r.id))];
}
