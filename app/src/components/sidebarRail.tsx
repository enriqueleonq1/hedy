// The sidebar folds into an icon rail: by itself on narrow windows, or when
// the user says so (remembered on this computer).

import { createContext, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from "react";

/** Below this width the sidebar starts as a rail (unless the user chose otherwise). */
export const NARROW_QUERY = "(max-width: 1100px)";
const STORAGE_KEY = "hedy.sidebar";

type Preference = "rail" | "full" | null;

function readPreference(): Preference {
  try {
    const value = localStorage.getItem(STORAGE_KEY);
    return value === "rail" || value === "full" ? value : null;
  } catch {
    return null;
  }
}

function writePreference(value: Preference) {
  try {
    if (value) localStorage.setItem(STORAGE_KEY, value);
    else localStorage.removeItem(STORAGE_KEY);
  } catch {
    // Storage blocked: the choice lasts until the window closes.
  }
}

function useNarrow(): boolean {
  const [narrow, setNarrow] = useState(() => window.matchMedia?.(NARROW_QUERY).matches ?? false);
  useEffect(() => {
    const query = window.matchMedia?.(NARROW_QUERY);
    if (!query) return;
    const onChange = () => setNarrow(query.matches);
    query.addEventListener?.("change", onChange);
    return () => query.removeEventListener?.("change", onChange);
  }, []);
  return narrow;
}

interface SidebarRail {
  rail: boolean;
  toggle: () => void;
}

const RailContext = createContext<SidebarRail>({ rail: false, toggle: () => {} });

export function SidebarRailProvider({ children }: { children: ReactNode }) {
  const narrow = useNarrow();
  const [preference, setPreference] = useState<Preference>(readPreference);
  const rail = preference ? preference === "rail" : narrow;
  const toggle = useCallback(() => {
    const next = rail ? "full" : "rail";
    setPreference(next);
    writePreference(next);
  }, [rail]);
  const value = useMemo(() => ({ rail, toggle }), [rail, toggle]);
  return <RailContext.Provider value={value}>{children}</RailContext.Provider>;
}

export function useSidebarRail(): SidebarRail {
  return useContext(RailContext);
}
