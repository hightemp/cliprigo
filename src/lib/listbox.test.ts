import { describe, expect, it } from "vitest";
import { listboxKey, typeahead } from "./listbox";

describe("listboxKey", () => {
  it("moves without wrapping, like a native select", () => {
    expect(listboxKey(4, 1, "ArrowDown")).toBe(2);
    expect(listboxKey(4, 3, "ArrowDown")).toBe(3);
    expect(listboxKey(4, 1, "ArrowUp")).toBe(0);
    expect(listboxKey(4, 0, "ArrowUp")).toBe(0);
    expect(listboxKey(4, 2, "Home")).toBe(0);
    expect(listboxKey(4, 1, "End")).toBe(3);
  });

  it("ignores other keys", () => {
    expect(listboxKey(4, 1, "Enter")).toBeNull();
    expect(listboxKey(4, 1, "a")).toBeNull();
  });
});

describe("typeahead", () => {
  const labels = ["Auto", "H.264", "AV1", "VP9"];

  it("finds the next option starting with the typed text", () => {
    expect(typeahead(labels, 0, "a")).toBe(2);
    expect(typeahead(labels, 2, "a")).toBe(0);
    expect(typeahead(labels, 0, "av")).toBe(2);
    expect(typeahead(labels, 0, "vp")).toBe(3);
  });

  it("is null when nothing matches", () => {
    expect(typeahead(labels, 0, "x")).toBeNull();
  });
});
