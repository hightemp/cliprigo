import { describe, expect, it } from "vitest";
import { errorReport } from "./report";

describe("errorReport", () => {
  it("puts the error code and raw message before the diagnostics", () => {
    const report = errorReport(
      {
        code: "ytdlp_timeout",
        message: "no answer in 120 s; last output: [youtube] x",
      },
      "Cliprigo 0.1.2 on windows x86_64",
      "probe https://youtu.be/x",
    );
    expect(report).toContain("Action: probe https://youtu.be/x");
    expect(report).toContain("Error code: ytdlp_timeout");
    expect(report).toContain(
      "Message: no answer in 120 s; last output: [youtube] x",
    );
    expect(report.indexOf("Error code")).toBeLessThan(
      report.indexOf("Cliprigo 0.1.2"),
    );
  });

  it("accepts plain strings, Error objects and unknown values", () => {
    expect(errorReport("boom", "D")).toContain("Message: boom");
    expect(errorReport(new Error("oops"), "D")).toContain("Message: oops");
    expect(errorReport(undefined, "D")).toContain("Error code: unknown");
  });
});
