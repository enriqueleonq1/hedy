import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";

import { errorMessage, OVERVIEW_KEY, useApi } from "../api/context";
import type { CustomTheme, SettingKey, ThemeSetting } from "../api/types";
import { useToast } from "../components/ui/Toast";
import { useT } from "../i18n";
import { codeThemeName } from "./codeTheme";
import { HEDY_THEMES, presetById, type ThemeDef } from "./presets";
import { repairContrast, type ContrastFix } from "./repair";
import { applyTokens, themes, type ThemeBase, type ThemeName, type ThemeTokens } from "./tokens";

/** The user's own themes (shared by the gallery and the provider). */
export const THEMES_KEY = ["themes"] as const;

/** What an unsaved theme was made from, so "Ajustar" can reopen its dialog as it was. */
export type DraftSource =
  | { kind: "vscode"; text: string; fileName: string | null; name: string }
  | { kind: "described"; description: string; account: string; base: ThemeBase | null };

export interface Preview {
  theme: ThemeDef;
  /** Colors `repairContrast` moved so the theme reads well (shown in the preview bar). */
  fixes: ContrastFix[];
  /** An unsaved theme: applying it saves it first. */
  draft?: { origin: CustomTheme["origin"]; source: DraftSource };
}

interface ThemeState {
  /** The theme on screen (the previewed one while previewing). */
  theme: ThemeDef;
  /** The theme used in each mode (`ui.theme.dark` / `ui.theme.light`). */
  inUse: Record<ThemeBase, ThemeDef>;
  mode: ThemeSetting;
  /** The OS's mode right now. */
  system: ThemeBase;
  customs: CustomTheme[];
  previewing: Preview | null;
  preview(preview: Preview): void;
  cancelPreview(): void;
  /** Saves the previewed theme (a draft first becomes a theme of the user's own) and uses it. */
  applyPreview(): Promise<void>;
  applying: boolean;
}

const fallback: ThemeDef = HEDY_THEMES.dark;
const ThemeContext = createContext<ThemeState>({
  theme: fallback,
  inUse: HEDY_THEMES,
  mode: "dark",
  system: "dark",
  customs: [],
  previewing: null,
  preview: () => {},
  cancelPreview: () => {},
  applyPreview: async () => {},
  applying: false,
});

function systemBase(): ThemeBase {
  return window.matchMedia?.("(prefers-color-scheme: light)").matches ? "light" : "dark";
}

/** A saved theme of the user's own, made whole and readable again (it may predate a token). */
export function customThemeDef(theme: CustomTheme): ThemeDef {
  const tokens: ThemeTokens = { ...themes[theme.base], ...theme.colors };
  return { id: `custom:${theme.id}`, name: theme.name, base: theme.base, tokens: repairContrast(tokens, theme.base).tokens };
}

/** A theme id of a setting as a theme of `base`: an unknown, deleted or other-base id falls back to Hedy's. */
function resolve(id: string | undefined, base: ThemeBase, customs: Map<string, ThemeDef>): ThemeDef {
  const found = id?.startsWith("custom:") ? customs.get(id) : id ? presetById(id) : undefined;
  return found && found.base === base ? found : HEDY_THEMES[base];
}

interface ThemeProviderProps {
  /** `ui.theme`: follow the system, or always light or dark. */
  setting: ThemeSetting;
  /** `ui.theme.dark` and `ui.theme.light` (missing: Hedy's own). */
  dark?: string;
  light?: string;
  children: ReactNode;
}

/**
 * Applies the user's theme: the mode ("system" follows the OS live) picks dark
 * or light, and that mode's theme sets the tokens. A preview paints the whole
 * app with another theme without saving anything until it is applied.
 */
