import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "@fontsource-variable/inter";
import "@fontsource-variable/jetbrains-mono";
import "@fontsource-variable/sora";

import { App } from "./App";
import { MockApi } from "./api/MockApi";
import { isTauri, tauriApi } from "./api/tauriApi";
import "./styles/global.css";
import "./styles/shell.css";
import "./styles/skills.css";
import { applyTheme } from "./theme/tokens";

// Dark tokens until the user's theme setting loads.
applyTheme("dark");

// In a plain browser (`npm run dev`) the UI runs against sample data, with a
// few running and recent tasks on the board (`#activity`; `?activity=none`
// leaves it empty); `?theme=light|dark` forces a theme there.
function browserApi(): MockApi {
  const api = MockApi.withSampleData().withSampleTemplates();
  // The running sample task (tienda) changes tools over time, so its mascot cycles.
  if (new URLSearchParams(window.location.search).get("activity") !== "none") api.withSampleActivity().withLiveTools();
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
  return api;
}

const api = isTauri() ? tauriApi : browserApi();

const root = document.getElementById("root");
if (!root) throw new Error("missing #root element");

createRoot(root).render(
  <StrictMode>
    <App api={api} />
  </StrictMode>,
);
