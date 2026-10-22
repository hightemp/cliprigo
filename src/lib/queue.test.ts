import { describe, expect, it } from "vitest";
import { applyEvent } from "./queue";
import type { DownloadRecord, QueueEntry, Snapshot } from "./types";

function record(id: number, status: DownloadRecord["status"]): DownloadRecord {
  return {
    id,
    videoId: `v${id}`,
    url: `https://youtu.be/v${id}`,
    title: `Video ${id}`,
    uploader: null,
    duration: null,
    thumbnailUrl: null,
    format: "video",
    quality: null,
    codec: "auto",
    filePath: null,
    fileSize: null,
    status,
    error: null,
    playlistId: null,
    playlistTitle: null,
    createdAt: 1,
    completedAt: null,
    outputDir: "/dl",
  };
}

const snap: Snapshot = {
  stage: "downloading",
  part: 1,
  percent: 42,
  downloaded: 42,
  total: 100,
  speed: 10,
  eta: 5,
  postprocessor: null,
};

describe("applyEvent", () => {
  it("appends new active records in arrival order", () => {
    let q: QueueEntry[] = [];
    q = applyEvent(q, { type: "status", record: record(1, "queued") });
    q = applyEvent(q, { type: "status", record: record(2, "queued") });
    expect(q.map((e) => e.id)).toEqual([1, 2]);
    expect(q[0].progress).toBeNull();
  });

  it("updates status in place and keeps progress", () => {
    let q = applyEvent([], { type: "status", record: record(1, "queued") });
    q = applyEvent(q, { type: "status", record: record(1, "downloading") });
    q = applyEvent(q, { type: "progress", id: 1, snapshot: snap });
    q = applyEvent(q, { type: "status", record: record(1, "processing") });
    expect(q).toHaveLength(1);
    expect(q[0].status).toBe("processing");
    expect(q[0].progress).toEqual(snap);
  });

  it("removes records that finished", () => {
    for (const status of [
      "completed",
      "failed",
      "canceled",
      "interrupted",
    ] as const) {
      let q = applyEvent([], { type: "status", record: record(1, "queued") });
      q = applyEvent(q, { type: "status", record: record(1, status) });
      expect(q).toEqual([]);
    }
  });

  it("clears stale progress when re-queued for a retry", () => {
    let q = applyEvent([], {
      type: "status",
      record: record(1, "downloading"),
    });
    q = applyEvent(q, { type: "progress", id: 1, snapshot: snap });
    q = applyEvent(q, { type: "status", record: record(1, "queued") });
    expect(q[0].progress).toBeNull();
  });

  it("ignores progress for unknown ids and does not mutate input", () => {
    const q = applyEvent([], { type: "status", record: record(1, "queued") });
    const frozen = Object.freeze([...q]);
    const next = applyEvent(frozen as QueueEntry[], {
      type: "progress",
      id: 99,
      snapshot: snap,
    });
    expect(next).toEqual(q);
  });
});
