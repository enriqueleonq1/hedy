import { useCallback, useEffect, useState, type ReactNode } from "react";

import type { Account, Overview, UsageWindow } from "../api/types";
import { useLanguage, useT, type Language, type MessageKey, type Translate } from "../i18n";
import { useCreate } from "../screens/create/CreateProvider";
import { useNowSeconds } from "../tasks/countdown";
import { isOpen } from "../tasks/timeline";
import { useConversations, useTasks } from "../tasks/hooks";
import { useInbox } from "../tasks/inbox";
import { DEFAULT_ACCOUNT_COLOR } from "../theme/tokens";
import { useFocusMode } from "./focusMode";
import { useSidebarMenus } from "./SidebarMenus";
import { SidebarRailProvider, useSidebarRail } from "./sidebarRail";
import { SidebarTasks, useOpenProjects } from "./SidebarTasks";
import { ChevronIcon, CollapseIcon, HomeIcon, LockIcon, MemoryIcon, PlusIcon, QuestionIcon, SearchIcon, SettingsIcon, TerminalIcon } from "./ui/icons";
import { MenuButton } from "./ui/Menu";
import { Mascot } from "./Mascot/Mascot";
import { modKey, paletteKeys } from "./palette/keys";
import {
  headlineText,
  levelClass,
  planPercent,
  resetLabel,
  sharedWindows,
  usageLabel,
  usageTitle,
  usePlanRefresh,
  windowLabel,
  windowText,
} from "./planUsage";
import { Meter } from "./ui/Meter";
import { timeAgo } from "../format";
import { projectColor } from "./projects";
import { useTerminals } from "./Terminal/TerminalProvider";
import { Button } from "./ui/Button";

/** Ajustes' sections (`#settings/<section>`). */
export type SettingsSection = "general" | "accounts" | "appearance" | "saving" | "templates" | "sync" | "teams" | "advanced";

/** Memoria's sections. */
export type MemoryTab = "notes" | "skills" | "pending" | "map";

export type View =
  | { screen: "welcome" }
  /** `account`: scroll to that account's card and highlight it. */
  | { screen: "accounts"; account?: string }
  /** `section`: which part of Ajustes; "themes" is Apariencia at its theme gallery ("Cambiar tema…"). */
  | { screen: "settings"; section?: SettingsSection | "themes" }
  | { screen: "activity" }
  /**
   * `task`: open that task; `null` or unset: the project's home (a new task),
   * continuing conversation `carry` when given ("Llevar a un proyecto").
   */
  | { screen: "workspace"; project: string; task?: string | null; carry?: string }
  /** Memoria; `project`: "Para" that project (unset: everything); `tab`: which section. */
  | { screen: "memory"; project?: string; tab?: MemoryTab }
  | { screen: "status"; project: string }
  /** A project's settings ("Ajustes" on its home). */
  | { screen: "projectSettings"; project: string }
  /** A project's flows (the orchestrator); `flow`: the one open, by key. */
  | { screen: "flows"; project: string; flow?: string }
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
  return view.screen === "workspace" || view.screen === "memory" || view.screen === "status" || view.screen === "projectSettings" || view.screen === "flows"
    ? (view.project ?? null)
    : null;
}

export function defaultAccount(overview: Overview): Account | undefined {
  return overview.accounts.find((a) => a.id === overview.defaultAccount) ?? overview.accounts[0];
}


