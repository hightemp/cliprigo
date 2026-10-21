import { en } from "./en";
import { ru } from "./ru";
import type { Lang } from "../format";
import type { Language } from "../types";

/** Forms keyed by `Intl.PluralRules` categories; `other` is the fallback. */
export interface Plural {
  one?: string;
  few?: string;
  many?: string;
  other: string;
}

type Widen<T> = {
  [K in keyof T]: T[K] extends string
    ? string
    : T[K] extends { other: string }
      ? Plural
      : Widen<T[K]>;
};

export type Messages = Widen<typeof en>;
export type Params = Record<string, string | number>;

const MESSAGES: Record<Lang, Messages> = { en, ru };

export function resolveLanguage(
  setting: Language,
  systemLanguage: string | undefined,
): Lang {
  if (setting !== "system") return setting;
  return systemLanguage?.toLowerCase().startsWith("ru") ? "ru" : "en";
}

function lookup(messages: Messages, key: string): unknown {
  return key
    .split(".")
    .reduce<unknown>(
      (node, part) =>
        typeof node === "object" && node !== null
          ? (node as Record<string, unknown>)[part]
          : undefined,
      messages,
    );
}

function interpolate(text: string, params: Params | undefined): string {
  if (!params) return text;
  return text.replace(/\{(\w+)\}/g, (match, name: string) =>
    name in params ? String(params[name]) : match,
  );
}

/** Looks up `key` (dotted path); unknown keys come back unchanged. */
export function translate(lang: Lang, key: string, params?: Params): string {
  const value = lookup(MESSAGES[lang], key);
  if (typeof value === "string") return interpolate(value, params);
  if (typeof value === "object" && value !== null && "other" in value) {
    const plural = value as Plural;
    const count = Number(params?.count ?? 0);
    const category = new Intl.PluralRules(lang).select(count) as keyof Plural;
    return interpolate(plural[category] ?? plural.other, params);
  }
  return key;
}
