import { describe, expect, it } from "vitest";
import {
  componentLabel,
  detectAndroid,
  isBundled,
  sharedLink,
} from "./platform";

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

  it("takes the link out of shared text", () => {
    expect(sharedLink("https://youtu.be/dQw4w9WgXcQ?si=abc")).toBe(
      "https://youtu.be/dQw4w9WgXcQ?si=abc",
    );
    expect(
      sharedLink(
        "Never Gonna Give You Up\nhttps://www.youtube.com/watch?v=dQw4w9WgXcQ",
      ),
    ).toBe("https://www.youtube.com/watch?v=dQw4w9WgXcQ");
    expect(sharedLink("Look: (https://youtu.be/x).")).toBe(
      "https://youtu.be/x",
    );
    expect(sharedLink("no link here")).toBeNull();
    expect(sharedLink(null)).toBeNull();
  });
});
