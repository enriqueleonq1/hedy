import { createContext, useContext, useEffect, useState, type ReactNode } from "react";

import type { ThemeSetting } from "../api/types";
import { applyTheme, type ThemeName } from "./tokens";

const ThemeContext = createContext<ThemeName>("dark");

function systemTheme(): ThemeName {
  return window.matchMedia?.("(prefers-color-scheme: light)").matches ? "light" : "dark";
}

/** Applies the user's theme choice; "system" follows the OS live. */
export function ThemeProvider({ setting, children }: { setting: ThemeSetting; children: ReactNode }) {
  const [system, setSystem] = useState<ThemeName>(systemTheme);
  const resolved: ThemeName = setting === "system" ? system : setting;

  useEffect(() => {
    const query = window.matchMedia?.("(prefers-color-scheme: light)");
    if (!query) return;
    const update = () => setSystem(query.matches ? "light" : "dark");
    query.addEventListener("change", update);
    return () => query.removeEventListener("change", update);
  }, []);

  useEffect(() => {
    applyTheme(resolved);
    document.body.style.background = "var(--bg)";
  }, [resolved]);

  return <ThemeContext.Provider value={resolved}>{children}</ThemeContext.Provider>;
}

export function useThemeName(): ThemeName {
  return useContext(ThemeContext);
}
