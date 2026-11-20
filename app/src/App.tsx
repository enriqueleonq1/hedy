import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

import { ApiProvider, errorMessage, useApi, useOverview } from "./api/context";
import type { HedyApi } from "./api/HedyApi";
import { FocusModeProvider } from "./components/focusMode";
import { UnsavedChangesProvider, useUnsavedChanges } from "./code/editor/unsaved";
import { FileNavigatorProvider } from "./files/navigator";
import { ReviewCommentsProvider } from "./review/comments";
import { Shell, Sidebar, taskView, viewProject, type MemoryTab, type SettingsSection, type View } from "./components/Layout";
import { Mascot } from "./components/Mascot/Mascot";
import { CommandPalette } from "./components/palette/CommandPalette";
import { PlanUsageProvider } from "./components/planUsage";
import { ShortcutsProvider, useProjectShortcuts } from "./components/shortcuts";
import { StatusBar } from "./components/statusbar/StatusBar";
import { TerminalPanel } from "./components/Terminal/TerminalPanel";
import { TerminalProvider } from "./components/Terminal/TerminalProvider";
import { Button } from "./components/ui/Button";
import { ToastProvider } from "./components/ui/Toast";
import { CreateProvider } from "./screens/create/CreateProvider";
import { MemoryHub } from "./screens/memory/MemoryHub";
import { ProjectSettings } from "./screens/project/ProjectSettings";
import { BackgroundTrialsProvider } from "./screens/skills/backgroundTrials";
import { Onboarding } from "./screens/onboarding/Onboarding";
import { Settings } from "./screens/Settings";
import { ThemeStudio } from "./screens/themes/ThemeStudio";
import { StatusScreen } from "./screens/status/StatusScreen";
import { HomeScreen } from "./screens/home/HomeScreen";
import { Workspace } from "./screens/workspace/Workspace";
import { useSyncEvents } from "./sync/hooks";
import { TaskEventsBridge } from "./tasks/hooks";
import { ThemeProvider } from "./theme/ThemeProvider";
import { I18nProvider, useT } from "./i18n";
import { DEFAULT_ACCOUNT_COLOR, ERROR_MASCOT_COLOR } from "./theme/tokens";

export function App({ api, queryClient }: { api: HedyApi; queryClient?: QueryClient }) {
  const [client] = useState(
    () =>
      queryClient ??
      new QueryClient({ defaultOptions: { queries: { refetchOnWindowFocus: true, staleTime: 15_000 } } }),
  );
  return (
    <QueryClientProvider client={client}>
      <ApiProvider api={api}>
        <Localized>
          <ToastProvider>
            <TaskEventsBridge />
            <BackgroundTrialsProvider>
              <FileNavigatorProvider>
                <ReviewCommentsProvider>
                  <UnsavedChangesProvider>
                    <Root />
                  </UnsavedChangesProvider>
                </ReviewCommentsProvider>
              </FileNavigatorProvider>
            </BackgroundTrialsProvider>
          </ToastProvider>
        </Localized>
      </ApiProvider>
    </QueryClientProvider>
  );
}

/**
 * The language the user chose (the system's until the settings load), around
 * everything that shows text, toasts included.
 */
function Localized({ children }: { children: ReactNode }) {
  const overview = useOverview();
  return <I18nProvider setting={overview.data?.language ?? "system"}>{children}</I18nProvider>;
}

/**
 * `#accounts`, `#a=<id>` (accounts, that card), `#settings` (`/<section>` one of Ajustes' sections, `/themes` Apariencia at the
 * theme gallery), `#sync` (its sync card), `#activity`, `#p=<path>` (workspace; `&t=<id>`
 * that task, `&new` a new one, `&new&from=<id>` a new one continuing that conversation), `#memory` (Memoria, for everything;
 * `#memory=<tab>` that section), `#m=<path>` (Memoria for that project; `&<tab>` that section), `#s=<path>` (status),
 * `#c` (a new quick question), `#c=<id>` (that conversation). The old `#k=<path>`, `#skills`, `#ask` and `#ask=<path>` land in Memoria.
 */
