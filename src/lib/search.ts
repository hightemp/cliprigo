import type { SearchFeature, SearchFilters } from "./types";

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
