// The sync's state in the status bar (shown once it is set up), and a small
// panel with its PCs and "Sincronizar ahora" (docs/decisions.md §34).

import { useEffect, useId, useRef, useState } from "react";

import type { SyncStatus } from "../../api/types";
import { timeAgo } from "../../format";
import { useT } from "../../i18n";
import { deviceName, repoLabel, syncState, useSyncNow, useSyncStatus } from "../../sync/hooks";
import { useAnchoredPopover } from "../ui/popover";
import "../../styles/sync.css";

export function SyncItem({ onOpenSettings }: { onOpenSettings: () => void }) {
  const status = useSyncStatus().data;
  if (!status) return null;
  return <SyncButton status={status} onOpenSettings={onOpenSettings} />;
}

function SyncButton({ status, onOpenSettings }: { status: SyncStatus; onOpenSettings: () => void }) {
  const t = useT();
  const now = useSyncNow();
  const [open, setOpen] = useState(false);
  const id = useId();
  const wrap = useRef<HTMLDivElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  const panel = useRef<HTMLDivElement>(null);
  useAnchoredPopover(open, trigger, panel, "start");

  useEffect(() => {
    if (!open) return;
    panel.current?.querySelector<HTMLElement>("button")?.focus();
    const onDown = (e: MouseEvent) => {
      if (!wrap.current?.contains(e.target as Node)) setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      setOpen(false);
      trigger.current?.focus();
    };
    document.addEventListener("mousedown", onDown);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDown);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  const state = now.isPending ? "syncing" : syncState(status);
  const text =
    state === "syncing"
      ? t("sync.syncing")
      : state === "error"
        ? t("sync.bar.error")
        : state === "review"
          ? t("sync.bar.review", { n: status.conflicts })
          : state === "pending"
            ? t("sync.bar.pending")
            : t("sync.bar.ok");
  const last = status.lastSync === null ? t("sync.notYet") : timeAgo(status.lastSync, t);

  return (
    <div ref={wrap} className="sync-item">
      <button
        ref={trigger}
        type="button"
        className={`statusbar__item sync-item__button sync-item__button--${state}`}
        aria-haspopup="dialog"
        aria-expanded={open}
        aria-controls={open ? id : undefined}
        aria-label={t("sync.bar.label", { state: text })}
        title={status.lastError ?? undefined}
        onClick={() => setOpen((o) => !o)}
      >
        <span className="statusbar__dot sync-item__dot" aria-hidden />
        <span className="statusbar__text" aria-hidden>
          {text}
        </span>
      </button>
      {open && (
        <div ref={panel} id={id} className="sync-pop" role="dialog" aria-label={t("sync.title")}>
          <div className="sync-pop__head">
            <span className={`sync-pop__dot sync-item__button--${state}`} aria-hidden />
            <div className="sync-pop__titles">
              <strong>
                {state === "error"
                  ? t("sync.failed")
                  : state === "review" || state === "pending"
                    ? text
                    : t("sync.upToDate")}
              </strong>
              <span className="small muted">
                {last} · {repoLabel(status.url)}
              </span>
              {status.lastError && <span className="small sync-hero__error">{status.lastError}</span>}
            </div>
          </div>
          <div className="sync-pop__pcs">
            <span className="small muted">{t("sync.pop.pcs")}</span>
            {status.devices.map((d) => (
              <div key={d.id} className="sync-pop__pc">
                <span>
                  {deviceName(d, t)}
                  {d.thisPc && <span className="muted"> · {t("sync.pop.this")}</span>}
                </span>
                <span className="muted">{d.lastSync === null ? t("sync.notYet") : timeAgo(d.lastSync, t)}</span>
              </div>
            ))}
          </div>
          <div className="sync-pop__actions">
            <button
              type="button"
              className="sync-link"
              onClick={() => {
                setOpen(false);
                onOpenSettings();
              }}
            >
              {t("sync.pop.settings")}
            </button>
            <button type="button" className="btn btn--primary btn--small" disabled={now.isPending} onClick={() => now.mutate()}>
              {t("sync.now")}
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
