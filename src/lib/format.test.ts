import { describe, expect, it } from "vitest";
import {
  formatBytes,
  formatDuration,
  formatViews,
  pluralCount,
  formatEta,
  formatSpeed,
  qualityLabel,
} from "./format";

describe("formatBytes", () => {
  it("uses localized units and separators", () => {
    expect(formatBytes(0, "en")).toBe("0 B");
    expect(formatBytes(512, "en")).toBe("512 B");
    expect(formatBytes(1536, "en")).toBe("1.5 KB");
    expect(formatBytes(1_505_504, "en")).toBe("1.4 MB");
    expect(formatBytes(1_505_504, "ru")).toBe("1,4 МБ");
    expect(formatBytes(5 * 1024 ** 3, "ru")).toBe("5 ГБ");
  });

  it("handles missing values", () => {
    expect(formatBytes(null, "en")).toBe("");
    expect(formatBytes(undefined, "en")).toBe("");
    expect(formatBytes(-1, "en")).toBe("");
  });
});

describe("formatSpeed", () => {
  it("appends per-second suffix", () => {
    expect(formatSpeed(1_662_629, "en")).toBe("1.6 MB/s");
    expect(formatSpeed(1_662_629, "ru")).toBe("1,6 МБ/с");
    expect(formatSpeed(null, "en")).toBe("");
  });
});

describe("formatDuration", () => {
  it("formats minutes and hours", () => {
    expect(formatDuration(0)).toBe("0:00");
    expect(formatDuration(9)).toBe("0:09");
    expect(formatDuration(213)).toBe("3:33");
    expect(formatDuration(3661)).toBe("1:01:01");
    expect(formatDuration(null)).toBe("");
  });

  it("is used for eta too", () => {
    expect(formatEta(292)).toBe("4:52");
    expect(formatEta(null)).toBe("");
  });
});

describe("qualityLabel", () => {
  it("names common heights", () => {
    expect(qualityLabel(2160)).toBe("4K (2160p)");
    expect(qualityLabel(1440)).toBe("2K (1440p)");
    expect(qualityLabel(1080)).toBe("1080p");
    expect(qualityLabel(144)).toBe("144p");
  });
});

describe("formatViews", () => {
  it("is compact in both languages", () => {
    expect(formatViews(1222317, "en")).toBe("1.2M");
    expect(formatViews(338921, "en")).toBe("339K");
    expect(formatViews(950, "en")).toBe("950");
    expect(formatViews(1222317, "ru")).toBe("1,2\u00a0млн");
    expect(formatViews(338921, "ru")).toBe("339\u00a0тыс.");
  });

  it("is empty when unknown", () => {
    expect(formatViews(null, "en")).toBe("");
    expect(formatViews(-1, "en")).toBe("");
  });
});

describe("pluralCount", () => {
  it("picks the plural after a shortened number", () => {
    // "339 тыс. просмотров", although 338 921 alone takes "просмотр".
    expect(new Intl.PluralRules("ru").select(pluralCount(338921))).toBe("many");
    expect(pluralCount(21)).toBe(21);
    expect(pluralCount(999)).toBe(999);
  });
});
