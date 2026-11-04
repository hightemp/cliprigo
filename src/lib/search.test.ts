import { describe, expect, it } from "vitest";
import { NO_FILTERS, activeFilters, toggleFeature } from "./search";

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
