import { describe, expect, it } from "vitest";
import {
  TEMPLATE_EXAMPLES,
  fixTemplate,
  missingExt,
  previewName,
  splitFields,
} from "./template";

describe("file name template", () => {
  it("needs .%(ext)s for the quality suffix and the extension", () => {
    expect(missingExt("%(title)s [%(id)s].%(ext)s")).toBe(false);
    expect(missingExt("%(uploader)s/%(title)s.%(ext)s")).toBe(false);
    expect(missingExt("%(title)s")).toBe(true);
    expect(missingExt("%(title)s.mp4")).toBe(true);
    expect(missingExt("%(title)s-%(ext)s")).toBe(true);
  });

  it("is fixed by ending it with .%(ext)s", () => {
    expect(fixTemplate("%(title)s")).toBe("%(title)s.%(ext)s");
    expect(fixTemplate("%(title)s [%(id)s] ")).toBe(
      "%(title)s [%(id)s].%(ext)s",
    );
    // A fixed extension would be wrong for audio or webm, so it is replaced.
    expect(fixTemplate("%(title)s.mp4")).toBe("%(title)s.%(ext)s");
    expect(fixTemplate("%(title)s - %(ext)s")).toBe("%(title)s.%(ext)s");
    // Dots inside the name are kept.
    expect(fixTemplate("v1.2 %(title)s")).toBe("v1.2 %(title)s.%(ext)s");
  });

  it("offers only examples that keep the quality suffix", () => {
    expect(TEMPLATE_EXAMPLES.length).toBeGreaterThan(0);
    for (const example of TEMPLATE_EXAMPLES) {
      expect(missingExt(example)).toBe(false);
    }
  });

  it("splits help texts into plain text and fields shown as code", () => {
    expect(splitFields("Ends with .%(ext)s: 720p, %(…)s.")).toEqual([
      { text: "Ends with ", code: false },
      { text: ".%(ext)s", code: true },
      { text: ": 720p, ", code: false },
      { text: "%(…)s", code: true },
      { text: ".", code: false },
    ]);
    expect(splitFields("plain")).toEqual([{ text: "plain", code: false }]);
  });

  it("previews the file name like yt-dlp, with the quality suffix", () => {
    const sample = {
      title: "Sunset",
      id: "abc123",
      uploader: "Nature",
      upload_date: "20261024",
      ext: "mp4",
    };
    expect(previewName("%(title)s [%(id)s].%(ext)s", sample, " 720p")).toBe(
      "Sunset [abc123] 720p.mp4",
    );
    expect(
      previewName(
        "%(upload_date>%Y-%m-%d)s %(uploader)s/%(title)s.%(ext)s",
        sample,
        " 720p",
      ),
    ).toBe("2026-10-24 Nature/Sunset 720p.mp4");
    // Like the backend: no .%(ext)s, no suffix (and no extension).
    expect(previewName("%(title)s", sample, " 720p")).toBe("Sunset");
    // yt-dlp writes NA for fields it doesn't know.
    expect(previewName("%(nope)s.%(ext)s", sample, "")).toBe("NA.mp4");
  });
});
