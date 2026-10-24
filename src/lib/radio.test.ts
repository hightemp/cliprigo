import { describe, expect, it } from "vitest";
import { radioKey } from "./radio";

describe("radioKey", () => {
  const modes = ["video", "audio"] as const;

  it("moves with the arrow keys and wraps around", () => {
    expect(radioKey(modes, "video", "ArrowRight")).toBe("audio");
    expect(radioKey(modes, "video", "ArrowDown")).toBe("audio");
    expect(radioKey(modes, "audio", "ArrowRight")).toBe("video");
    expect(radioKey(modes, "audio", "ArrowLeft")).toBe("video");
    expect(radioKey(modes, "video", "ArrowUp")).toBe("audio");
  });

  it("jumps to the ends with Home and End", () => {
    const three = ["a", "b", "c"];
    expect(radioKey(three, "b", "Home")).toBe("a");
    expect(radioKey(three, "b", "End")).toBe("c");
  });

  it("ignores other keys", () => {
    expect(radioKey(modes, "video", "Enter")).toBeNull();
    expect(radioKey(modes, "video", "a")).toBeNull();
  });
});
