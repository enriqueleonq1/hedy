import { useState } from "react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

import { ApiProvider, errorMessage, useOverview } from "./api/context";
import type { HedyApi } from "./api/HedyApi";
import { SCREENS, Shell, Sidebar, Topbar, type Screen } from "./components/Layout";
import { Mascot } from "./components/Mascot/Mascot";
import { TerminalPanel } from "./components/Terminal/TerminalPanel";
import { TerminalProvider } from "./components/Terminal/TerminalProvider";
import { Button } from "./components/ui/Button";
import { ToastProvider } from "./components/ui/Toast";
import { AccountsScreen } from "./screens/accounts/AccountsScreen";
import { Settings } from "./screens/Settings";
import { Welcome } from "./screens/Welcome";
import { ThemeProvider } from "./theme/ThemeProvider";

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
          <Root />
        </ToastProvider>
      </ApiProvider>
    </QueryClientProvider>
  );
}

function Root() {
  const overview = useOverview();
  const [screen, setScreenState] = useState<Screen>(screenFromHash);
  const setScreen = (next: Screen) => {
    setScreenState(next);
    history.replaceState(null, "", `#${next}`);
  };

  if (overview.isPending) {
    return (
      <div className="splash">
        <Mascot color="#7AA2F7" state="thinking" size={140} label="Cargando" />
      </div>
    );
  }
  if (overview.isError) {
    return (
      <div className="splash">
        <Mascot color="#F7768E" state="sleeping" size={140} />
        <p role="alert">{errorMessage(overview.error)}</p>
        <Button onClick={() => overview.refetch()}>Reintentar</Button>
      </div>
    );
  }

  const data = overview.data;
  return (
    <ThemeProvider setting={data.theme}>
      <TerminalProvider>
        <Shell
          topbar={<Topbar overview={data} screen={screen} onNavigate={setScreen} />}
          sidebar={<Sidebar overview={data} onNavigate={setScreen} />}
          terminal={<TerminalPanel />}
        >
          {screen === "welcome" && <Welcome overview={data} onNavigate={setScreen} />}
          {screen === "accounts" && <AccountsScreen overview={data} />}
          {screen === "settings" && <Settings overview={data} />}
        </Shell>
      </TerminalProvider>
    </ThemeProvider>
  );
}

function screenFromHash(): Screen {
  const hash = window.location.hash.slice(1);
  return SCREENS.find((s) => s.screen === hash)?.screen ?? "welcome";
}