export function ThemeProvider({ setting, dark, light, children }: ThemeProviderProps) {
  const api = useApi();
  const client = useQueryClient();
  const toast = useToast();
  const t = useT();
  const [system, setSystem] = useState<ThemeBase>(systemBase);
  const [previewing, setPreviewing] = useState<Preview | null>(null);
  const [applying, setApplying] = useState(false);

  useEffect(() => {
    const query = window.matchMedia?.("(prefers-color-scheme: light)");
    if (!query) return;
    const update = () => setSystem(query.matches ? "light" : "dark");
    query.addEventListener("change", update);
    return () => query.removeEventListener("change", update);
  }, []);

  const wantsCustom = Boolean(dark?.startsWith("custom:") || light?.startsWith("custom:"));
  const list = useQuery({ queryKey: THEMES_KEY, queryFn: () => api.listThemes(), staleTime: Infinity });
  const customs = useMemo(() => list.data ?? [], [list.data]);
  const customDefs = useMemo(() => new Map(customs.map((c) => [`custom:${c.id}`, customThemeDef(c)])), [customs]);

  const inUse = useMemo(
    () => ({ dark: resolve(dark, "dark", customDefs), light: resolve(light, "light", customDefs) }),
    [dark, light, customDefs],
  );
  const base: ThemeBase = setting === "system" ? system : setting;
  const theme = previewing?.theme ?? inUse[base];
  // A theme of the user's own waits for the list: no flash of Hedy's default first.
  const waiting = !previewing && wantsCustom && list.isPending;

  useLayoutEffect(() => {
    if (waiting) return;
    applyTokens(theme.tokens, theme.base);
    document.body.style.background = "var(--bg)";
  }, [theme, waiting]);

  const preview = useCallback((next: Preview) => setPreviewing(next), []);
  const cancelPreview = useCallback(() => setPreviewing(null), []);

  // The latest values for the async apply (the undo restores what was there).
  const latest = useRef({ setting, dark, light, system, previewing });
  useEffect(() => {
    latest.current = { setting, dark, light, system, previewing };
  });

  const applyPreview = useCallback(async () => {
    const { previewing: current, setting: mode, dark: was, light: wasLight, system: os } = latest.current;
    if (!current) return;
    const { base: target, name } = current.theme;
    setApplying(true);
    try {
      let id = current.theme.id;
      if (current.draft) {
        const saved = await api.saveTheme({
          id: null,
          name,
          base: target,
          colors: current.theme.tokens,
          origin: current.draft.origin,
        });
        await client.invalidateQueries({ queryKey: THEMES_KEY });
        id = `custom:${saved.id}`;
      }
      const key: SettingKey = target === "dark" ? "ui.theme.dark" : "ui.theme.light";
      const previous = (target === "dark" ? was : wasLight) ?? null;
      await api.setSetting(key, id);
      const switchMode = mode !== "system" && mode !== target;
      if (switchMode) await api.setSetting("ui.theme", target);
      await client.invalidateQueries({ queryKey: OVERVIEW_KEY });
      setPreviewing(null);

      const undo = {
        label: t("theme.undo"),
        onClick: () =>
          void (async () => {
            await api.setSetting(key, previous);
            if (switchMode) await api.setSetting("ui.theme", mode);
            await client.invalidateQueries({ queryKey: OVERVIEW_KEY });
          })().catch((error: unknown) => toast.error(errorMessage(error, t))),
      };
      // In "system" mode with the OS in the other mode, it shows once the OS switches.
      if (mode === "system" && os !== target) {
        toast.info(t(target === "dark" ? "theme.appliedLaterDark" : "theme.appliedLaterLight", { name }), {
          durationMs: 10_000,
          actions: [
            {
              label: t(target === "dark" ? "theme.useAlwaysDark" : "theme.useAlwaysLight"),
              onClick: () =>
                void api
                  .setSetting("ui.theme", target)
                  .then(() => client.invalidateQueries({ queryKey: OVERVIEW_KEY }))
                  .catch((error: unknown) => toast.error(errorMessage(error, t))),
            },
            undo,
          ],
        });
      } else {
        toast.info(t("theme.applied", { name }), { durationMs: 8000, action: undo });
      }
    } catch (error) {
      toast.error(errorMessage(error, t));
    } finally {
      setApplying(false);
    }
  }, [api, client, toast, t]);

  const value = useMemo<ThemeState>(
    () => ({
      theme,
      inUse,
      mode: setting,
      system,
      customs,
      previewing,
      preview,
      cancelPreview,
      applyPreview,
      applying,
    }),
    [theme, inUse, setting, system, customs, previewing, preview, cancelPreview, applyPreview, applying],
  );

  return <ThemeContext.Provider value={value}>{children}</ThemeContext.Provider>;
}

export function useTheme(): ThemeState {
  return useContext(ThemeContext);
}

/** Dark or light: the base of the theme on screen. */
export function useThemeName(): ThemeName {
  return useContext(ThemeContext).theme.base;
}

/** Every token of the theme on screen (for what cannot read CSS variables, like xterm). */
export function useThemeTokens(): ThemeTokens {
  return useContext(ThemeContext).theme.tokens;
}

/** The shiki theme of a diff in the theme on screen (see `codeTheme.ts`). */
export function useCodeTheme(): string {
  const theme = useContext(ThemeContext).theme;
  return useMemo(() => codeThemeName(theme), [theme]);
}
