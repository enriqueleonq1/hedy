import { useState, type ReactNode } from "react";

import type { Account, Overview } from "../api/types";
import { useLanguage, useT } from "../i18n";
import { useCreate } from "../screens/create/CreateProvider";
import { useNowSeconds } from "../tasks/countdown";
import { isOpen } from "../tasks/timeline";
import { useConversations } from "../tasks/hooks";
import { useInbox } from "../tasks/inbox";
import { DEFAULT_ACCOUNT_COLOR } from "../theme/tokens";
import { shortAccountLabel } from "./accounts";
import { useFocusMode } from "./focusMode";
import { SidebarRailProvider, useSidebarRail } from "./sidebarRail";
import { AccountsIcon, ActivityIcon, CollapseIcon, GraphIcon, QuestionIcon, SettingsIcon } from "./ui/icons";
import { MenuButton } from "./ui/Menu";
import { Mascot } from "./Mascot/Mascot";
import { modKey, paletteKeys } from "./palette/keys";
import { headlineText, levelClass, planPercent, usageLabel, usageTitle, usePlanRefresh } from "./planUsage";
import { projectColor } from "./projects";
import { useTerminals } from "./Terminal/TerminalProvider";
import { Button } from "./ui/Button";

export type View =
  | { screen: "welcome" }
  /** `account`: scroll to that account's card and highlight it. */
  | { screen: "accounts"; account?: string }
  /** `section`: scroll to the theme gallery ("Cambiar tema…"), the sync card or the teams card. */
  | { screen: "settings"; section?: "themes" | "sync" | "teams" }
  | { screen: "activity" }
  /** "Pregunta a tu memoria"; `project`: about that project's memory. */
  | { screen: "ask"; project?: string }
  /**
   * `task`: open that task instead of the most recent open one; `null`: a new task,
   * continuing conversation `carry` when given ("Llevar a un proyecto").
   */
  | { screen: "workspace"; project: string; task?: string | null; carry?: string }
  /** `tab`: the skills tab instead of the notes. */
  | { screen: "memory"; project: string; tab?: "notes" | "skills" }
  | { screen: "status"; project: string }
  /**
   * A question outside any project: `task` opens that conversation, none a
   * new one (its composer starting with `draft`, if given).
   */
  | { screen: "conversation"; task?: string; draft?: string };

/** Where a task opens: its project's workspace or, outside any project, its conversation. */
export function taskView(projects: { path: string }[], project: string, task: string): View {
  return projects.some((p) => p.path === project)
    ? { screen: "workspace", project, task }
    : { screen: "conversation", task };
}

/** Conversations listed in the sidebar before "Ver todas". */
const SIDEBAR_CONVERSATIONS = 5;

/** The project a view belongs to (its workspace, memory or status). */
export function viewProject(view: View): string | null {
  return view.screen === "workspace" || view.screen === "memory" || view.screen === "status" ? view.project : null;
}

export function defaultAccount(overview: Overview): Account | undefined {
  return overview.accounts.find((a) => a.id === overview.defaultAccount) ?? overview.accounts[0];
}


interface TopbarProps {
  overview: Overview;
  view: View;
  onNavigate: (view: View) => void;
  /** Opens the command palette (also Ctrl/⌘ K). */
  onOpenPalette?: () => void;
}

export function Topbar({ overview, view, onNavigate, onOpenPalette }: TopbarProps) {
  const terminals = useTerminals();
  const focusMode = useFocusMode();
  const t = useT();
  const keys = paletteKeys();
  const color = defaultAccount(overview)?.color ?? DEFAULT_ACCOUNT_COLOR;

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

      <span className="spacer" />
      {focusMode.focus && (
        <button
          type="button"
          className="topbar__quiet"
          aria-label={t("shell.focus.exit")}
          aria-keyshortcuts="Control+Backslash"
          title={`${t("shell.focus.exit")} (${modKey()} \\)`}
          onClick={focusMode.toggle}
        >
          {t("shell.focus.on")}
          <kbd>{`${modKey()} \\`}</kbd>
        </button>
      )}
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
      <button
        type="button"
        className="topbar__quiet"
        onClick={() =>
          terminals.sessions.length === 0
            ? terminals.open({ kind: "shell", path: null }, t("common.terminal"))
            : terminals.setPanelOpen(!terminals.panelOpen)
        }
        aria-pressed={terminals.panelOpen}
      >
        {t("common.terminal")}
        {terminals.sessions.length > 0 ? ` · ${terminals.sessions.length}` : ""}
      </button>
    </header>
  );
}

