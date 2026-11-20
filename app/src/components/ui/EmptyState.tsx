// The one empty or error state: what happened, in a line, and always the next
// step as a button (Reintentar for an error), never a dead end.

import type { ReactNode } from "react";

import { useT } from "../../i18n";
import { Button } from "./Button";

export function EmptyState({
  title,
  text,
  actions,
  onRetry,
  error = false,
  announce = true,
  art,
}: {
  title: string;
  text?: ReactNode;
  /** The next steps; the first one is usually the primary. */
  actions?: ReactNode;
  /** An error that may pass: a "Reintentar" button after the actions. */
  onRetry?: () => void;
  error?: boolean;
  /**
   * An error is announced (`role="alert"`) unless the screen gathers several
   * of them in one alert of its own.
   */
  announce?: boolean;
  /** A mascot or an icon above the title (decorative). */
  art?: ReactNode;
}) {
  const t = useT();
  return (
    <div className={`empty-state${error ? " empty-state--error" : ""}`} role={error && announce ? "alert" : undefined}>
      {art && (
        <div className="empty-state__art" aria-hidden>
          {art}
        </div>
      )}
      <p className="empty-state__title">{title}</p>
      {text && <p className="empty-state__text">{text}</p>}
      {(actions || onRetry) && (
        <div className="empty-state__actions">
          {actions}
          {onRetry && <Button onClick={onRetry}>{t("common.retry")}</Button>}
        </div>
      )}
    </div>
  );
}
