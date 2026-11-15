// Focus mode (Ctrl/⌘ \): the sidebar steps aside so the work fills the window.
// Temporary on purpose: it is not remembered, unlike the sidebar rail.

import { createContext, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from "react";

interface FocusMode {
  focus: boolean;
  toggle: () => void;
}

const FocusContext = createContext<FocusMode>({ focus: false, toggle: () => {} });

/** Ctrl/⌘ \, wherever the keyboard reports the backslash. */
export function isFocusShortcut(e: KeyboardEvent): boolean {
  return (e.ctrlKey || e.metaKey) && !e.shiftKey && (e.key === "\\" || e.code === "Backslash");
}

export function FocusModeProvider({ children }: { children: ReactNode }) {
  const [focus, setFocus] = useState(false);
  const toggle = useCallback(() => setFocus((on) => !on), []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      // In a terminal, Ctrl+\ belongs to the shell (SIGQUIT).
      if (e.target instanceof Element && e.target.closest(".xterm")) return;
      if (!isFocusShortcut(e)) return;
      e.preventDefault();
      toggle();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [toggle]);

  const value = useMemo(() => ({ focus, toggle }), [focus, toggle]);
  return <FocusContext.Provider value={value}>{children}</FocusContext.Provider>;
}

export function useFocusMode(): FocusMode {
  return useContext(FocusContext);
}
