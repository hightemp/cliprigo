import { describe, expect, it } from "vitest";
import { mergeSaved } from "./settings";

describe("mergeSaved", () => {
  const base = {
    proxy: "",
    concurrency: 2,
    filenameTemplate: "%(title)s.%(ext)s",
  };

  it("takes what the backend stored for the fields that were sent", () => {
    // The backend normalizes: trims the proxy.
    const sent = { ...base, proxy: " http://p:1 " };
    const saved = { ...base, proxy: "http://p:1" };
    expect(mergeSaved(sent, saved, sent)).toEqual(saved);
  });

  it("keeps edits made while the save was in flight", () => {
    const sent = { ...base, concurrency: 3 };
    const saved = { ...base, concurrency: 3 };
    const current = { ...sent, filenameTemplate: "%(id)s.%(ext)s" };
    expect(mergeSaved(sent, saved, current)).toEqual({
      ...saved,
      filenameTemplate: "%(id)s.%(ext)s",
    });
  });
});