export function Sidebar({
  overview,
  view,
  onNavigate,
  onOpenPalette,
}: {
  overview: Overview;
  view: View;
  onNavigate: (v: View) => void;
  /** Opens the command palette (also Ctrl/⌘ K). */
  onOpenPalette?: () => void;
}) {
  const t = useT();
  const language = useLanguage();
  const { rail, toggle } = useSidebarRail();
  const terminals = useTerminals();
  const keys = paletteKeys();
  const active = viewProject(view);
  const inbox = useInbox();
  const needsYou = inbox.count;
  const create = useCreate();
  const menus = useSidebarMenus({ view, onNavigate });
  const conversations = useConversations().data ?? [];
  const [allConversations, setAllConversations] = useState(false);
  const shownConversations = allConversations ? conversations : conversations.slice(0, SIDEBAR_CONVERSATIONS);
  const openConversation = view.screen === "conversation" ? (view.task ?? null) : undefined;
  // Plan figures and how old they are (`PlanUsageProvider` keeps them fresh).
  const planBusy = usePlanRefresh().busy;
  const now = useNowSeconds(60_000);
  // In the rail, names are not on screen: they go in the accessible name and the tooltip.
  const named = (name: string) => (rail ? { "aria-label": name, title: name } : {});
  const home = view.screen === "welcome" || view.screen === "activity";

  // Notes and skills the AIs proposed, for Memoria's badge.
  const proposals = inbox.items.filter((entry) => entry.kind === "note" || entry.kind === "skill").length;

  // What waits for the user in each project, for its row.
  // Which projects show their latest tasks under their row.
  const openProjects = useOpenProjects(active);
  const pending = new Map<string, number>();
  for (const entry of inbox.items) {
    const path = entry.kind === "task" ? entry.item.task.project : entry.kind === "note" || entry.kind === "skill" ? entry.project : null;
    if (path) pending.set(path, (pending.get(path) ?? 0) + 1);
  }

  const project = overview.projects.find((p) => p.path === active);

  // The folder of what is on screen: the open task's copy, else its project, else home.
  const projectTasks = useTasks(active ?? "", !!active);
  const shownTask =
    view.screen === "workspace" && view.task ? projectTasks.data?.find((task) => task.id === view.task) : undefined;
  // Accepting or discarding deletes the task's copy: then its project.
  const here = (shownTask && isOpen(shownTask) ? shownTask.workspace?.worktree : undefined) ?? project?.path ?? null;

  // The account of what is on screen (the open task's, else its project's), else the default one.
  const account =
    overview.accounts.find((a) => a.id === (shownTask?.account ?? project?.account)) ?? defaultAccount(overview);

  // One terminal per folder, always in the one on screen: Ctrl/⌘ J, or the button at the bottom.
  const toggleTerminal = useCallback(() => {
    const mine = terminals.sessions.find((s) => s.request.kind === "shell" && s.request.path === here);
    if (!mine) {
      const name = shownTask ? shownTask.title : project?.name;
      terminals.open({ kind: "shell", path: here }, name ? `${t("common.terminal")} · ${name}` : t("common.terminal"));
    } else if (terminals.panelOpen && terminals.activeKey === mine.key) {
      terminals.setPanelOpen(false);
    } else {
      terminals.activate(mine.key);
      terminals.setPanelOpen(true);
    }
  }, [terminals, here, shownTask, project, t]);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!(e.ctrlKey || e.metaKey) || e.altKey || e.shiftKey) return;
      if (document.querySelector('[aria-modal="true"]')) return;
      const key = e.key.toLowerCase();
      if (key === "j") {
        e.preventDefault();
        toggleTerminal();
      } else if (key === "n") {
        // ⌘ on a Mac (its Ctrl+N moves the caret); never over a message being written.
        if (e.defaultPrevented || (modKey() === "⌘" ? !e.metaKey : !e.ctrlKey)) return;
        const typing = document.activeElement;
        if ((typing instanceof HTMLTextAreaElement || typing instanceof HTMLInputElement) && typing.value.trim()) return;
        e.preventDefault();
        // A new conversation in a conversation; else a new task in the project on screen, or the first one.
        if (view.screen === "conversation") return onNavigate({ screen: "conversation" });
        const target = project?.path ?? overview.projects[0]?.path;
        if (target) onNavigate({ screen: "workspace", project: target, task: null });
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [toggleTerminal, project, overview.projects, onNavigate, view.screen]);

  return (
    <aside id="sidebar" className={`sidebar${rail ? " sidebar--rail" : ""}`} aria-label={t("shell.sidebar")}>
      <div className="sidebar__top">
        <button className="brand" aria-label="Hedy" title={t("shell.home")} onClick={() => onNavigate({ screen: "welcome" })}>
          <Mascot
            color={account?.color ?? DEFAULT_ACCOUNT_COLOR}
            state={terminals.sessions.some((s) => s.running) ? "working" : "idle"}
            size={24}
            label=""
          />
          {!rail && <span className="brand__name">Hedy</span>}
        </button>
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
      </div>

      {onOpenPalette && (
        <button
          type="button"
          className="sidebar__search"
          aria-label={t("palette.open", { keys })}
          aria-keyshortcuts={keys === "⌘K" ? "Meta+K" : "Control+K"}
          title={t("palette.open", { keys })}
          onClick={onOpenPalette}
        >
          <SearchIcon />
          {!rail && (
            <>
              <span className="sidebar__search-text">{t("palette.hint")}</span>
              <kbd className="kbd">{keys}</kbd>
            </>
          )}
        </button>
      )}

      <nav className="sidebar__group" aria-label={t("shell.nav")}>
        <button
          className="sidebar__item sidebar__nav"
          aria-current={home ? "page" : undefined}
          title={rail ? t("shell.home") : undefined}
          aria-label={
            needsYou === 0
              ? t("shell.home")
              : needsYou === 1
                ? t("shell.homeOne")
                : t("shell.homeMany", { n: needsYou })
          }
          onClick={() => onNavigate({ screen: "welcome" })}
        >
          <HomeIcon />
          <span className="sidebar__name">{t("shell.home")}</span>
          {needsYou > 0 && (
            <span className="nav-badge" aria-hidden>
              {needsYou}
            </span>
          )}
        </button>

        <button
          className="sidebar__item sidebar__nav"
          aria-current={view.screen === "memory" ? "page" : undefined}
          {...named(t("ask.nav"))}
          // The memory of what is on screen: its project's, else everything.
          onClick={() => onNavigate({ screen: "memory", project: active ?? undefined })}
        >
          <MemoryIcon />
          <span className="sidebar__name">{t("ask.nav")}</span>
          {proposals > 0 && (
            <span className="nav-badge nav-badge--quiet" title={t("shell.memoryPending", { n: proposals })}>
              <span aria-hidden>{proposals}</span>
              <span className="sr-only">{t("shell.memoryPending", { n: proposals })}</span>
            </span>
          )}
        </button>
      </nav>

      <section className="sidebar__group">
        <div className={rail ? "sr-only" : "sidebar__head"}>
          <h2 className="sidebar__title">{t("common.projects")}</h2>
          {!rail && overview.accounts.length > 0 && (
            <MenuButton label={t("create.addProject")} items={create.menu.slice(0, 2)} className="menu-button sidebar__add">
              <PlusIcon />
            </MenuButton>
          )}
        </div>
        {/* No projects yet: the next step right there (with no account, the foot offers that first). */}
        {overview.projects.length === 0 &&
          (overview.accounts.length === 0 ? (
            !rail && <p className="hint sidebar__empty">{t("shell.noProjects")}</p>
          ) : (
            <>
              {!rail && <p className="hint sidebar__empty">{t("shell.noProjects")}</p>}
              <button type="button" className="sidebar__item" {...named(t("create.newProject"))} onClick={() => create.newProject()}>
                <PlusIcon />
                <span className="sidebar__name">{t("create.newProject")}</span>
              </button>
            </>
          ))}
        <ul className="sidebar__list">
          {overview.projects.map((p) => {
            const color = projectColor(p, overview.accounts);
            const waiting = pending.get(p.path) ?? 0;
            const unfolded = !rail && openProjects.isOpen(p.path);
            const listId = `sidebar-tasks-${p.path}`;
            return (
              <li key={p.path} className={rail ? "sidebar__row" : "sidebar__row sidebar__row--tree"}>
                {!rail && (
                  <button
                    type="button"
                    className="sidebar__toggle"
                    aria-expanded={unfolded}
                    aria-controls={unfolded ? listId : undefined}
                    aria-label={t(unfolded ? "shell.hideTasks" : "shell.showTasks", { name: p.name })}
                    title={t(unfolded ? "shell.hideTasks" : "shell.showTasks", { name: p.name })}
                    onClick={() => openProjects.toggle(p.path)}
                  >
                    <ChevronIcon />
                  </button>
                )}
                <button
                  className="sidebar__item"
                  aria-current={active === p.path ? "page" : undefined}
                  {...named(p.name)}
                  {...menus.trigger({ kind: "project", project: p })}
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
                      {p.protected && (
                        <span className="sidebar__lock" title={t("shell.protected")}>
                          <LockIcon />
                          <span className="sr-only">{t("shell.protected")}</span>
                        </span>
                      )}
                      {waiting > 0 && (
                        <span className="sidebar__count" title={t(waiting === 1 ? "shell.projectWaitingOne" : "shell.projectWaiting", { n: waiting })}>
                          <span aria-hidden>{waiting}</span>
                          <span className="sr-only">
                            {t(waiting === 1 ? "shell.projectWaitingOne" : "shell.projectWaiting", { n: waiting })}
                          </span>
                        </span>
                      )}
                    </>
                  )}
                </button>
                {!rail && (
                  <button
                    type="button"
                    className="sidebar__row-add"
                    aria-label={t("shell.newTaskIn", { name: p.name })}
                    title={t("shell.newTaskIn", { name: p.name })}
                    onClick={() => onNavigate({ screen: "workspace", project: p.path, task: null })}
                  >
                    <PlusIcon />
                  </button>
                )}
                {unfolded && (
                  <SidebarTasks
                    id={listId}
                    project={p.path}
                    current={view.screen === "workspace" && view.project === p.path ? (view.task ?? null) : null}
                    onOpen={(task) => onNavigate({ screen: "workspace", project: p.path, task })}
                    onAll={() => onNavigate({ screen: "workspace", project: p.path })}
                  />
                )}
              </li>
            );
          })}
        </ul>
      </section>

      {overview.accounts.length > 0 && (
        <section className="sidebar__group">
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
                  <PlusIcon />
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
                        {...menus.trigger({ kind: "chat", task })}
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

      <span className="spacer" />
      <div className="sidebar__foot">
        {overview.accounts.length === 0 ? (
          !rail && (
            // Quiet: Inicio has the one primary "Agregar mi primera cuenta".
            <Button small variant="ghost" onClick={() => onNavigate({ screen: "accounts" })}>
              {t("common.addAccount")}
            </Button>
          )
        ) : (
          account && <AccountUsage account={account} rail={rail} busy={planBusy.has(account.id)} now={now} language={language} onOpen={() => onNavigate({ screen: "accounts", account: account.id })} />
        )}
        <button
          type="button"
          className="sidebar__item"
          aria-pressed={terminals.panelOpen}
          aria-keyshortcuts={modKey() === "⌘" ? "Meta+J" : "Control+J"}
          {...named(t("common.terminal"))}
          onClick={toggleTerminal}
        >
          <TerminalIcon />
          <span className="sidebar__name">
            {t("common.terminal")}
            {terminals.sessions.length > 0 ? ` · ${terminals.sessions.length}` : ""}
          </span>
          {!rail && <kbd className="kbd">{`${modKey()} J`}</kbd>}
        </button>
        <button
          className="sidebar__item"
          aria-current={view.screen === "settings" || view.screen === "accounts" ? "page" : undefined}
          {...named(t("common.settings"))}
          onClick={() => onNavigate({ screen: "settings" })}
        >
          <SettingsIcon />
          <span className="sidebar__name">{t("common.settings")}</span>
        </button>
      </div>
      {menus.element}
    </aside>
  );
}

