import { useQueryClient } from "@tanstack/react-query";
import { useEffect, useState } from "react";

import { OVERVIEW_KEY } from "../../api/context";
import { useT } from "../../i18n";
import "../../styles/code.css";
import { Button } from "../ui/Button";
import { clampSize, ResizeHandle } from "../ui/ResizeHandle";
import { useTerminals } from "./TerminalProvider";
import { TerminalView } from "./TerminalView";

const HEIGHT_KEY = "hedy.terminal.height";
export const DEFAULT_TERMINAL_HEIGHT = 320;
export const MIN_TERMINAL_HEIGHT = 120;
/** Room always left above the terminal: the header and a few lines of the page. */
const MIN_ABOVE = 180;

function readHeight(): number | null {
  try {
    const value = Number(localStorage.getItem(HEIGHT_KEY));
    return Number.isFinite(value) && value > 0 ? value : null;
  } catch {
    return null;
  }
}

function writeHeight(value: number) {
  try {
    localStorage.setItem(HEIGHT_KEY, String(Math.round(value)));
  } catch {
    // Remembering a height is only a convenience.
  }
}

function useWindowHeight(): number {
  const [height, setHeight] = useState(() => window.innerHeight);
  useEffect(() => {
    const update = () => setHeight(window.innerHeight);
    window.addEventListener("resize", update);
    return () => window.removeEventListener("resize", update);
  }, []);
  return height;
}

/** The panel's height: remembered, kept within the window, maximizable (as VS Code's). */
function usePanelHeight() {
  const windowHeight = useWindowHeight();
  const [saved, setSaved] = useState<number | null>(readHeight);
  const [restore, setRestore] = useState<number | null>(null);
  const max = Math.max(MIN_TERMINAL_HEIGHT, windowHeight - MIN_ABOVE);
  const height = clampSize(saved ?? DEFAULT_TERMINAL_HEIGHT, MIN_TERMINAL_HEIGHT, max);
  const set = (value: number) => {
    setSaved(value);
    setRestore(null);
    writeHeight(value);
  };
  const maximized = height >= max - 1;
  const toggleMax = () => {
    const next = maximized ? (restore ?? DEFAULT_TERMINAL_HEIGHT) : max;
    setRestore(maximized ? null : height);
    setSaved(next);
    writeHeight(next);
  };
  return { height, max, set, maximized, toggleMax };
}

export function TerminalPanel() {
  const { sessions, activeKey, panelOpen, open, close, activate, markExited, setPanelOpen } = useTerminals();
  const queryClient = useQueryClient();
  const t = useT();
  const panel = usePanelHeight();

  if (sessions.length === 0) return null;

  return (
    <section className="terminal-panel" hidden={!panelOpen} aria-label={t("common.terminal")} style={{ height: panel.height }}>
      <ResizeHandle
        className="terminal-panel__edge"
        orientation="horizontal"
        label={t("shell.resizeTerminal")}
        title={t("shell.resizeTerminalHint")}
        value={panel.height}
        min={MIN_TERMINAL_HEIGHT}
        max={panel.max}
        onChange={panel.set}
        onToggleMax={panel.toggleMax}
      />
      <div className="terminal-panel__bar" role="tablist" aria-label={t("shell.terminals")}>
        {sessions.map((s) => (
          <div key={s.key} className={`terminal-tab${s.key === activeKey ? " terminal-tab--active" : ""}`}>
            <button
              role="tab"
              aria-selected={s.key === activeKey}
              className="terminal-tab__title"
              onClick={() => activate(s.key)}
            >
              <span className={`dot ${s.running ? "dot--running" : "dot--idle"}`} aria-hidden />
              {s.title}
            </button>
            <button className="terminal-tab__close" aria-label={t("common.closeNamed", { name: s.title })} onClick={() => close(s.key)}>
              ×
            </button>
          </div>
        ))}
        <Button small variant="ghost" onClick={() => open({ kind: "shell", path: null }, t("common.terminal"))}>
          + {t("common.terminal")}
        </Button>
        <span className="spacer" />
        <Button small variant="ghost" onClick={panel.toggleMax}>
          {panel.maximized ? t("shell.restoreTerminal") : t("shell.maximizeTerminal")}
        </Button>
        <Button small variant="ghost" onClick={() => setPanelOpen(false)} aria-label={t("shell.hideTerminal")}>
          {t("shell.hide")}
        </Button>
      </div>
      <div className="terminal-panel__body">
        {sessions.map((s) => (
          <TerminalView
            key={s.key}
            request={s.request}
            active={panelOpen && s.key === activeKey}
            onExit={() => {
              markExited(s.key);
              // A finished login changes the account's state.
              void queryClient.invalidateQueries({ queryKey: OVERVIEW_KEY });
            }}
          />
        ))}
      </div>
    </section>
  );
}
