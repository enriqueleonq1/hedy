// The top bar's one place for plan usage: a compact indicator (the accounts at
// risk, or the one closest to its limit) that opens every account's windows.

import { useEffect, useId, useRef, useState } from "react";

import type { Account, Overview } from "../api/types";
import { useLanguage, useT, type Language, type Translate } from "../i18n";
import {
  barColor,
  formatBalance,
  headlineText,
  planPercent,
  resetLabel,
  sharedWindows,
  shortWindowLabel,
  usageLabel,
  usageLevel,
  WARNING_PERCENT,
  windowText,
} from "./planUsage";
import type { View } from "./Layout";

interface Reading {
  account: Account;
  /** The headline percentage, or what a task reported; null when unknown. */
  percent: number | null;
  /** "Semana · 83%", "34%", "$2,10", "—". */
  text: string;
}

/** Every account's compact reading, closest to its limit first and unknown ones last. */
export function usageReadings(accounts: Account[], t: Translate, language: Language): Reading[] {
  return accounts
    .map((account) => {
      const percent = planPercent(account);
      return { account, percent, text: headlineText(account, t) ?? usageLabel(account, language) };
    })
    .sort((a, b) => (b.percent ?? -1) - (a.percent ?? -1));
}

/** The accounts the indicator names: those at risk (≥ 80 %), else the one closest to its limit. */
export function indicatorReadings(readings: Reading[]): Reading[] {
  const atRisk = readings.filter((r) => r.percent !== null && r.percent >= WARNING_PERCENT);
  return atRisk.length > 0 ? atRisk.slice(0, 3) : readings.slice(0, 1);
}

function toneClass(percent: number | null): string {
  if (percent === null) return "muted";
  const level = usageLevel(percent);
  return level === "ok" ? "" : `level--${level}`;
}

export function UsageIndicator({ overview, onNavigate }: { overview: Overview; onNavigate: (view: View) => void }) {
  const t = useT();
  const language = useLanguage();
  const [open, setOpen] = useState(false);
  const wrap = useRef<HTMLDivElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  const panelId = useId();

  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (!wrap.current?.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("mousedown", onDown);
    return () => document.removeEventListener("mousedown", onDown);
  }, [open]);

  if (overview.accounts.length === 0) return null;
  const readings = usageReadings(overview.accounts, t, language);
  const shown = indicatorReadings(readings);
  const summary = shown.map((r) => `${r.account.name}: ${r.text}`).join(", ");

  const close = () => {
    setOpen(false);
    trigger.current?.focus();
  };
  const go = (account: string) => {
    setOpen(false);
    onNavigate({ screen: "accounts", account });
  };

  return (
    <div
      className="usage-indicator"
      ref={wrap}
      onKeyDown={(e) => {
        if (e.key === "Escape" && open) {
          e.stopPropagation();
          close();
        }
      }}
    >
      <button
        ref={trigger}
        type="button"
        className="usage-pill usage-indicator__trigger"
        aria-expanded={open}
        aria-controls={panelId}
        aria-label={t("shell.usage.trigger", { summary })}
        onClick={() => setOpen((o) => !o)}
      >
        {shown.map((r) => (
          <span key={r.account.id} className="usage-indicator__chip">
            <span className="dot" style={{ background: r.account.color }} aria-hidden />
            <span className="usage-indicator__name">{r.account.name}</span>
            <span className={toneClass(r.percent)}>{r.text}</span>
          </span>
        ))}
        <span className="usage-indicator__caret" aria-hidden>
          ▾
        </span>
      </button>
      {open && (
        <div id={panelId} className="usage-popover" role="dialog" aria-label={t("shell.usage.title")}>
          <p className="usage-popover__title">{t("shell.usage.title")}</p>
          <ul className="usage-popover__list">
            {readings.map(({ account }) => (
              <li key={account.id}>
                <UsageRow account={account} onOpen={() => go(account.id)} />
              </li>
            ))}
          </ul>
          <button
            type="button"
            className="link-button usage-popover__all"
            onClick={() => {
              setOpen(false);
              onNavigate({ screen: "accounts" });
            }}
          >
            {t("shell.usage.all")}
          </button>
        </div>
      )}
    </div>
  );
}

/** An account in the popover: each shared window with its bar and when it renews. */
function UsageRow({ account, onOpen }: { account: Account; onOpen: () => void }) {
  const t = useT();
  const language = useLanguage();
  const usage = account.usage.plan.usage;
  const windows = usage ? sharedWindows(usage) : [];
  let fallback: string | null = null;
  if (windows.length === 0) {
    if (usage?.balance) fallback = `${t("accounts.plan.balance")}: ${formatBalance(usage.balance, language)}`;
    else if (account.usage.limit?.percent != null) fallback = usageLabel(account, language);
    else if (account.kind === "apiKey") fallback = t("welcome.plans.apiKey");
    else if (account.usage.plan.supported) fallback = t("accounts.plan.checking");
    else fallback = t("accounts.plan.notReported", { provider: account.providerName });
  }
  const label = windows.length ? windows.map((w) => windowText(w, t)).join(", ") : (fallback ?? "");

  return (
    <button
      type="button"
      className="usage-row"
      aria-label={t("shell.openAccount", { name: account.name, usage: label })}
      onClick={onOpen}
    >
      <span className="usage-row__name">
        <span className="dot" style={{ background: account.color }} aria-hidden />
        {account.name}
      </span>
      {windows.map((w) => {
        const resets = resetLabel(w, t, language);
        return (
          <span key={`${w.kind}-${w.minutes ?? ""}`} className="usage-row__window">
            <span className="usage-row__line">
              <span>{shortWindowLabel(w, t)}</span>
              <span className={toneClass(w.percent)}>{Math.round(w.percent)}%</span>
            </span>
            <span className="bar bar--thin" aria-hidden>
              <span className="bar__fill" style={{ width: `${Math.min(w.percent, 100)}%`, background: barColor(w.percent, account.color) }} />
            </span>
            {resets && <span className="small muted">{resets}</span>}
          </span>
        );
      })}
      {fallback && <span className="small muted">{fallback}</span>}
    </button>
  );
}