/** The rows' short names: the window is said in full on hover. */
const COMPACT_LABEL: Record<Exclude<UsageWindow["kind"], "other">, MessageKey> = {
  session: "shell.usage.session",
  week: "shell.usage.week",
  day: "shell.usage.day",
};

function compactLabel(w: UsageWindow, t: Translate): string {
  if (w.kind !== "other") return t(COMPACT_LABEL[w.kind]);
  return w.minutes !== null && w.minutes % 60 === 0 ? t("shell.usage.hours", { n: w.minutes / 60 }) : t("shell.usage.other");
}

/**
 * The plan of the account in use: its name, then a row per limit window (the
 * 5-hour session and the week), each with its meter and figure; on hover, which
 * window it is and when it renews. One meter when no window is known.
 */
function AccountUsage({
  account,
  rail,
  busy,
  now,
  language,
  onOpen,
}: {
  account: Account;
  rail: boolean;
  busy: boolean;
  now: number;
  language: Language;
  onOpen: () => void;
}) {
  const t = useT();
  const percent = planPercent(account);
  const usage = headlineText(account, t) ?? usageLabel(account, language);
  const plan = account.usage.plan.usage;
  const windows = plan ? sharedWindows(plan).slice(0, 2) : [];
  const windowTitle = (w: UsageWindow) =>
    [windowLabel(w, t), `${Math.round(w.percent)}%`, resetLabel(w, t, language, now * 1000)].filter(Boolean).join(" · ");
  const checkedAt = plan?.checkedAt;
  const age = checkedAt == null ? null : t("shell.usage.updated", { when: timeAgo(checkedAt, t, now) });
  const title =
    windows.length > 0
      ? [account.name, ...windows.map(windowTitle), age].filter(Boolean).join("\n")
      : usageTitle(account, t, language, now);
  return (
    <button
      type="button"
      className="sidebar__usage"
      title={[title, busy && t("shell.usage.refreshing")].filter(Boolean).join(" · ")}
      aria-label={t("shell.openAccount", { name: account.name, usage: windows.length > 0 ? windows.map((w) => windowText(w, t)).join(", ") : usage })}
      aria-busy={busy || undefined}
      onClick={onOpen}
    >
      <span className="sidebar__usage-top">
        <span className="dot" style={{ background: account.color }} aria-hidden />
        {!rail && <span className="sidebar__usage-name">{account.name}</span>}
        {/* The figure goes on each row; folded, the one closest to its limit. */}
        {percent !== null && (rail || windows.length === 0) && (
          <span className={["sidebar__usage-pct", levelClass(percent), busy && "plan-figure--busy"].filter(Boolean).join(" ")}>
            {Math.round(percent)}%
          </span>
        )}
      </span>
      {!rail && windows.length > 0 && (
        <span className="sidebar__usage-windows">
          {windows.map((w) => (
            <span key={`${w.kind}-${w.minutes ?? ""}`} className="sidebar__usage-window" title={windowTitle(w)}>
              <span className="sidebar__usage-label">{compactLabel(w, t)}</span>
              <Meter percent={w.percent} label={windowText(w, t)} />
              <span className={["sidebar__usage-pct", levelClass(w.percent), busy && "plan-figure--busy"].filter(Boolean).join(" ")}>
                {Math.round(w.percent)}%
              </span>
            </span>
          ))}
        </span>
      )}
      {!rail && windows.length === 0 && percent !== null && <Meter percent={percent} label={usage} />}
    </button>
  );
}