const MEMORY_TABS: MemoryTab[] = ["notes", "skills", "pending", "map"];
const SETTINGS_SECTIONS: SettingsSection[] = ["general", "accounts", "appearance", "saving", "templates", "sync", "teams", "advanced"];
const isSettingsSection = (s: string): s is SettingsSection => (SETTINGS_SECTIONS as string[]).includes(s);
const isMemoryTab = (tab: string | undefined): tab is MemoryTab => !!tab && (MEMORY_TABS as string[]).includes(tab);

function viewFromHash(): View {
  const raw = window.location.hash.slice(1);
  if (raw.startsWith("p=")) {
    // The path is encoded, so a raw `&` starts the task part.
    const [path = "", extra, more] = raw.slice(2).split("&");
    const project = decodeURIComponent(path);
    if (extra === "new" && more?.startsWith("from=")) {
      return { screen: "workspace", project, task: null, carry: decodeURIComponent(more.slice(5)) };
    }
    if (extra === "new") return { screen: "workspace", project, task: null };
    if (extra?.startsWith("t=")) return { screen: "workspace", project, task: decodeURIComponent(extra.slice(2)) };
    return { screen: "workspace", project };
  }
  if (raw.startsWith("m=")) {
    const [path = "", tab] = raw.slice(2).split("&");
    return { screen: "memory", project: decodeURIComponent(path), ...(isMemoryTab(tab) ? { tab } : {}) };
  }
  const hash = decodeURIComponent(raw);
  // Actividad lives in Inicio now: its old address still lands there.
  if (hash === "activity") return { screen: "welcome" };
  if (hash === "accounts" || hash === "settings") return { screen: hash };
  if (hash.startsWith("settings/")) {
    const section = hash.slice(9);
    // "themes" is Apariencia at its theme gallery.
    return { screen: "settings", ...(isSettingsSection(section) || section === "themes" ? { section } : {}) };
  }
  // Memoria, and the addresses its parts had before.
  if (hash === "memory" || hash === "ask") return { screen: "memory" };
  if (hash.startsWith("memory=")) {
    const tab = hash.slice(7);
    return { screen: "memory", ...(isMemoryTab(tab) ? { tab } : {}) };
  }
  if (hash === "skills") return { screen: "memory", tab: "skills" };
  if (hash.startsWith("ask=")) return { screen: "memory", project: hash.slice(4) };
  if (hash === "sync") return { screen: "settings", section: "sync" };
  if (hash === "teams") return { screen: "settings", section: "teams" };
  if (hash === "c") return { screen: "conversation" };
  if (hash.startsWith("c=")) return { screen: "conversation", task: hash.slice(2) };
  if (hash.startsWith("a=")) return { screen: "accounts", account: hash.slice(2) };
  if (hash.startsWith("k=")) return { screen: "memory", project: hash.slice(2), tab: "skills" };
  if (hash.startsWith("s=")) return { screen: "status", project: hash.slice(2) };
  if (hash.startsWith("ps=")) return { screen: "projectSettings", project: hash.slice(3) };
  return { screen: "welcome" };
}

function hashFor(view: View): string {
  if (view.screen === "workspace") {
    const from = view.task === null && view.carry ? `&from=${encodeURIComponent(view.carry)}` : "";
    const task = view.task === null ? `&new${from}` : view.task ? `&t=${encodeURIComponent(view.task)}` : "";
    return `#p=${encodeURIComponent(view.project)}${task}`;
  }
  if (view.screen === "memory") {
    const tab = view.tab && view.tab !== "notes" ? view.tab : null;
    if (!view.project) return tab ? `#memory=${tab}` : "#memory";
    return `#m=${encodeURIComponent(view.project)}${tab ? `&${tab}` : ""}`;
  }
  if (view.screen === "status") return `#s=${encodeURIComponent(view.project)}`;
  if (view.screen === "projectSettings") return `#ps=${encodeURIComponent(view.project)}`;
  if (view.screen === "accounts" && view.account) return `#a=${encodeURIComponent(view.account)}`;
  if (view.screen === "conversation") return view.task ? `#c=${encodeURIComponent(view.task)}` : "#c";
  if (view.screen === "settings" && view.section === "sync") return "#sync";
  if (view.screen === "settings" && view.section === "teams") return "#teams";
  if (view.screen === "settings" && view.section && view.section !== "general") {
    return `#settings/${view.section === "themes" ? "appearance" : view.section}`;
  }
  return `#${view.screen}`;
}

