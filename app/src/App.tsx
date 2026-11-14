import { useCallback, useEffect, useState, type ReactNode } from "react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

import { ApiProvider, errorMessage, useApi, useOverview } from "./api/context";
import type { HedyApi } from "./api/HedyApi";
import type { TaskMode } from "./api/types";
import { Shell, Sidebar, Topbar, viewProject, type View } from "./components/Layout";
import { Mascot } from "./components/Mascot/Mascot";
import { TerminalPanel } from "./components/Terminal/TerminalPanel";
import { TerminalProvider } from "./components/Terminal/TerminalProvider";
import { Button } from "./components/ui/Button";
import { ToastProvider } from "./components/ui/Toast";
import { AccountsScreen } from "./screens/accounts/AccountsScreen";
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
            <Root />
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
 * `#accounts`, `#settings`, `#activity`, `#p=<path>` (workspace), `#m=<path>`
 * (memory), `#s=<path>` (status).
 */
function viewFromHash(): View {
  const hash = decodeURIComponent(window.location.hash.slice(1));
  if (hash === "accounts" || hash === "settings" || hash === "activity") return { screen: hash };
  if (hash.startsWith("p=")) return { screen: "workspace", project: hash.slice(2) };
  if (hash.startsWith("m=")) return { screen: "memory", project: hash.slice(2) };
  if (hash.startsWith("s=")) return { screen: "status", project: hash.slice(2) };
  return { screen: "welcome" };
}

function hashFor(view: View): string {
  if (view.screen === "workspace") return `#p=${encodeURIComponent(view.project)}`;
  if (view.screen === "memory") return `#m=${encodeURIComponent(view.project)}`;
  if (view.screen === "status") return `#s=${encodeURIComponent(view.project)}`;
  return `#${view.screen}`;
}

function Root() {
  const overview = useOverview();
  const t = useT();
  const [view, setView] = useState<View>(viewFromHash);
  const [openProjects, setOpenProjects] = useState<string[]>(() => {
    const project = viewProject(view);
    return project ? [project] : [];
  });
  const [mode, setMode] = useState<TaskMode>("project");
  const api = useApi();

  const navigate = useCallback((next: View) => {
    setView(next);
    const project = viewProject(next);
    if (project) setOpenProjects((open) => (open.includes(project) ? open : [...open, project]));
    history.replaceState(null, "", hashFor(next));
  }, []);

  // A click on a desktop notification opens that task.
  useEffect(
    () => api.onOpenTask(({ taskId, project }) => navigate({ screen: "workspace", project, task: taskId })),
    [api, navigate],
  );

  const closeProject = (path: string) => {
    const rest = openProjects.filter((p) => p !== path);
    setOpenProjects(rest);
    if (viewProject(view) === path) {
      const last = rest.at(-1);
      navigate(last ? { screen: "workspace", project: last } : { screen: "welcome" });
    }
  };

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
        <Shell
          topbar={
            <Topbar
              overview={data}
              view={current}
              openProjects={openProjects}
              mode={mode}
              onNavigate={navigate}
              onCloseProject={closeProject}
              onMode={setMode}
            />
          }
          sidebar={<Sidebar overview={data} view={current} onNavigate={navigate} />}
          terminal={<TerminalPanel />}
        >
          {current.screen === "welcome" && <Welcome overview={data} onNavigate={navigate} />}
          {current.screen === "accounts" && <AccountsScreen overview={data} />}
          {current.screen === "settings" && <Settings overview={data} />}
          {current.screen === "activity" && (
            <ActivityScreen
              overview={data}
              onOpenTask={(project, task) => navigate({ screen: "workspace", project, task })}
            />
          )}
          {current.screen === "workspace" && (
            <Workspace
              key={`${current.project}\n${current.task ?? ""}`}
              overview={data}
              project={current.project}
              initialTask={current.task}
              mode={mode}
              onOpenMemory={() => navigate({ screen: "memory", project: current.project })}
              onOpenStatus={() => navigate({ screen: "status", project: current.project })}
            />
          )}
          {current.screen === "status" && (
            <StatusScreen
              key={current.project}
              overview={data}
              project={current.project}
              onOpenChat={() => navigate({ screen: "workspace", project: current.project })}
              onOpenMemory={() => navigate({ screen: "memory", project: current.project })}
            />
          )}
          {current.screen === "memory" && (
            <MemoryScreen
              key={current.project}
              overview={data}
              project={current.project}
              onBack={() => navigate({ screen: "workspace", project: current.project })}
            />
          )}
        </Shell>
      </TerminalProvider>
    </ThemeProvider>
  );
}