/** In focus mode the sidebar steps aside: this is the way back (also Ctrl/⌘ \). */
function FocusExit() {
  const t = useT();
  const focusMode = useFocusMode();
  if (!focusMode.focus) return null;
  return (
    <button
      type="button"
      className="focus-exit"
      aria-label={t("shell.focus.exit")}
      aria-keyshortcuts="Control+Backslash"
      title={`${t("shell.focus.exit")} (${modKey()} \\)`}
      onClick={focusMode.toggle}
    >
      {t("shell.focus.on")}
      <kbd className="kbd">{`${modKey()} \\`}</kbd>
    </button>
  );
}

interface ShellParts {
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

function ShellFrame({ sidebar, children, terminal, statusbar }: ShellParts) {
  const t = useT();
  const { rail } = useSidebarRail();
  const { focus } = useFocusMode();
  return (
    <div className={`app${rail ? " app--rail" : ""}${focus ? " app--focus" : ""}`}>
      {/* The first stop of Tab. Not a plain "#main" link: the hash is the app's route. */}
      <a
        className="skip-link"
        href="#main"
        onClick={(e) => {
          e.preventDefault();
          document.getElementById("main")?.focus();
        }}
      >
        {t("shell.skipToContent")}
      </a>
      {sidebar}
      <FocusExit />
      <main className="main" id="main" tabIndex={-1}>
        {children}
      </main>
      {terminal}
      {statusbar}
    </div>
  );
}

