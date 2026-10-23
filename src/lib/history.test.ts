import { describe, expect, it } from "vitest";
import { appendPage } from "./history";

describe("appendPage", () => {
  it("skips rows already shown", () => {
    // New downloads shift offset paging by one: the next page repeats the last row.
    const shown = [{ id: 5 }, { id: 4 }, { id: 3 }];
    expect(appendPage(shown, [{ id: 3 }, { id: 2 }]).map((r) => r.id)).toEqual([
      5, 4, 3, 2,
    ]);
    expect(appendPage([], [{ id: 1 }])).toEqual([{ id: 1 }]);
  });
});
