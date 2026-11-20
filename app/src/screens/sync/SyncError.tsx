// A failed sync in a box: its sentence, "Reintentar", and git's own message
// behind "Detalles". The plain words come from `sync/errors`.

import { useId, useState } from "react";

import { Button } from "../../components/ui/Button";
import { useT } from "../../i18n";
import { SYNC_ERROR_TEXT, syncErrorKind } from "../../sync/errors";

/**
 * The failure's sentence, "Reintentar" and git's message behind "Detalles".
 * `titled={false}` when the failure's title already shows just above it.
 */
export function SyncErrorBox({
  message,
  retrying,
  onRetry,
  titled = true,
}: {
  message: string;
  retrying: boolean;
  onRetry: () => void;
  titled?: boolean;
}) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const id = useId();
  const { title, text } = SYNC_ERROR_TEXT[syncErrorKind(message)];
  return (
    <div className="sync-error" role="alert">
      <div className="sync-error__text">
        {titled && <strong>{t(title)}</strong>}
        <span className="small">{t(text)}</span>
      </div>
      <Button small variant="primary" disabled={retrying} onClick={onRetry}>
        {retrying ? t("sync.syncing") : t("common.retry")}
      </Button>
      <button
        type="button"
        className="disclosure disclosure--quiet sync-error__toggle"
        aria-expanded={open}
        aria-controls={id}
        onClick={() => setOpen((o) => !o)}
      >
        <span className="disclosure__mark" aria-hidden>
          ›
        </span>
        {t("sync.error.details")}
      </button>
      <pre id={id} className="sync-error__detail mono small" hidden={!open}>
        {message}
      </pre>
    </div>
  );
}
