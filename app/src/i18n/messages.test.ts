import accounts from "./messages/accounts";
import common from "./messages/common";
import memory from "./messages/memory";
import onboarding from "./messages/onboarding";
import settings from "./messages/settings";
import shell from "./messages/shell";
import status from "./messages/status";
import welcome from "./messages/welcome";
import workspace from "./messages/workspace";
import { messages } from "./messages";
import { translator } from ".";

const AREAS = { accounts, common, memory, onboarding, settings, shell, status, welcome, workspace };

const placeholders = (text: string) => [...text.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort();

describe("messages", () => {
  it("has a non-empty Spanish and English text for every key", () => {
    const keys = Object.keys(messages.es);
    expect(Object.keys(messages.en).sort()).toEqual([...keys].sort());
    for (const key of keys) {
      const k = key as keyof typeof messages.es;
      expect(messages.es[k].trim(), `es ${key}`).not.toBe("");
      expect(messages.en[k].trim(), `en ${key}`).not.toBe("");
    }
  });

  it("uses the same placeholders in both languages", () => {
    for (const key of Object.keys(messages.es) as (keyof typeof messages.es)[]) {
      expect(placeholders(messages.en[key]), key).toEqual(placeholders(messages.es[key]));
    }
  });

  it("never defines a key in two areas", () => {
    const seen = new Map<string, string>();
    for (const [area, table] of Object.entries(AREAS)) {
      for (const key of Object.keys(table.es)) {
        expect(seen.get(key), `${key} in ${area}`).toBeUndefined();
        seen.set(key, area);
      }
      expect(Object.keys(table.en).sort(), area).toEqual(Object.keys(table.es).sort());
    }
    expect(seen.size).toBe(Object.keys(messages.es).length);
  });

  it("fills placeholders and leaves unknown ones alone", () => {
    const t = translator("en");
    expect(t("common.closeNamed", { name: "hedy" })).toBe("Close hedy");
    expect(translator("es")("time.days", { n: 3 })).toBe("hace 3 días");
    expect(t("time.days")).toBe("{n} days ago");
  });
});
