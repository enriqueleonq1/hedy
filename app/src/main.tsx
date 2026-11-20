import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "@fontsource-variable/geist";
import "@fontsource-variable/geist-mono";
import "@fontsource-variable/sora";

import { App } from "./App";
import type { HedyApi } from "./api/HedyApi";
import { MockApi } from "./api/MockApi";
import { isTauri, tauriApi } from "./api/tauriApi";
import { dataMode, failingApi } from "./devData";
import "./styles/global.css";
import "./styles/shell.css";
import "./styles/skills.css";
import { applyTheme } from "./theme/tokens";

// Dark tokens until the user's theme setting loads.
applyTheme("dark");

// In a plain browser (`npm run dev`) the UI runs against sample data, with a
// few running and recent tasks on the board (`#activity`; `?activity=none`
// leaves it empty); `?theme=light|dark` forces a theme there.
// `?data=empty` starts with no accounts and no projects, `?data=noprojects`
// with the sample accounts and no projects, and `?data=fail` with every read
// but the overview failing, for the empty and error states (src/devData.ts).
async function browserApi(): Promise<HedyApi> {
  const data = dataMode(window.location.search);
  const api = data === "empty" ? new MockApi() : MockApi.withSampleData().withSampleTemplates();
  if (data === "noprojects") {
    for (const project of (await api.overview()).projects) await api.removeProject(project.path);
  }
  // The running sample task (tienda) changes tools over time, so its mascot cycles.
  if (new URLSearchParams(window.location.search).get("activity") !== "none" && data === null) {
    api.withSampleActivity().withLiveTools();
  }
  const theme = new URLSearchParams(window.location.search).get("theme");
  if (theme === "light" || theme === "dark") void api.setSetting("ui.theme", theme);
  // `?onboarding=1` shows the first-run guide; `?lang=es|en` picks the language.
  if (new URLSearchParams(window.location.search).has("onboarding")) void api.setSetting("ui.onboarded", null);
  const lang = new URLSearchParams(window.location.search).get("lang");
  if (lang === "es" || lang === "en") void api.setSetting("ui.language", lang);
  // `?demo=1`: a task halfway through, waiting on a permission card.
  if (new URLSearchParams(window.location.search).has("demo")) {
    void api.startTask({
      project: "/code/hedy",
      prompt: "Agrega un recordatorio por WhatsApp 24 horas antes de cada cita.",
      mode: "project",
      model: null,
      planFirst: false,
    });
    // `?demo=limit` also shows the limit card.
    if (new URLSearchParams(window.location.search).get("demo") === "limit") {
      setTimeout(() => api.simulateLimit("t1"), 2500);
    }
  }
  return data === "fail" ? failingApi(api) : api;
}

const container = document.getElementById("root");
if (!container) throw new Error("missing #root element");
const root = createRoot(container);

function render(api: HedyApi) {
  root.render(
    <StrictMode>
      <App api={api} />
    </StrictMode>,
  );
}

if (isTauri()) render(tauriApi);
else void browserApi().then(render);
