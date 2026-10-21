import type { AppError } from "./types";

function normalize(err: unknown): AppError {
  if (
    typeof err === "object" &&
    err !== null &&
    "code" in err &&
    "message" in err
  ) {
    const e = err as AppError;
    return { code: String(e.code), message: String(e.message) };
  }
  if (typeof err === "string") return { code: "unknown", message: err };
  if (err instanceof Error) return { code: "unknown", message: err.message };
  return { code: "unknown", message: "" };
}

/**
 * Text for "Copy details": what failed, the backend error as is (not translated),
 * then the diagnostics report, so it can be pasted into a chat or an issue.
 */
export function errorReport(
  err: unknown,
  diagnostics: string,
  action?: string,
): string {
  const { code, message } = normalize(err);
  return [
    action ? `Action: ${action}` : null,
    `Error code: ${code}`,
    `Message: ${message}`,
    "",
    diagnostics,
  ]
    .filter((line) => line !== null)
    .join("\n");
}
