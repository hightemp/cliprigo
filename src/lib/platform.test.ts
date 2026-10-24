import { describe, expect, it } from "vitest";
import { componentLabel, detectAndroid, isBundled } from "./platform";

describe("platform", () => {
  it("detects the Android WebView", () => {
    expect(
      detectAndroid(
        "Mozilla/5.0 (Linux; Android 15; Pixel 6; wv) AppleWebKit/537.36 Chrome/140.0 Mobile Safari/537.36",
      ),
    ).toBe(true);
    expect(
      detectAndroid("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36"),
    ).toBe(false);
  });

  it("names the JavaScript runtime QuickJS on Android", () => {
    expect(componentLabel("deno", true)).toBe("quickjs");
    expect(componentLabel("deno", false)).toBe("deno");
    expect(componentLabel("ffmpeg", true)).toBe("ffmpeg");
  });

  it("recognizes components shipped with the app", () => {
    expect(isBundled({ installed: true, version: "bundled" })).toBe(true);
    expect(isBundled({ installed: true, version: "v2.9.8" })).toBe(false);
  });
});
