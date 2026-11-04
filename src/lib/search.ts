import type { SearchFeature, SearchFilters, SearchRecord } from "./types";

export const NO_FILTERS: SearchFilters = {
  uploadDate: null,
  length: null,
  features: [],
  sort: "relevance",
};

/** How many filters differ from a plain search, for the button's badge. */
export function activeFilters(f: SearchFilters): number {
  return (
    Number(f.uploadDate !== null) +
    Number(f.length !== null) +
    f.features.length +
    Number(f.sort !== "relevance")
  );
}

export function toggleFeature(
  features: readonly SearchFeature[],
  feature: SearchFeature,
): SearchFeature[] {
  return features.includes(feature)
    ? features.filter((f) => f !== feature)
    : [...features, feature];
}

/** Same as query_key in src-tauri/src/db/search_history.rs. */
function queryKey(query: string): string {
  return query.trim().split(/\s+/).join(" ").toLocaleLowerCase();
}

/** Past searches to offer under the search field: the latest ones for an empty
 * field, else those containing the text (but not the very query typed). */
export function suggestQueries(
  history: readonly SearchRecord[],
  text: string,
  limit = 8,
): SearchRecord[] {
  const typed = queryKey(text);
  const matching = typed
    ? history.filter((r) => {
        const key = queryKey(r.query);
        return key !== typed && key.includes(typed);
      })
    : history;
  return matching.slice(0, limit);
}

/** The filters that narrowed a search, as shown in its labels. */
export function filterLabels(
  f: SearchFilters,
  t: (key: string) => string,
): string[] {
  return [
    f.uploadDate && t(`search.filters.date.${f.uploadDate}`),
    f.length && t(`search.filters.lengths.${f.length}`),
    ...f.features.map((x) => t(`search.filters.features.${x}`)),
    f.sort !== "relevance" && t(`search.filters.sorts.${f.sort}`),
  ].filter((s): s is string => Boolean(s));
}
