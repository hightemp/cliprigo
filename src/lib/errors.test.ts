import { describe, expect, it } from "vitest";
import { errorMessage, parseStoredError } from "./errors";
import { translate } from "./i18n";

const tEn = (key: string, params?: Record<string, string | number>) =>
  translate("en", key, params);

describe("errorMessage", () => {
  it("translates known codes", () => {
    const msg = errorMessage(
      { code: "ytdlp_bot_check", message: "Sign in to confirm" },
      tEn,
    );
    expect(msg).not.toContain("errors.");
    expect(msg.toLowerCase()).toContain("bot");
  });

  it("can include the backend message in the translation", () => {
    const msg = errorMessage(
      {
        code: "ytdlp_timeout",
        message:
          "no answer in 120 s; last output: [youtube] x: Downloading webpage",
      },
      tEn,
    );
    expect(msg).toContain("proxy");
    expect(msg).toContain("[youtube] x: Downloading webpage");
  });

  it("falls back to the raw message for unknown codes", () => {
    expect(errorMessage({ code: "weird", message: "boom" }, tEn)).toBe("boom");
  });

  it("accepts strings and Error objects", () => {
    expect(errorMessage("plain", tEn)).toBe("plain");
    expect(errorMessage(new Error("oops"), tEn)).toBe("oops");
    expect(errorMessage(undefined, tEn)).toBe(
      translate("en", "errors.unknown"),
    );
  });
});

describe("parseStoredError", () => {
  it("reads the JSON the queue stores", () => {
    expect(parseStoredError('{"code":"ytdlp_network","message":"x"}')).toEqual({
      code: "ytdlp_network",
      message: "x",
    });
  });

  it("tolerates plain text and empty values", () => {
    expect(parseStoredError("old text")).toEqual({
      code: "other",
      message: "old text",
    });
    expect(parseStoredError(null)).toBeNull();
    expect(parseStoredError("")).toBeNull();
  });
});
