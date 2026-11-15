import type { ReactNode } from "react";

import type { Account, Overview, TaskMode } from "../api/types";
import { useLanguage, useT } from "../i18n";
import { useInbox } from "../tasks/inbox";
import { MODE_LABEL } from "../tasks/timeline";
import { DEFAULT_ACCOUNT_COLOR, TIP_MASCOT_COLOR } from "../theme/tokens";
import { shortAccountLabel } from "./accounts";
import { SidebarRailProvider, useSidebarRail } from "./sidebarRail";
import { AccountsIcon, ActivityIcon, CollapseIcon, SettingsIcon } from "./ui/icons";
import { Mascot } from "./Mascot/Mascot";
import { paletteKeys } from "./palette/keys";
import { barColor, headlineText, planPercent, usageLabel, usePlanUsageRefresh } from "./planUsage";
import { projectColor } from "./projects";
import { useTerminals } from "./Terminal/TerminalProvider";
import { Button } from "./ui/Button";
import { Segmented } from "./ui/controls";
import { UsageIndicator } from "./UsageIndicator";

export type View =
  | { screen: "welcome" }
  /** `account`: scroll to that account's card and highlight it. */
  | { screen: "accounts"; account?: string }
  | { screen: "settings" }
  | { screen: "activity" }
  /** `task`: open that task instead of the most recent open one; `null`: a new task. */
  | { screen: "workspace"; project: string; task?: string | null }
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

const MODES = Object.keys(MODE_LABEL) as TaskMode[];

interface TopbarProps {
  overview: Overview;
  view: View;
  openProjects: string[];
  mode: TaskMode;
  onNavigate: (view: View) => void;
  onCloseProject: (path: string) => void;
  onMode: (mode: TaskMode) => void;
  /** Opens the command palette (also Ctrl/⌘ K). */
  onOpenPalette?: () => void;
}

