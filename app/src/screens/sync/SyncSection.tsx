// Ajustes → Sincronización (docs/decisions.md §34): an invitation while it is
// off; once on, its state first, the PCs, what syncs and what stays, the
// changes to review, and the technical detail behind "Cómo funciona".

import { useEffect, useId, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";

import { useAction, useApi } from "../../api/context";
import type { MemoryHome, SyncConflict, SyncDevice, SyncStatus } from "../../api/types";
import { Button, ConfirmButton } from "../../components/ui/Button";
import { useToast } from "../../components/ui/Toast";
import { timeAgo } from "../../format";
import { useT, type MessageKey } from "../../i18n";
import {
  MEMORY_HOMES_KEY,
  SYNC_CONFLICTS_KEY,
  SYNC_KEY,
  deviceName,
  pcName,
  refreshAfterSync,
  repoLabel,
  syncState,
  useSyncNow,
  useSyncStatus,
} from "../../sync/hooks";
import { AddPcDialog, JoinDialog, RemovePcDialog, SetupDialog } from "./SyncDialogs";
import "../../styles/sync.css";

export function SyncSection({ focus }: { focus?: boolean }) {
  const status = useSyncStatus();
  const [dialog, setDialog] = useState<"setup" | "join" | null>(null);

  useEffect(() => {
    if (!focus) return;
    const section = document.getElementById("sync-section");
    section?.scrollIntoView?.({ block: "start" });
    section?.querySelector<HTMLElement>("h2")?.focus({ preventScroll: true });
  }, [focus]);

  return (
    <section id="sync-section" className="card settings__section sync">
      {status.data ? (
        <SyncOn status={status.data} />
      ) : (
        <SyncOff loading={status.isPending} onSetup={() => setDialog("setup")} onJoin={() => setDialog("join")} />
      )}
      {dialog === "setup" && <SetupDialog onClose={() => setDialog(null)} />}
      {dialog === "join" && <JoinDialog onClose={() => setDialog(null)} />}
    </section>
  );
}

function SyncIcon() {
  return (
    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
      <path d="M20 12a8 8 0 0 1-13.7 5.6" />
      <path d="M4 12A8 8 0 0 1 17.7 6.4" />
      <path d="M18 2.5v4h-4" />
      <path d="M6 21.5v-4h4" />
    </svg>
  );
}

function SyncOff({ loading, onSetup, onJoin }: { loading: boolean; onSetup: () => void; onJoin: () => void }) {
  const t = useT();
  return (
    <>
      <div className="sync__head">
        <span className="sync__icon">
          <SyncIcon />
        </span>
        <div className="sync__titles">
          <h2 tabIndex={-1}>{t("sync.title")}</h2>
          <span className="small muted">{t("sync.tagline")}</span>
        </div>
        <span className="sync__pill">{t("sync.off")}</span>
      </div>
      <p className="muted small">{t("sync.offText")}</p>
      <div className="row">
        <Button variant="primary" disabled={loading} onClick={onSetup}>
          {t("sync.setUp")}
        </Button>
        <button type="button" className="sync-link" disabled={loading} onClick={onJoin}>
          {t("sync.joinLink")}
        </button>
      </div>
    </>
  );
}

function SyncOn({ status }: { status: SyncStatus }) {
  const t = useT();
  const now = useSyncNow();
  const [adding, setAdding] = useState(false);
  const [removing, setRemoving] = useState<SyncDevice | null>(null);
  const state = syncState(status);
  const last = status.lastSync === null ? t("sync.notYet") : timeAgo(status.lastSync, t);
  const pcs = t(status.devices.length === 1 ? "sync.pcs.one" : "sync.pcs.other", { n: status.devices.length });
  const headline = now.isPending
    ? t("sync.syncing")
    : state === "error"
      ? t("sync.failed")
      : state === "review"
        ? t(status.conflicts === 1 ? "sync.toReview.one" : "sync.toReview.other", { n: status.conflicts })
        : state === "pending"
          ? t("sync.pending")
          : t("sync.upToDate");

  return (
    <>
      <div className="sync__head">
        <span className="sync__icon">
          <SyncIcon />
        </span>
        <div className="sync__titles">
          <h2 tabIndex={-1}>{t("sync.title")}</h2>
          <span className="small muted">{t("sync.tagline")}</span>
        </div>
      </div>

      <div className={`sync-hero sync-hero--${state}`} role="status">
        <span className="sync-hero__mark" aria-hidden>
          {state === "ok" ? "✓" : state === "pending" ? "…" : "!"}
        </span>
        <div className="sync-hero__text">
          <strong>{headline}</strong>
          <span className="small muted">
            {last} · {pcs} · <span className="mono">{repoLabel(status.url)}</span>
          </span>
          {status.lastError && <span className="small sync-hero__error">{status.lastError}</span>}
        </div>
        <Button variant="primary" disabled={now.isPending} onClick={() => now.mutate()}>
          {t("sync.now")}
        </Button>
      </div>

      <div className="sync__block">
        <div className="row row--between">
          <h3>{t("sync.pcsTitle")}</h3>
          <button type="button" className="sync-link" onClick={() => setAdding(true)}>
            {t("sync.addPc")}
          </button>
        </div>
        <table className="sync-pcs">
          <thead>
            <tr>
              <th scope="col">{t("sync.col.name")}</th>
              <th scope="col">{t("sync.col.last")}</th>
              <th scope="col">
                <span className="sr-only">{t("sync.remove")}</span>
              </th>
            </tr>
          </thead>
          <tbody>
            {status.devices.map((d) => (
              <tr key={d.id}>
                <td>
                  {deviceName(d, t)}
                  {d.thisPc && <span className="muted small"> · {t("sync.thisPc")}</span>}
                </td>
                <td className="muted">{d.lastSync === null ? t("sync.notYet") : timeAgo(d.lastSync, t)}</td>
                <td className="sync-pcs__action">
                  {!d.thisPc && (
                    <button
                      type="button"
                      className="sync-link sync-link--danger"
                      aria-label={t("sync.removeNamed", { pc: deviceName(d, t) })}
                      onClick={() => setRemoving(d)}
                    >
                      {t("sync.remove")}
                    </button>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      <ProjectsSummary />
      <Conflicts status={status} />

      <div className="sync__foot">
        <p className="small muted">{t("sync.encrypted")}</p>
        <HowItWorks />
        <Leave />
      </div>
      {adding && <AddPcDialog url={status.url} onClose={() => setAdding(false)} />}
      {removing && <RemovePcDialog device={removing} onClose={() => setRemoving(null)} />}
    </>
  );
}

/** What travels and what stays, from every project's memory home. */
export function summarize(homes: MemoryHome[]) {
  const personal = homes.filter((h) => h.syncs).length;
  const stay = homes.length - personal;
  const groups = [
    ...new Set(homes.flatMap((h) => (h.home?.kind === "shared" ? [h.home.group.split("/").pop() ?? h.home.group] : []))),
  ];
  const waiting = homes.filter((h) => h.ask !== null && !h.home).length;
  return { personal, stay, groups, waiting };
}

function ProjectsSummary() {
  const api = useApi();
  const t = useT();
  const homes = useQuery({ queryKey: MEMORY_HOMES_KEY, queryFn: () => api.memoryHomes() }).data;
  if (!homes) return null;
  const { personal, stay, groups, waiting } = summarize(homes);
  return (
    <div className="sync-split">
      <div className="sync-split__cell">
        <span className="small muted">{t("sync.synced")}</span>
        <strong>{t(personal === 1 ? "sync.syncedCount.one" : "sync.syncedCount.other", { n: personal })}</strong>
        <span className="small muted">{t("sync.syncedDetail")}</span>
      </div>
      <div className="sync-split__cell">
        <span className="small muted">{t("sync.stay")}</span>
        <strong>{t(stay === 1 ? "sync.stayCount.one" : "sync.stayCount.other", { n: stay })}</strong>
        <span className="small muted">
          {groups.length > 0 ? t("sync.stayGroups", { groups: groups.join(", ") }) : t("sync.stayNone")}
          {waiting > 0 && ` · ${t(waiting === 1 ? "sync.waiting.one" : "sync.waiting.other", { n: waiting })}`}
        </span>
      </div>
    </div>
  );
}

function Conflicts({ status }: { status: SyncStatus }) {
  const api = useApi();
  const t = useT();
  const conflicts = useQuery({
    queryKey: [...SYNC_CONFLICTS_KEY, status.conflicts],
    queryFn: () => api.syncConflicts(),
    enabled: status.conflicts > 0,
  }).data;
  if (!conflicts?.length) return null;
  return (
    <div className="sync__block">
      <h3>{t("sync.conflicts")}</h3>
      {conflicts.map((c) => (
        <ConflictCard key={c.id} conflict={c} status={status} />
      ))}
    </div>
  );
}

const KIND_LABEL: Record<SyncConflict["kind"], MessageKey> = {
  note: "sync.conflict.kind.note",
  skill: "sync.conflict.kind.skill",
  template: "sync.conflict.kind.template",
  theme: "sync.conflict.kind.theme",
  suggestion: "sync.conflict.kind.suggestion",
};

/** Both versions side by side; the one in use is marked, either can win. */
export function ConflictCard({ conflict, status }: { conflict: SyncConflict; status: SyncStatus }) {
  const t = useT();
  const toast = useToast();
  // As in a diff: the other version first, the one in use (the newer) last.
  const sides: { side: SyncConflict["kept"]; useOther: boolean; inUse: boolean }[] = [
    { side: conflict.other, useOther: true, inUse: false },
    { side: conflict.kept, useOther: false, inUse: true },
  ];
  const client = useQueryClient();
  // The chosen version is written here at once: what shows it is read again.
  const resolve = useAction((api, useOther: boolean) => api.resolveSyncConflict(conflict.id, useOther), {
    invalidate: [],
    onSuccess: () => {
      toast.info(t("sync.conflict.resolved"));
      return refreshAfterSync(client);
    },
  });
  // Two sides from the same PC are told apart by when they were written.
  const samePc = pcName(status, conflict.kept.device, t) === pcName(status, conflict.other.device, t);
  const label = (side: SyncConflict["kept"]) =>
    samePc
      ? t("sync.conflict.useAt", { when: timeAgo(side.at, t) })
      : t("sync.conflict.use", { pc: pcName(status, side.device, t) });
  return (
    <article className="sync-conflict" aria-label={t(KIND_LABEL[conflict.kind])}>
      <header className="row">
        <span className="sync-conflict__badge">{t("sync.conflict.badge")}</span>
        <span className="small muted">{t(KIND_LABEL[conflict.kind])}</span>
      </header>
      <div className="sync-conflict__sides">
        {sides.map(({ side, inUse }) => (
          <div key={String(inUse)} className="sync-conflict__side">
            <span className="small muted">
              {t("sync.conflict.side", { pc: pcName(status, side.device, t), when: timeAgo(side.at, t) })}
              {inUse && ` · ${t("sync.conflict.inUse")}`}
            </span>
            <p className={side.text === null ? "muted" : undefined}>{side.text ?? t("sync.conflict.deleted")}</p>
          </div>
        ))}
      </div>
      <div className="actions sync-conflict__actions">
        {sides.map(({ side, useOther }) => (
          <Button
            key={String(useOther)}
            small
            variant={useOther ? "secondary" : "primary"}
            disabled={resolve.isPending}
            onClick={() => resolve.mutate(useOther)}
          >
            {label(side)}
          </Button>
        ))}
      </div>
    </article>
  );
}

function HowItWorks() {
  const t = useT();
  const [open, setOpen] = useState(false);
  const id = useId();
  const points: MessageKey[] = [
    "sync.how.repo",
    "sync.how.keys",
    "sync.how.projects",
    "sync.how.conflicts",
    "sync.how.never",
    "sync.how.auto",
  ];
  return (
    <>
      <button
        type="button"
        className="disclosure disclosure--quiet"
        aria-expanded={open}
        aria-controls={id}
        aria-label={t("sync.howLabel")}
        onClick={() => setOpen((o) => !o)}
      >
        <span className="disclosure__mark" aria-hidden>
          ›
        </span>
        {t("sync.how")}
      </button>
      <ul id={id} className="settings__how small muted" hidden={!open}>
        {points.map((key) => (
          <li key={key}>{t(key)}</li>
        ))}
      </ul>
    </>
  );
}

function Leave() {
  const t = useT();
  const toast = useToast();
  const client = useQueryClient();
  const leave = useAction((api) => api.syncLeave(), {
    invalidate: [SYNC_KEY],
    onSuccess: () => {
      toast.info(t("sync.left"));
      void refreshAfterSync(client);
    },
  });
  return (
    <div className="sync-leave">
      <ConfirmButton small confirmLabel={t("sync.leaveConfirm")} disabled={leave.isPending} onConfirm={() => leave.mutate(undefined)}>
        {t("sync.leave")}
      </ConfirmButton>
      <span className="small muted">{t("sync.leaveHint")}</span>
    </div>
  );
}
