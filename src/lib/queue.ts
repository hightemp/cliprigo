import { ACTIVE_STATUSES } from "./types";
import type { QueueEntry, QueueEvent } from "./types";

/**
 * Applies a backend queue event to the list of active downloads. Finished records
 * leave the queue (they live on in the history); the input is never mutated.
 */
export function applyEvent(
  entries: readonly QueueEntry[],
  event: QueueEvent,
): QueueEntry[] {
  if (event.type === "progress") {
    return entries.map((e) =>
      e.id === event.id ? { ...e, progress: event.snapshot } : e,
    );
  }

  const { record } = event;
  if (!ACTIVE_STATUSES.includes(record.status)) {
    return entries.filter((e) => e.id !== record.id);
  }
  const index = entries.findIndex((e) => e.id === record.id);
  if (index === -1) {
    return [...entries, { ...record, progress: null }];
  }
  const previous = entries[index];
  // A re-queued download (retry) starts over.
  const progress = record.status === "queued" ? null : previous.progress;
  const next = [...entries];
  next[index] = { ...record, progress };
  return next;
}
