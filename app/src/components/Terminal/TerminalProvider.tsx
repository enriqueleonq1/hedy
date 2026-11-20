import { createContext, useCallback, useContext, useMemo, useState, type ReactNode } from "react";

import type { TerminalRequest } from "../../api/types";

interface TerminalSession {
  key: number;
  title: string;
  request: TerminalRequest;
  /** Typed into the shell once it opens. */
  input?: string;
  running: boolean;
}

interface TerminalsApi {
  sessions: TerminalSession[];
  activeKey: number | null;
  /**
   * Asks a terminal to take the keyboard: a new `seq` each time one is opened or
   * chosen with the pointer. Moving between tabs with the arrow keys clears it, so
   * the focus stays on the tabs.
   */
  focusRequest: { key: number; seq: number } | null;
  panelOpen: boolean;
  open(request: TerminalRequest, title: string, input?: string): void;
  close(key: number): void;
  /** Shows a terminal; `focus` (the default) also moves the keyboard into it. */
  activate(key: number, focus?: boolean): void;
  markExited(key: number): void;
  setPanelOpen(open: boolean): void;
}

const TerminalsContext = createContext<TerminalsApi | null>(null);
let nextKey = 0;
let nextFocus = 0;

export function TerminalProvider({ children }: { children: ReactNode }) {
  const [sessions, setSessions] = useState<TerminalSession[]>([]);
  const [activeKey, setActiveKey] = useState<number | null>(null);
  const [panelOpen, setPanelOpen] = useState(false);
  const [focusRequest, setFocusRequest] = useState<{ key: number; seq: number } | null>(null);

  const open = useCallback((request: TerminalRequest, title: string, input?: string) => {
    const key = ++nextKey;
    setSessions((all) => [...all, { key, title, request, input, running: true }]);
    setActiveKey(key);
    setFocusRequest({ key, seq: ++nextFocus });
    setPanelOpen(true);
  }, []);

  const activate = useCallback((key: number, focus = true) => {
    setActiveKey(key);
    setFocusRequest(focus ? { key, seq: ++nextFocus } : null);
  }, []);

  const close = useCallback((key: number) => {
    setSessions((all) => all.filter((s) => s.key !== key));
  }, []);

  const markExited = useCallback((key: number) => {
    setSessions((all) => all.map((s) => (s.key === key ? { ...s, running: false } : s)));
  }, []);

  // Falls back to the newest session when the active one was closed.
  const effectiveActive = sessions.some((s) => s.key === activeKey) ? activeKey : (sessions.at(-1)?.key ?? null);

  const api = useMemo<TerminalsApi>(
    () => ({
      sessions,
      activeKey: effectiveActive,
      focusRequest,
      panelOpen: panelOpen && sessions.length > 0,
      open,
      close,
      activate,
      markExited,
      setPanelOpen,
    }),
    [sessions, effectiveActive, focusRequest, panelOpen, open, close, activate, markExited],
  );
  return <TerminalsContext.Provider value={api}>{children}</TerminalsContext.Provider>;
}

export function useTerminals(): TerminalsApi {
  const api = useContext(TerminalsContext);
  if (!api) throw new Error("useTerminals must be used inside <TerminalProvider>");
  return api;
}
