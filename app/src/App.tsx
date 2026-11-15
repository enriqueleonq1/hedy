import { useCallback, useEffect, useMemo, useState, type ReactNode } from "react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

import { ApiProvider, errorMessage, useApi, useOverview } from "./api/context";
import type { HedyApi } from "./api/HedyApi";
import { FocusModeProvider } from "./components/focusMode";
import { FileNavigatorProvider } from "./files/navigator";
import { ReviewCommentsProvider } from "./review/comments";
import { Shell, Sidebar, taskView, Topbar, viewProject, type View } from "./components/Layout";
import { Mascot } from "./components/Mascot/Mascot";
import { CommandPalette } from "./components/palette/CommandPalette";
import { ShortcutsProvider, useProjectShortcuts } from "./components/shortcuts";
import { StatusBar } from "./components/statusbar/StatusBar";
import { TerminalPanel } from "./components/Terminal/TerminalPanel";
import { TerminalProvider } from "./components/Terminal/TerminalProvider";
import { Button } from "./components/ui/Button";
import { ToastProvider } from "./components/ui/Toast";
import { AccountsScreen } from "./screens/accounts/AccountsScreen";
import { CreateProvider } from "./screens/create/CreateProvider";
import { ActivityScreen } from "./screens/activity/ActivityScreen";
import { MemoryScreen } from "./screens/memory/MemoryScreen";
import { Onboarding } from "./screens/onboarding/Onboarding";
import { Settings } from "./screens/Settings";
import { StatusScreen } from "./screens/status/StatusScreen";
import { Welcome } from "./screens/Welcome";
import { Workspace } from "./screens/workspace/Workspace";
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
            <FileNavigatorProvider>
              <ReviewCommentsProvider>
                <Root />
              </ReviewCommentsProvider>
            </FileNavigatorProvider>
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
 * `#accounts`, `#a=<id>` (accounts, that card), `#settings`, `#activity`, `#p=<path>` (workspace; `&t=<id>`
 * that task, `&new` a new one), `#m=<path>` (memory), `#k=<path>` (memory, skills tab), `#s=<path>` (status),
 * `#c` (a new quick question), `#c=<id>` (that conversation).
 */
function viewFromHash(): View {
  const raw = window.location.hash.slice(1);
  if (raw.startsWith("p=")) {
    // The path is encoded, so a raw `&` starts the task part.
    const [path = "", extra] = raw.slice(2).split("&");
    const project = decodeURIComponent(path);
    if (extra === "new") return { screen: "workspace", project, task: null };
    if (extra?.startsWith("t=")) return { screen: "workspace", project, task: decodeURIComponent(extra.slice(2)) };
    return { screen: "workspace", project };
  }
  const hash = decodeURIComponent(raw);
  if (hash === "accounts" || hash === "settings" || hash === "activity") return { screen: hash };
  if (hash === "c") return { screen: "conversation" };
  if (hash.startsWith("c=")) return { screen: "conversation", task: hash.slice(2) };
  if (hash.startsWith("a=")) return { screen: "accounts", account: hash.slice(2) };
  if (hash.startsWith("m=")) return { screen: "memory", project: hash.slice(2) };
  if (hash.startsWith("k=")) return { screen: "memory", project: hash.slice(2), tab: "skills" };
  if (hash.startsWith("s=")) return { screen: "status", project: hash.slice(2) };
  return { screen: "welcome" };
}

function hashFor(view: View): string {
  if (view.screen === "workspace") {
    const task = view.task === null ? "&new" : view.task ? `&t=${encodeURIComponent(view.task)}` : "";
    return `#p=${encodeURIComponent(view.project)}${task}`;
  }
  if (view.screen === "memory") return `#${view.tab === "skills" ? "k" : "m"}=${encodeURIComponent(view.project)}`;
  if (view.screen === "status") return `#s=${encodeURIComponent(view.project)}`;
  if (view.screen === "accounts" && view.account) return `#a=${encodeURIComponent(view.account)}`;
  if (view.screen === "conversation") return view.task ? `#c=${encodeURIComponent(view.task)}` : "#c";
  return `#${view.screen}`;
}

function Root() {
  const overview = useOverview();
  const t = useT();
  const [view, setView] = useState<View>(viewFromHash);
  const [palette, setPalette] = useState(false);
  const api = useApi();

  // Each screen and task is a step of the history: Back and Forward walk them.
  const navigate = useCallback(
    (next: View, options?: { replace?: boolean }) => {
      setView(next);
      const hash = hashFor(next);
      if (options?.replace) history.replaceState(null, "", hash);
      else if (hash !== window.location.hash) history.pushState(null, "", hash);
    },
    [],
  );

  useEffect(() => {
    const onPop = () => setView(viewFromHash());
    window.addEventListener("popstate", onPop);
    return () => window.removeEventListener("popstate", onPop);
  }, []);

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

  if (overview.isPending) {
    return (
      <div className="splash">
        <Mascot color={DEFAULT_ACCOUNT_COLOR} state="thinking" size={140} label={t("common.loading")} />
      </div>
    );
  }
  if (overview.isError) {
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
      <ThemeProvider setting={data.theme}>
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
    <ThemeProvider setting={data.theme}>
      <TerminalProvider>
        <CreateProvider overview={data} onNavigate={navigate}>
        <FocusModeProvider>
        <ShortcutsProvider>
        <Shell
          topbar={
            <Topbar
              overview={data}
              view={current}
              onNavigate={navigate}
              onOpenPalette={() => setPalette(true)}
            />
          }
          sidebar={<Sidebar overview={data} view={current} onNavigate={navigate} />}
          terminal={<TerminalPanel />}
          statusbar={<StatusBar overview={data} view={current} onNavigate={navigate} />}
        >
          {current.screen === "welcome" && <Welcome overview={data} onNavigate={navigate} />}
          {current.screen === "accounts" && <AccountsScreen overview={data} focus={current.account} />}
          {current.screen === "settings" && <Settings overview={data} />}
          {current.screen === "activity" && (
            <ActivityScreen
              overview={data}
              onOpenTask={openTask}
              onOpenMemory={(project, tab) => navigate({ screen: "memory", project, tab })}
            />
          )}
          {current.screen === "workspace" && (
            <Workspace
              key={current.project}
              overview={data}
              project={current.project}
              task={current.task}
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
              onOpenSettings={() => navigate({ screen: "settings" })}
            />
          )}
          {current.screen === "memory" && (
            <MemoryScreen
              key={current.project}
              overview={data}
              project={current.project}
              initialTab={current.tab}
              onTab={(tab) => navigate({ screen: "memory", project: current.project, tab })}
              onBack={() => navigate({ screen: "workspace", project: current.project })}
            />
          )}
        </Shell>
        <CommandPalette open={palette} onOpenChange={setPalette} overview={data} view={current} onNavigate={navigate} />
        </ShortcutsProvider>
        </FocusModeProvider>
        </CreateProvider>
      </TerminalProvider>
    </ThemeProvider>
  );
}
