import type { AppError } from "./types";

export type Translate = (
  key: string,
  params?: Record<string, string | number>,
) => string;

function isAppError(value: unknown): value is AppError {
  return (
    typeof value === "object" &&
    value !== null &&
    typeof (value as AppError).code === "string" &&
    typeof (value as AppError).message === "string"
  );
}

/** A user-facing message for anything a command can throw. */
export function errorMessage(err: unknown, t: Translate): string {
  if (isAppError(err)) {
    const key = `errors.${err.code}`;
    // Translations may show the backend's detail via {message}.
    const translated = t(key, { message: err.message });
    // Codes without a friendly text (io, db...) show the backend's message.
    if (translated !== key) return translated;
    return err.message || t("errors.unknown");
  }
  if (typeof err === "string" && err) return err;
  if (err instanceof Error && err.message) return err.message;
  return t("errors.unknown");
}

/** The queue stores failures as `{ code, message }` JSON; older rows may be plain text. */
export function parseStoredError(stored: string | null): AppError | null {
  if (!stored) return null;
  try {
    const parsed: unknown = JSON.parse(stored);
    if (isAppError(parsed)) return parsed;
  } catch {
    // Not JSON: plain text from an older version.
  }
  return { code: "other", message: stored };
}