export function Topbar({
  overview,
  view,
  openProjects,
  mode,
  onNavigate,
  onCloseProject,
  onMode,
  onOpenPalette,
}: TopbarProps) {
  const terminals = useTerminals();
  const t = useT();
  const keys = paletteKeys();
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
      {onOpenPalette && (
        <button
          type="button"
          className="palette-hint"
          aria-label={t("palette.open", { keys })}
          aria-keyshortcuts={keys === "⌘K" ? "Meta+K" : "Control+K"}
          title={t("palette.open", { keys })}
          onClick={onOpenPalette}
        >
          <span className="palette-hint__text">{t("palette.hint")}</span>
          <kbd>{keys}</kbd>
        </button>
      )}
      {view.screen === "workspace" && (
        <Segmented<TaskMode>
          label={t("shell.mode")}
          value={mode}
          options={MODES.map((value) => ({ value, label: t(MODE_LABEL[value]) }))}
          onChange={onMode}
        />
      )}
      <UsageIndicator overview={overview} onNavigate={onNavigate} />
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

/** A small ring for an account in the rail: how much of its headline window is used. */
function MiniRing({ percent, color }: { percent: number; color: string }) {
  const r = 10;
  const length = 2 * Math.PI * r;
  const filled = (length * Math.min(percent, 100)) / 100;
  return (
    <svg className="mini-ring" width="28" height="28" viewBox="0 0 28 28" aria-hidden="true">
      <circle cx="14" cy="14" r={r} className="mini-ring__track" />
      {percent > 0 && (
        <circle
          cx="14"
          cy="14"
          r={r}
          className="mini-ring__fill"
          stroke={color}
          strokeDasharray={`${filled.toFixed(1)} ${length.toFixed(1)}`}
          transform="rotate(-90 14 14)"
        />
      )}
    </svg>
  );
}

export function Sidebar({ overview, view, onNavigate }: { overview: Overview; view: View; onNavigate: (v: View) => void }) {
  const t = useT();
  const language = useLanguage();
  const { rail, toggle } = useSidebarRail();
  const accountById = new Map(overview.accounts.map((a) => [a.id, a]));
  const tipColor = defaultAccount(overview)?.color ?? TIP_MASCOT_COLOR;
  const active = viewProject(view);
  const used = overview.accounts.some((a) => a.usage.weekTokens > 0);
  const needsYou = useInbox().count;
  // The sidebar shows every account's plan usage: keep it fresh while it is on screen.
  usePlanUsageRefresh(overview.accounts, view.screen);
  // In the rail, names are not on screen: they go in the accessible name and the tooltip.
  const named = (name: string) => (rail ? { "aria-label": name, title: name } : {});

  return (
    <aside id="sidebar" className={`sidebar${rail ? " sidebar--rail" : ""}`} aria-label={t("common.projectsAndAccounts")}>
      <button
        type="button"
        className="sidebar__toggle"
        aria-expanded={!rail}
        aria-controls="sidebar"
        aria-label={rail ? t("shell.sidebar.expand") : t("shell.sidebar.collapse")}
        title={rail ? t("shell.sidebar.expand") : t("shell.sidebar.collapse")}
        onClick={toggle}
      >
        <CollapseIcon collapsed={rail} />
      </button>

      <button
        className="sidebar__item sidebar__nav"
        aria-current={view.screen === "activity" ? "page" : undefined}
        title={rail ? t("activity.title") : undefined}
        aria-label={
          needsYou === 0
            ? t("activity.title")
            : needsYou === 1
              ? t("activity.navOne")
              : t("activity.navMany", { n: needsYou })
        }
        onClick={() => onNavigate({ screen: "activity" })}
      >
        <ActivityIcon />
        <span className="sidebar__name">{t("activity.title")}</span>
        {needsYou > 0 && (
          <span className="nav-badge" aria-hidden>
            {needsYou}
          </span>
        )}
      </button>

      <section>
        <h2 className={rail ? "sr-only" : "sidebar__title"}>{t("common.projects")}</h2>
        {overview.projects.length === 0 && !rail && <p className="hint sidebar__empty">{t("shell.noProjects")}</p>}
        <ul className="sidebar__list">
          {overview.projects.map((p) => {
            const account = p.account ? accountById.get(p.account) : undefined;
            const color = projectColor(p, overview.accounts);
            return (
              <li key={p.path}>
                <button
                  className="sidebar__item"
                  aria-current={active === p.path ? "page" : undefined}
                  {...named(p.name)}
                  onClick={() => onNavigate({ screen: "workspace", project: p.path })}
                >
                  {rail ? (
                    <span className="sidebar__avatar" style={{ borderColor: color }} aria-hidden>
                      {p.name.charAt(0).toUpperCase()}
                    </span>
                  ) : (
                    <>
                      <span className="dot" style={{ background: color }} aria-hidden />
                      <span className="sidebar__name">{p.name}</span>
                      {account && (
                        <span className={`sidebar__meta${p.protected ? " sidebar__meta--protected" : ""}`}>
                          {shortAccountLabel(account)}
                        </span>
                      )}
                    </>
                  )}
                </button>
              </li>
            );
          })}
        </ul>
      </section>

      <section>
        <button
          className={rail ? "sidebar__item sidebar__accounts" : "sidebar__title sidebar__title--link"}
          aria-current={view.screen === "accounts" ? "page" : undefined}
          {...named(t("common.accountsAndModels"))}
          onClick={() => onNavigate({ screen: "accounts" })}
        >
          {rail ? <AccountsIcon /> : t("common.accountsAndModels")}
        </button>
        {overview.accounts.length === 0 && !rail && (
          <Button small variant="primary" onClick={() => onNavigate({ screen: "accounts" })}>
            {t("common.addAccount")}
          </Button>
        )}
        <ul className="sidebar__list">
          {overview.accounts.map((a) => {
            // The same headline window as the top bar and the home rings; the figure is in the label.
            const percent = planPercent(a) ?? 0;
            const usage = headlineText(a, t) ?? usageLabel(a, language);
            return (
              <li key={a.id}>
                <button
                  type="button"
                  className="sidebar__account"
                  title={`${a.name}: ${usage}`}
                  aria-label={t("shell.openAccount", { name: a.name, usage })}
                  aria-current={view.screen === "accounts" && view.account === a.id ? "true" : undefined}
                  onClick={() => onNavigate({ screen: "accounts", account: a.id })}
                >
                  {rail ? (
                    <MiniRing percent={percent} color={barColor(percent, a.color)} />
                  ) : (
                    <>
                      <span className="sidebar__account-name">{a.name}</span>
                      <span className="bar bar--thin" aria-hidden>
                        <span
                          className="bar__fill"
                          style={{ width: `${Math.min(percent, 100)}%`, background: barColor(percent, a.color) }}
                        />
                      </span>
                    </>
                  )}
                </button>
              </li>
            );
          })}
        </ul>
      </section>

      <span className="spacer" />
      <button
        className="sidebar__item sidebar__settings"
        aria-current={view.screen === "settings" ? "page" : undefined}
        {...named(t("common.settings"))}
        onClick={() => onNavigate({ screen: "settings" })}
      >
        <SettingsIcon />
        <span className="sidebar__name">{t("common.settings")}</span>
      </button>
      {!rail && (
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
      )}
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
    <SidebarRailProvider>
      <ShellFrame topbar={topbar} sidebar={sidebar} terminal={terminal}>
        {children}
      </ShellFrame>
    </SidebarRailProvider>
  );
}

function ShellFrame({
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
  const { rail } = useSidebarRail();
  return (
    <div className={`app${rail ? " app--rail" : ""}`}>
      {topbar}
      {sidebar}
      <main className="main" id="main">
        {children}
      </main>
      {terminal}
    </div>
  );
}

