import { describe, expect, it } from "vitest";
import { translate } from "./i18n";
import {
  NO_FILTERS,
  activeFilters,
  filterLabels,
  suggestQueries,
  toggleFeature,
} from "./search";
import type { SearchRecord } from "./types";

describe("search filters", () => {
  it("counts what narrows the results", () => {
    expect(activeFilters(NO_FILTERS)).toBe(0);
    expect(
      activeFilters({
        uploadDate: "week",
        length: null,
        features: ["4k", "hdr"],
        sort: "views",
      }),
    ).toBe(4);
  });

  it("toggles a feature", () => {
    expect(toggleFeature(["hd"], "4k")).toEqual(["hd", "4k"]);
    expect(toggleFeature(["hd", "4k"], "hd")).toEqual(["4k"]);
  });
});

function record(id: number, query: string): SearchRecord {
  return {
    id,
    query,
    filters: NO_FILTERS,
    searchedAt: 1_700_000_000,
    resultCount: 20,
  };
}

describe("search suggestions", () => {
  const history = [
    record(4, "Котики смешные"),
    record(3, "lofi beats"),
    record(2, "КОТИКИ"),
    record(1, "Rust tutorial"),
  ];

  it("lists the latest searches for an empty field", () => {
    expect(suggestQueries(history, "  ", 3).map((r) => r.id)).toEqual([
      4, 3, 2,
    ]);
  });

  it("keeps searches containing the text, in any case", () => {
    expect(suggestQueries(history, "котик").map((r) => r.id)).toEqual([4, 2]);
    expect(suggestQueries(history, "TUT").map((r) => r.id)).toEqual([1]);
  });

  it("leaves out the search already typed", () => {
    expect(suggestQueries(history, " котики ").map((r) => r.id)).toEqual([4]);
  });
});

describe("filter labels", () => {
  const t = (key: string) => translate("en", key);

  it("names only what differs from a plain search", () => {
    expect(filterLabels(NO_FILTERS, t)).toEqual([]);
    expect(
      filterLabels(
        {
          uploadDate: "week",
          length: "long",
          features: ["4k", "subtitles"],
          sort: "views",
        },
        t,
      ),
    ).toEqual([
      "This week",
      "Over 20 minutes",
      "4K",
      "Subtitles",
      "Popularity",
    ]);
  });
});
