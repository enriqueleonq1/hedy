import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "@fontsource-variable/inter";
import "@fontsource-variable/jetbrains-mono";
import "@fontsource-variable/sora";

import { App } from "./App";
import { MockApi } from "./api/MockApi";
import { isTauri, tauriApi } from "./api/tauriApi";
import "./styles/global.css";
import { applyTheme } from "./theme/tokens";

// Dark tokens until the user's theme setting loads.
applyTheme("dark");

// In a plain browser (`npm run dev`) the UI runs against sample data;
// `?theme=light|dark` forces a theme there.
function browserApi(): MockApi {
  const api = MockApi.withSampleData();
  const theme = new URLSearchParams(window.location.search).get("theme");
  if (theme === "light" || theme === "dark") void api.setSetting("ui.theme", theme);
  // `?demo=1`: a task halfway through, waiting on a permission card.
  if (new URLSearchParams(window.location.search).has("demo")) {
    void api.startTask({
      project: "/code/hedy",
      prompt: "Agrega un recordatorio por WhatsApp 24 horas antes de cada cita.",
      mode: "project",
      model: null,
      planFirst: false,
    });
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