export function Sidebar({ overview, view, onNavigate }: { overview: Overview; view: View; onNavigate: (v: View) => void }) {
  const t = useT();
  const language = useLanguage();
  const { rail, toggle } = useSidebarRail();
  const accountById = new Map(overview.accounts.map((a) => [a.id, a]));
  const active = viewProject(view);
  const needsYou = useInbox().count;
  const create = useCreate();
  const conversations = useConversations().data ?? [];
  const [allConversations, setAllConversations] = useState(false);
  const shownConversations = allConversations ? conversations : conversations.slice(0, SIDEBAR_CONVERSATIONS);
  const openConversation = view.screen === "conversation" ? (view.task ?? null) : undefined;
  // Plan figures and how old they are (`PlanUsageProvider` keeps them fresh).
  const planBusy = usePlanRefresh().busy;
  const now = useNowSeconds(60_000);
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

      <button
        className="sidebar__item sidebar__nav"
        aria-current={view.screen === "ask" ? "page" : undefined}
        {...named(t("ask.nav"))}
        onClick={() => onNavigate({ screen: "ask" })}
      >
        <GraphIcon />
        <span className="sidebar__name">{t("ask.nav")}</span>
      </button>

      <section>
        <div className={rail ? "sr-only" : "sidebar__head"}>
          <h2 className="sidebar__title">{t("common.projects")}</h2>
          {!rail && overview.accounts.length > 0 && (
            <MenuButton label={t("create.addProject")} items={create.menu.slice(0, 2)} className="menu-button sidebar__add">
              <span aria-hidden>+</span>
            </MenuButton>
          )}
        </div>
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

      {overview.accounts.length > 0 && (
        <section>
          {rail ? (
            <button
              className="sidebar__item"
              aria-current={openConversation === null ? "page" : undefined}
              {...named(t("create.quickQuestion"))}
              onClick={() => create.quickQuestion()}
            >
              <QuestionIcon />
            </button>
          ) : (
            <>
              <div className="sidebar__head">
                <h2 className="sidebar__title">{t("conversation.list")}</h2>
                <button
                  type="button"
                  className="menu-button sidebar__add"
                  aria-label={t("create.quickQuestion")}
                  title={t("create.quickQuestion")}
                  aria-current={openConversation === null ? "page" : undefined}
                  onClick={() => create.quickQuestion()}
                >
                  <span aria-hidden>+</span>
                </button>
              </div>
              {conversations.length === 0 ? (
                <p className="hint sidebar__empty">{t("conversation.sidebarEmpty")}</p>
              ) : (
                <ul className="sidebar__list">
                  {shownConversations.map((task) => (
                    <li key={task.id}>
                      <button
                        className="sidebar__item sidebar__item--conversation"
                        aria-current={openConversation === task.id ? "page" : undefined}
                        title={task.title}
                        onClick={() => onNavigate({ screen: "conversation", task: task.id })}
                      >
                        <span
                          className={`sidebar__bullet${isOpen(task) && task.live ? " sidebar__bullet--live" : ""}`}
                          aria-hidden
                        />
                        <span className="sidebar__name">{task.title}</span>
                      </button>
                    </li>
                  ))}
                </ul>
              )}
              {conversations.length > SIDEBAR_CONVERSATIONS && (
                <button
                  type="button"
                  className="sidebar__more"
                  aria-expanded={allConversations}
                  onClick={() => setAllConversations((all) => !all)}
                >
                  {allConversations ? t("conversation.fewer") : t("conversation.more", { n: conversations.length })}
                </button>
              )}
            </>
          )}
        </section>
      )}

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
            // The same headline window as the top bar; quiet unless it gets close to the limit.
            const percent = planPercent(a);
            const usage = headlineText(a, t) ?? usageLabel(a, language);
            const busy = planBusy.has(a.id);
            return (
              <li key={a.id}>
                <button
                  type="button"
                  className="sidebar__item sidebar__account"
                  title={[usageTitle(a, t, language, now), busy && t("shell.usage.refreshing")].filter(Boolean).join(" · ")}
                  aria-label={t("shell.openAccount", { name: a.name, usage })}
                  aria-busy={busy || undefined}
                  aria-current={view.screen === "accounts" && view.account === a.id ? "true" : undefined}
                  onClick={() => onNavigate({ screen: "accounts", account: a.id })}
                >
                  <span className="dot" style={{ background: a.color }} aria-hidden />
                  <span className="sidebar__name">{a.name}</span>
                  {percent !== null && (
                    <span
                      className={["sidebar__meta sidebar__percent", levelClass(percent), busy && "plan-figure--busy"]
                        .filter(Boolean)
                        .join(" ")}
                    >
                      {Math.round(percent)}%
                    </span>
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
    </aside>
  );
}

interface ShellParts {
  topbar: ReactNode;
  sidebar: ReactNode;
  children: ReactNode;
  terminal: ReactNode;
  /** The status bar along the bottom of the window (`StatusBar`). */
  statusbar?: ReactNode;
}

export function Shell(parts: ShellParts) {
  return (
    <SidebarRailProvider>
      <ShellFrame {...parts} />
    </SidebarRailProvider>
  );
}

function ShellFrame({ topbar, sidebar, children, terminal, statusbar }: ShellParts) {
  const { rail } = useSidebarRail();
  const { focus } = useFocusMode();
  return (
    <div className={`app${rail ? " app--rail" : ""}${focus ? " app--focus" : ""}`}>
      {topbar}
      {sidebar}
      <main className="main" id="main">
        {children}
      </main>
      {terminal}
      {statusbar}
    </div>
  );
}

