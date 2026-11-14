// Every text of the interface, by area. Spanish is the source; each area file
// types its English table against the Spanish one, so a missing translation
// does not compile.

import common from "./common";
import onboarding from "./onboarding";

export type Language = "es" | "en";

const es = { ...common.es, ...onboarding.es };
const en: Record<keyof typeof es, string> = { ...common.en, ...onboarding.en };

export type MessageKey = keyof typeof es;

export const messages: Record<Language, Record<MessageKey, string>> = { es, en };
