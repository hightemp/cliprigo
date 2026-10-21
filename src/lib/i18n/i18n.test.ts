import { describe, expect, it } from "vitest";
import { en } from "./en";
import { ru } from "./ru";
import { resolveLanguage, translate } from "./index";

/** Flattens nested message objects into dotted keys; plural objects count as leaves. */
function keys(obj: object, prefix = ""): string[] {
  return Object.entries(obj).flatMap(([k, v]) => {
    const key = prefix ? `${prefix}.${k}` : k;
    const isPlural =
      typeof v === "object" && v !== null && "other" in (v as object);
    return typeof v === "object" && v !== null && !isPlural
      ? keys(v as object, key)
      : [key];
  });
}

describe("messages", () => {
  it("ru and en have exactly the same keys", () => {
    expect(keys(ru).sort()).toEqual(keys(en).sort());
  });

  it("have no empty strings", () => {
    for (const lang of [ru, en]) {
      for (const key of keys(lang)) {
        expect(translate(lang === ru ? "ru" : "en", key)).not.toBe("");
      }
    }
  });
});

describe("translate", () => {
  it("looks up nested keys and interpolates params", () => {
    expect(translate("en", "tabs.download")).toBe("Download");
    expect(translate("ru", "tabs.download")).toBe("Загрузка");
    expect(translate("en", "setup.downloading", { name: "yt-dlp" })).toContain(
      "yt-dlp",
    );
  });

  it("returns the key for unknown messages", () => {
    expect(translate("en", "nope.missing")).toBe("nope.missing");
  });

  it("uses Russian plural forms", () => {
    expect(translate("ru", "queue.count", { count: 1 })).toBe("1 загрузка");
    expect(translate("ru", "queue.count", { count: 3 })).toBe("3 загрузки");
    expect(translate("ru", "queue.count", { count: 5 })).toBe("5 загрузок");
    expect(translate("ru", "queue.count", { count: 21 })).toBe("21 загрузка");
    expect(translate("en", "queue.count", { count: 1 })).toBe("1 download");
    expect(translate("en", "queue.count", { count: 2 })).toBe("2 downloads");
  });
});

describe("resolveLanguage", () => {
  it("follows the system language unless set", () => {
    expect(resolveLanguage("system", "ru-RU")).toBe("ru");
    expect(resolveLanguage("system", "uk-UA")).toBe("en");
    expect(resolveLanguage("system", "en-US")).toBe("en");
    expect(resolveLanguage("system", undefined)).toBe("en");
    expect(resolveLanguage("ru", "en-US")).toBe("ru");
    expect(resolveLanguage("en", "ru-RU")).toBe("en");
  });
});