/** A dialog that must finish first is open (the recovery words, a setup under way). */
function lockedByDialog(): boolean {
  return document.querySelector('[data-locked="true"]') !== null;
}

function Root() {
  const overview = useOverview();
  const t = useT();
  const [view, setView] = useState<View>(viewFromHash);
  const [palette, setPalette] = useState(false);
  const api = useApi();
  const unsaved = useUnsavedChanges();

  // Each screen and task is a step of the history: Back and Forward walk them.
  // Leaving a screen with an unsaved edit asks first (Guardar / Descartar / Cancelar).
  const navigate = useCallback(
    (next: View, options?: { replace?: boolean }) => {
      const go = () => {
        setView(next);
        const hash = hashFor(next);
        if (options?.replace) history.replaceState(null, "", hash);
        else if (hash !== window.location.hash) history.pushState(null, "", hash);
      };
      if (lockedByDialog()) return;
      if (!unsaved.dirty) return go();
      void unsaved.confirmLeave().then((ok) => ok && go());
    },
    [unsaved],
  );

  // The view on screen, to step back to when Back is answered with "Cancelar".
  const shown = useRef(view);
  useEffect(() => {
    shown.current = view;
  }, [view]);
  useEffect(() => {
    const onPop = () => {
      const next = viewFromHash();
      if (lockedByDialog()) return history.pushState(null, "", hashFor(shown.current));
      if (!unsaved.dirty) return setView(next);
      // The URL already moved: put it back while asking.
      const target = window.location.hash;
      history.pushState(null, "", hashFor(shown.current));
      void unsaved.confirmLeave().then((ok) => {
        if (!ok) return;
        history.replaceState(null, "", target);
        setView(next);
      });
    };
    window.addEventListener("popstate", onPop);
    return () => window.removeEventListener("popstate", onPop);
  }, [unsaved]);

  // A task opens in its project or, outside any project, as a conversation.
  const projects = overview.data?.projects;
  const openTask = useCallback(
    (project: string, task: string) => navigate(taskView(projects ?? [], project, task)),
    [navigate, projects],
  );

  // A click on a desktop notification opens that task.
  useEffect(() => api.onOpenTask(({ taskId, project }) => openTask(project, taskId)), [api, openTask]);

  // Alt/⌥ 1…9: the Nth project of the sidebar (once onboarded).
  const shortcutProjects = useMemo(
    () => (overview.data?.onboarded ? (projects ?? []) : []),
    [overview.data?.onboarded, projects],
  );
  const goToProject = useCallback((path: string) => navigate({ screen: "workspace", project: path }), [navigate]);
  useProjectShortcuts(shortcutProjects, goToProject);
  useSyncEvents();

  if (overview.isPending) {
    return (
      <div className="splash">
        <Mascot color={DEFAULT_ACCOUNT_COLOR} state="thinking" size={140} label={t("common.loading")} />
      </div>
    );
  }
  // A failed refresh keeps what is on screen (a dialog may hold something
  // that must finish, like the recovery words); only a first load fails here.
  if (!overview.data) {
    return (
      <div className="splash">
        <Mascot color={ERROR_MASCOT_COLOR} state="sleeping" size={140} />
        <p role="alert">{errorMessage(overview.error, t)}</p>
        <Button onClick={() => overview.refetch()}>{t("common.retry")}</Button>
      </div>
    );
  }

  const data = overview.data;
  // A project removed elsewhere falls back to the welcome screen.
  const project = viewProject(view);
  const current: View = project && !data.projects.some((p) => p.path === project) ? { screen: "welcome" } : view;

  if (!data.onboarded) {
    return (
      <ThemeProvider setting={data.theme} dark={data.themeDark} light={data.themeLight}>
        <TerminalProvider>
          <Onboarding
            overview={data}
            onFinish={(added) => navigate(added ? { screen: "workspace", project: added } : { screen: "welcome" })}
          />
        </TerminalProvider>
      </ThemeProvider>
    );
  }

  return (
    <ThemeProvider setting={data.theme} dark={data.themeDark} light={data.themeLight}>
      <TerminalProvider>
        <CreateProvider overview={data} onNavigate={navigate}>
        <FocusModeProvider>
        <ShortcutsProvider>
        {/* The sidebar and status bar show every account's plan usage: keep it fresh while they are on screen. */}
        <PlanUsageProvider accounts={data.accounts} screen={current.screen}>
        {/* Theme previews and the dialogs that make themes, over every screen. */}
        <ThemeStudio overview={data}>
        <Shell
          sidebar={<Sidebar overview={data} view={current} onNavigate={navigate} onOpenPalette={() => setPalette(true)} />}
          terminal={<TerminalPanel />}
          statusbar={
            current.screen === "workspace" || current.screen === "conversation" ? (
              <StatusBar view={current} onNavigate={navigate} />
            ) : undefined
          }
        >
          {(current.screen === "welcome" || current.screen === "activity") && (
            <HomeScreen overview={data} onNavigate={navigate} onOpenTask={openTask} />
          )}
          {(current.screen === "settings" || current.screen === "accounts") && (
            <Settings
              overview={data}
              section={
                current.screen === "accounts" ? "accounts" : current.section === "themes" ? "appearance" : (current.section ?? "general")
              }
              account={current.screen === "accounts" ? current.account : undefined}
              gallery={current.screen === "settings" && current.section === "themes"}
              onSection={(section) => navigate(section === "accounts" ? { screen: "accounts" } : { screen: "settings", section })}
            />
          )}
          {current.screen === "workspace" && (
            <Workspace
              key={current.project}
              overview={data}
              project={current.project}
              task={current.task}
              carry={current.task === null ? current.carry : undefined}
              onNavigate={navigate}
              onSelectTask={(task, options) => navigate({ screen: "workspace", project: current.project, task }, options)}
              onOpenMemory={() => navigate({ screen: "memory", project: current.project })}
              onOpenStatus={() => navigate({ screen: "status", project: current.project })}
            />
          )}
          {current.screen === "conversation" && (
            <Workspace
              key="conversation"
              overview={data}
              project={null}
              task={current.task ?? null}
              draft={current.draft}
              onNavigate={navigate}
              onSelectTask={(task, options) =>
                navigate(task ? { screen: "conversation", task } : { screen: "conversation" }, options)
              }
            />
          )}
          {current.screen === "status" && (
            <StatusScreen
              key={current.project}
              overview={data}
              project={current.project}
              onOpenChat={() => navigate({ screen: "workspace", project: current.project })}
              onOpenMemory={() => navigate({ screen: "memory", project: current.project })}
              onOpenSettings={() => navigate({ screen: "settings", section: "saving" })}
            />
          )}
          {current.screen === "projectSettings" && (
            <ProjectSettings key={current.project} overview={data} project={current.project} onNavigate={navigate} />
          )}
          {current.screen === "memory" && (
            <MemoryHub overview={data} project={current.project} tab={current.tab} onNavigate={navigate} />
          )}
        </Shell>
        <CommandPalette open={palette} onOpenChange={setPalette} overview={data} view={current} onNavigate={navigate} />
        </ThemeStudio>
        </PlanUsageProvider>
        </ShortcutsProvider>
        </FocusModeProvider>
        </CreateProvider>
      </TerminalProvider>
    </ThemeProvider>
  );
}
