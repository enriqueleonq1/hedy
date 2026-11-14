import { useState } from "react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

import { ApiProvider, errorMessage, useOverview } from "./api/context";
import type { HedyApi } from "./api/HedyApi";
import type { TaskMode } from "./api/types";
import { Shell, Sidebar, Topbar, viewProject, type View } from "./components/Layout";
import { Mascot } from "./components/Mascot/Mascot";
import { TerminalPanel } from "./components/Terminal/TerminalPanel";
import { TerminalProvider } from "./components/Terminal/TerminalProvider";
import { Button } from "./components/ui/Button";
import { ToastProvider } from "./components/ui/Toast";
import { AccountsScreen } from "./screens/accounts/AccountsScreen";
import { MemoryScreen } from "./screens/memory/MemoryScreen";
import { Settings } from "./screens/Settings";
import { StatusScreen } from "./screens/status/StatusScreen";
import { Welcome } from "./screens/Welcome";
import { Workspace } from "./screens/workspace/Workspace";
import { TaskEventsBridge } from "./tasks/hooks";
import { ThemeProvider } from "./theme/ThemeProvider";
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
        <ToastProvider>
          <TaskEventsBridge />
          <Root />
        </ToastProvider>
      </ApiProvider>
    </QueryClientProvider>
  );
}

/** `#accounts`, `#settings`, `#p=<path>` (workspace), `#m=<path>` (memory), `#s=<path>` (status). */
function viewFromHash(): View {
  const hash = decodeURIComponent(window.location.hash.slice(1));
  if (hash === "accounts" || hash === "settings") return { screen: hash };
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
  const [view, setView] = useState<View>(viewFromHash);
  const [openProjects, setOpenProjects] = useState<string[]>(() => {
    const project = viewProject(view);
    return project ? [project] : [];
  });
  const [mode, setMode] = useState<TaskMode>("project");

  const navigate = (next: View) => {
    setView(next);
    const project = viewProject(next);
    if (project) setOpenProjects((open) => (open.includes(project) ? open : [...open, project]));
    history.replaceState(null, "", hashFor(next));
  };

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
        <Mascot color={DEFAULT_ACCOUNT_COLOR} state="thinking" size={140} label="Cargando" />
      </div>
    );
  }
  if (overview.isError) {
    return (
      <div className="splash">
        <Mascot color={ERROR_MASCOT_COLOR} state="sleeping" size={140} />
        <p role="alert">{errorMessage(overview.error)}</p>
        <Button onClick={() => overview.refetch()}>Reintentar</Button>
      </div>
    );
  }

  const data = overview.data;
  // A project removed elsewhere falls back to the welcome screen.
  const project = viewProject(view);
  const current: View = project && !data.projects.some((p) => p.path === project) ? { screen: "welcome" } : view;

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
          {current.screen === "workspace" && (
            <Workspace
              key={current.project}
              overview={data}
              project={current.project}
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
