// Every text of the interface, by area. Spanish is the source; each area file
// types its English table against the Spanish one, so a missing translation
// does not compile.

import accounts from "./accounts";
import ask from "./ask";
import activity from "./activity";
import carry from "./carry";
import code from "./code";
import common from "./common";
import composer from "./composer";
import create from "./create";
import edit from "./edit";
import mascot from "./mascot";
import memory from "./memory";
import onboarding from "./onboarding";
import palette from "./palette";
import queue from "./queue";
import review from "./review";
import reviewer from "./reviewer";
import rules from "./rules";
import settings from "./settings";
import ship from "./ship";
import shell from "./shell";
import skillLibrary from "./skillLibrary";
import skills from "./skills";
import statusbar from "./statusbar";
import sync from "./sync";
import team from "./team";
import status from "./status";
import templates from "./templates";
import themes from "./themes";
import task from "./task";
import waits from "./waits";
import welcome from "./welcome";
import workspace from "./workspace";

export type Language = "es" | "en";

const es = {
  ...common.es,
  ...shell.es,
  ...mascot.es,
  ...onboarding.es,
  ...welcome.es,
  ...accounts.es,
  ...settings.es,
  ...memory.es,
  ...skills.es,
  ...skillLibrary.es,
  ...status.es,
  ...workspace.es,
  ...activity.es,
  ...queue.es,
  ...templates.es,
  ...review.es,
  ...reviewer.es,
  ...task.es,
  ...waits.es,
  ...palette.es,
  ...create.es,
  ...composer.es,
  ...ship.es,
  ...code.es,
  ...statusbar.es,
  ...edit.es,
  ...carry.es,
  ...themes.es,
  ...sync.es,
  ...team.es,
  ...ask.es,
  ...rules.es,
};
const en: Record<keyof typeof es, string> = {
  ...common.en,
  ...shell.en,
  ...mascot.en,
  ...onboarding.en,
  ...welcome.en,
  ...accounts.en,
  ...settings.en,
  ...memory.en,
  ...skills.en,
  ...skillLibrary.en,
  ...status.en,
  ...workspace.en,
  ...activity.en,
  ...queue.en,
  ...templates.en,
  ...review.en,
  ...reviewer.en,
  ...task.en,
  ...waits.en,
  ...palette.en,
  ...create.en,
  ...composer.en,
  ...ship.en,
  ...code.en,
  ...statusbar.en,
  ...edit.en,
  ...carry.en,
  ...themes.en,
  ...sync.en,
  ...team.en,
  ...ask.en,
  ...rules.en,
};

export type MessageKey = keyof typeof es;

export const messages: Record<Language, Record<MessageKey, string>> = { es, en };
