import type { ReactNode } from "react";

import type { Account, Overview, TaskMode } from "../api/types";
import { formatTokens, formatUsd } from "../format";
import { useLanguage, useT, type Language } from "../i18n";
import { useNeedsYouCount } from "../tasks/hooks";
import { MODE_LABEL } from "../tasks/timeline";
import { DEFAULT_ACCOUNT_COLOR, TIP_MASCOT_COLOR } from "../theme/tokens";
import { shortAccountLabel } from "./accounts";
import { Mascot } from "./Mascot/Mascot";
import { projectColor } from "./projects";
import { PROVIDER_NAMES } from "./providers";
import { useTerminals } from "./Terminal/TerminalProvider";
import { Button } from "./ui/Button";
import { Segmented } from "./ui/controls";

export type View =
  | { screen: "welcome" }
  | { screen: "accounts" }
  | { screen: "settings" }
  | { screen: "activity" }
  /** `task`: open that task instead of the most recent open one. */
  | { screen: "workspace"; project: string; task?: string }
  /** `tab`: the skills tab instead of the notes. */
  | { screen: "memory"; project: string; tab?: "notes" | "skills" }
  | { screen: "status"; project: string };

/** The project a view belongs to (its workspace, memory or status). */
export function viewProject(view: View): string | null {
  return view.screen === "workspace" || view.screen === "memory" || view.screen === "status" ? view.project : null;
}

export function defaultAccount(overview: Overview): Account | undefined {
  return overview.accounts.find((a) => a.id === overview.defaultAccount) ?? overview.accounts[0];
}

/** "34%" from a reported limit, "$2,10" for API spend, tokens otherwise. */
export function usageLabel(account: Account, language: Language): string {
  const { usage } = account;
  if (usage.limit?.percent != null) return `${Math.round(usage.limit.percent)}%`;
  if (account.kind === "apiKey" && usage.weekCostUsd != null) return formatUsd(usage.weekCostUsd, language);
  if (usage.weekTokens > 0) return `${formatTokens(usage.weekTokens)} tok`;
  return "—";
}

/** One entry per provider, from its default (or first) account. */
function providerSummary(overview: Overview): { label: string; account: Account }[] {
  const seen = new Map<string, Account>();
  const def = overview.defaultAccount;
  const preferred = [...overview.accounts].sort((a, b) => Number(b.id === def) - Number(a.id === def));
  for (const a of preferred) {
    const label = PROVIDER_NAMES[a.provider].plan;
    if (!seen.has(label)) seen.set(label, a);
  }
  return [...seen].map(([label, account]) => ({ label, account }));
}

const MODES = Object.keys(MODE_LABEL) as TaskMode[];

interface TopbarProps {
  overview: Overview;
  view: View;
  openProjects: string[];
  mode: TaskMode;
  onNavigate: (view: View) => void;
  onCloseProject: (path: string) => void;
  onMode: (mode: TaskMode) => void;
}

export function Topbar({ overview, view, openProjects, mode, onNavigate, onCloseProject, onMode }: TopbarProps) {
  const terminals = useTerminals();
  const t = useT();
  const language = useLanguage();
  const color = defaultAccount(overview)?.color ?? DEFAULT_ACCOUNT_COLOR;
  const active = viewProject(view);

  return (
    <header className="topbar">
      <button
        className="brand"
        aria-label={t("shell.home")}
        aria-current={view.screen === "welcome" ? "page" : undefined}
        onClick={() => onNavigate({ screen: "welcome" })}
      >
        <Mascot color={color} state={terminals.sessions.some((s) => s.running) ? "working" : "idle"} size={30} label="" />
        <span className="brand__name">Hedy</span>
      </button>

      <nav className="tabs" aria-label={t("shell.openProjects")}>
        {openProjects.map((path) => {
          const project = overview.projects.find((p) => p.path === path);
          if (!project) return null;
          return (
            <span key={path} className="tab-wrap">
              <button
                className="tab"
                aria-current={active === path ? "page" : undefined}
                onClick={() => onNavigate({ screen: "workspace", project: path })}
              >
                <span className="dot" style={{ background: projectColor(project, overview.accounts) }} aria-hidden />
                {project.name}
              </button>
              <button
                className="tab__close"
                aria-label={t("common.closeNamed", { name: project.name })}
                onClick={() => onCloseProject(path)}
              >
                ×
              </button>
            </span>
          );
        })}
        <button
          className="tab tab--add"
          aria-label={t("shell.openAnother")}
          title={t("shell.openAnother")}
          onClick={() => onNavigate({ screen: "welcome" })}
        >
          +
        </button>
      </nav>

      <span className="spacer" />
      {view.screen === "workspace" && (
        <Segmented<TaskMode>
          label={t("shell.mode")}
          value={mode}
          options={MODES.map((value) => ({ value, label: t(MODE_LABEL[value]) }))}
          onChange={onMode}
        />
      )}
      {overview.accounts.length > 0 && (
        <div className="usage-pill" aria-label={t("shell.weekUsageByProvider")}>
          {providerSummary(overview).map(({ label, account }) => {
            const usage = usageLabel(account, language);
            return (
              <span key={label} className="usage-pill__item">
                <span className="dot" style={{ background: account.color }} aria-hidden />
                {label} <span className={usage === "—" ? "muted" : undefined}>{usage}</span>
              </span>
            );
          })}
        </div>
      )}
      <Button
        variant="secondary"
        small
        onClick={() =>
          terminals.sessions.length === 0
            ? terminals.open({ kind: "shell", path: null }, t("common.terminal"))
            : terminals.setPanelOpen(!terminals.panelOpen)
        }
        aria-pressed={terminals.panelOpen}
      >
        {t("common.terminal")}
        {terminals.sessions.length > 0 ? ` · ${terminals.sessions.length}` : ""}
      </Button>
    </header>
  );
}

