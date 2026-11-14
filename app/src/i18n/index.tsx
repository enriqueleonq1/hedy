import { createContext, useContext, useMemo, type ReactNode } from "react";

import { messages, type Language, type MessageKey } from "./messages";

export type { Language, MessageKey } from "./messages";

/** What the user chose in Ajustes: a language, or the system's. */
export type LanguageSetting = "system" | Language;

export type Params = Record<string, string | number>;
export type Translate = (key: MessageKey, params?: Params) => string;

/** The system's language: Spanish for `es-*`, English for anything else. */
export function systemLanguage(): Language {
  const tag = typeof navigator === "undefined" ? "" : (navigator.language ?? "");
  return tag.toLowerCase().startsWith("es") ? "es" : "en";
}

export function resolveLanguage(setting: LanguageSetting | null | undefined): Language {
  return setting === "es" || setting === "en" ? setting : systemLanguage();
}

/** Fills `{name}` placeholders. */
function fill(text: string, params?: Params): string {
  if (!params) return text;
  return text.replace(/\{(\w+)\}/g, (whole, name: string) => (name in params ? String(params[name]) : whole));
}

export function translator(language: Language): Translate {
  const table = messages[language];
  return (key, params) => fill(table[key], params);
}

interface I18n {
  language: Language;
  t: Translate;
}

// Spanish outside a provider (isolated component tests), as before i18n.
const I18nContext = createContext<I18n>({ language: "es", t: translator("es") });

export function I18nProvider({ setting, children }: { setting: LanguageSetting; children: ReactNode }) {
  const language = resolveLanguage(setting);
  const value = useMemo(() => ({ language, t: translator(language) }), [language]);
  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>;
}

export function useT(): Translate {
  return useContext(I18nContext).t;
}

export function useLanguage(): Language {
  return useContext(I18nContext).language;
}
