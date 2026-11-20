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
  panelOpen: boolean;
  open(request: TerminalRequest, title: string, input?: string): void;
  close(key: number): void;
  activate(key: number): void;
  markExited(key: number): void;
  setPanelOpen(open: boolean): void;
}

const TerminalsContext = createContext<TerminalsApi | null>(null);
let nextKey = 0;

export function TerminalProvider({ children }: { children: ReactNode }) {
  const [sessions, setSessions] = useState<TerminalSession[]>([]);
  const [activeKey, setActiveKey] = useState<number | null>(null);
  const [panelOpen, setPanelOpen] = useState(false);

  const open = useCallback((request: TerminalRequest, title: string, input?: string) => {
    const key = ++nextKey;
    setSessions((all) => [...all, { key, title, request, input, running: true }]);
    setActiveKey(key);
    setPanelOpen(true);
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
      panelOpen: panelOpen && sessions.length > 0,
      open,
      close,
      activate: setActiveKey,
      markExited,
      setPanelOpen,
    }),
    [sessions, effectiveActive, panelOpen, open, close, markExited],
  );
  return <TerminalsContext.Provider value={api}>{children}</TerminalsContext.Provider>;
}

export function useTerminals(): TerminalsApi {
  const api = useContext(TerminalsContext);
  if (!api) throw new Error("useTerminals must be used inside <TerminalProvider>");
  return api;
}