export function Sidebar({ overview, view, onNavigate }: { overview: Overview; view: View; onNavigate: (v: View) => void }) {
  const t = useT();
  const language = useLanguage();
  const accountById = new Map(overview.accounts.map((a) => [a.id, a]));
  const tipColor = defaultAccount(overview)?.color ?? TIP_MASCOT_COLOR;
  const active = viewProject(view);
  const used = overview.accounts.some((a) => a.usage.weekTokens > 0);
  const needsYou = useNeedsYouCount();

  return (
    <aside className="sidebar" aria-label={t("common.projectsAndAccounts")}>
      <button
        className="sidebar__item sidebar__nav"
        aria-current={view.screen === "activity" ? "page" : undefined}
        aria-label={
          needsYou === 0
            ? t("activity.title")
            : needsYou === 1
              ? t("activity.navOne")
              : t("activity.navMany", { n: needsYou })
        }
        onClick={() => onNavigate({ screen: "activity" })}
      >
        <span className="sidebar__name">{t("activity.title")}</span>
        {needsYou > 0 && (
          <span className="nav-badge" aria-hidden>
            {needsYou}
          </span>
        )}
      </button>

      <section>
        <h2 className="sidebar__title">{t("common.projects")}</h2>
        {overview.projects.length === 0 && <p className="hint sidebar__empty">{t("shell.noProjects")}</p>}
        <ul className="sidebar__list">
          {overview.projects.map((p) => {
            const account = p.account ? accountById.get(p.account) : undefined;
            return (
              <li key={p.path}>
                <button
                  className="sidebar__item"
                  aria-current={active === p.path ? "page" : undefined}
                  onClick={() => onNavigate({ screen: "workspace", project: p.path })}
                >
                  <span className="dot" style={{ background: projectColor(p, overview.accounts) }} aria-hidden />
                  <span className="sidebar__name">{p.name}</span>
                  {account && (
                    <span className={`sidebar__meta${p.protected ? " sidebar__meta--protected" : ""}`}>
                      {shortAccountLabel(account)}
                    </span>
                  )}
                </button>
              </li>
            );
          })}
        </ul>
      </section>

      <section>
        <button
          className="sidebar__title sidebar__title--link"
          aria-current={view.screen === "accounts" ? "page" : undefined}
          onClick={() => onNavigate({ screen: "accounts" })}
        >
          {t("common.accountsAndModels")}
        </button>
        {overview.accounts.length === 0 && (
          <Button small variant="primary" onClick={() => onNavigate({ screen: "accounts" })}>
            {t("common.addAccount")}
          </Button>
        )}
        <ul className="sidebar__list">
          {overview.accounts.map((a) => {
            const percent = a.usage.limit?.percent ?? 0;
            const usage = usageLabel(a, language);
            return (
              <li key={a.id} className="sidebar__account">
                <div className="row row--between">
                  <span className="sidebar__account-name">{a.name}</span>
                  <span className="muted small">{usage}</span>
                </div>
                <div
                  className="bar bar--thin"
                  role="meter"
                  aria-label={`${a.name}: ${usage}`}
                  aria-valuemin={0}
                  aria-valuemax={100}
                  aria-valuenow={percent}
                >
                  <div className="bar__fill" style={{ width: `${Math.min(percent, 100)}%`, background: a.color }} />
                </div>
              </li>
            );
          })}
        </ul>
      </section>

      <span className="spacer" />
      <button
        className="sidebar__item sidebar__settings"
        aria-current={view.screen === "settings" ? "page" : undefined}
        onClick={() => onNavigate({ screen: "settings" })}
      >
        {t("common.settings")}
      </button>
      <div className="tip" role="note">
        <Mascot color={tipColor} state="idle" size={18} label="" />
        <p>
          {overview.accounts.length === 0
            ? t("shell.tip.noAccounts")
            : used
              ? t("shell.tip.used")
              : t("shell.tip.idle")}
        </p>
      </div>
    </aside>
  );
}

export function Shell({
  topbar,
  sidebar,
  children,
  terminal,
}: {
  topbar: ReactNode;
  sidebar: ReactNode;
  children: ReactNode;
  terminal: ReactNode;
}) {
  return (
    <div className="app">
      {topbar}
      {sidebar}
      <main className="main" id="main">
        {children}
      </main>
      {terminal}
    </div>
  );
}
