import { createContext, useCallback, useContext, useMemo, useState, type ReactNode } from "react";

type ToastKind = "info" | "error";
interface ToastItem {
  id: number;
  kind: ToastKind;
  text: string;
}

interface ToastApi {
  info(text: string): void;
  error(text: string): void;
}

const ToastContext = createContext<ToastApi | null>(null);
const DURATION_MS = 6000;

export function ToastProvider({ children }: { children: ReactNode }) {
  const [items, setItems] = useState<ToastItem[]>([]);

  const dismiss = useCallback((id: number) => {
    setItems((all) => all.filter((t) => t.id !== id));
  }, []);

  const push = useCallback(
    (kind: ToastKind, text: string) => {
      const id = Date.now() + Math.random();
      setItems((all) => [...all, { id, kind, text }]);
      setTimeout(() => dismiss(id), DURATION_MS);
    },
    [dismiss],
  );

  const api = useMemo<ToastApi>(
    () => ({ info: (t) => push("info", t), error: (t) => push("error", t) }),
    [push],
  );

  return (
    <ToastContext.Provider value={api}>
      {children}
      <div className="toasts" role="status" aria-live="polite">
        {items.map((t) => (
          <div key={t.id} className={`toast toast--${t.kind}`}>
            <span>{t.text}</span>
            <button className="toast__close" aria-label="Cerrar aviso" onClick={() => dismiss(t.id)}>
              ×
            </button>
          </div>
        ))}
      </div>
    </ToastContext.Provider>
  );
}

export function useToast(): ToastApi {
  const toast = useContext(ToastContext);
  if (!toast) throw new Error("useToast must be used inside <ToastProvider>");
  return toast;
}
