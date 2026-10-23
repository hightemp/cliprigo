/** Appends the next history page, skipping rows already shown: new downloads are
 * inserted at the top, so offset paging repeats rows and keyed lists then crash. */
export function appendPage<T extends { id: number }>(
  shown: T[],
  page: T[],
): T[] {
  const ids = new Set(shown.map((r) => r.id));
  return [...shown, ...page.filter((r) => !ids.has(r.id))];
}
