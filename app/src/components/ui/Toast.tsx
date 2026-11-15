import { createContext, useCallback, useContext, useMemo, useState, type ReactNode } from "react";

import { useT } from "../../i18n";

type ToastKind = "info" | "error";

/** A button in the toast ("Deshacer"); pressing it closes the toast. */
export interface ToastAction {
  label: string;
  onClick: () => void;
}

export interface ToastOptions {
  action?: ToastAction;
  /** How long it stays; 6 s by default. */
  durationMs?: number;
}

interface ToastItem {
  id: number;
  kind: ToastKind;
  text: string;
  action?: ToastAction;
}

interface ToastApi {
  info(text: string, options?: ToastOptions): void;
  error(text: string): void;
}

const ToastContext = createContext<ToastApi | null>(null);
const DURATION_MS = 6000;

export function ToastProvider({ children }: { children: ReactNode }) {
  const t = useT();
  const [items, setItems] = useState<ToastItem[]>([]);

  const dismiss = useCallback((id: number) => {
    setItems((all) => all.filter((item) => item.id !== id));
  }, []);

  const push = useCallback(
    (kind: ToastKind, text: string, options?: ToastOptions) => {
      const id = Date.now() + Math.random();
      setItems((all) => [...all, { id, kind, text, action: options?.action }]);
      setTimeout(() => dismiss(id), options?.durationMs ?? DURATION_MS);
    },
    [dismiss],
  );

  const api = useMemo<ToastApi>(
    () => ({ info: (text, options) => push("info", text, options), error: (text) => push("error", text) }),
    [push],
  );

  return (
    <ToastContext.Provider value={api}>
      {children}
      <div className="toasts" role="status" aria-live="polite">
        {items.map((item) => (
          <div key={item.id} className={`toast toast--${item.kind}`}>
            <span>{item.text}</span>
            {item.action && (
              <button
                className="toast__action"
                onClick={() => {
                  dismiss(item.id);
                  item.action?.onClick();
                }}
              >
                {item.action.label}
              </button>
            )}
            <button className="toast__close" aria-label={t("shell.dismiss")} onClick={() => dismiss(item.id)}>
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
