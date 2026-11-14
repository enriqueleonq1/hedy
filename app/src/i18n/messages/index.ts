// Every text of the interface, by area. Spanish is the source; each area file
// types its English table against the Spanish one, so a missing translation
// does not compile.

import accounts from "./accounts";
import common from "./common";
import memory from "./memory";
import onboarding from "./onboarding";
import settings from "./settings";
import shell from "./shell";
import status from "./status";
import welcome from "./welcome";
import workspace from "./workspace";

export type Language = "es" | "en";

const es = {
  ...common.es,
  ...shell.es,
  ...onboarding.es,
  ...welcome.es,
  ...accounts.es,
  ...settings.es,
  ...memory.es,
  ...status.es,
  ...workspace.es,
};
const en: Record<keyof typeof es, string> = {
  ...common.en,
  ...shell.en,
  ...onboarding.en,
  ...welcome.en,
  ...accounts.en,
  ...settings.en,
  ...memory.en,
  ...status.en,
  ...workspace.en,
};

export type MessageKey = keyof typeof es;

export const messages: Record<Language, Record<MessageKey, string>> = { es, en };
