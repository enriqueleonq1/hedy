// Ajustes → Sincronización (docs/decisions.md §34): an invitation while it is
// off; once on, its state first (a failure in plain words), the changes to
// resolve, the PCs, what syncs and what stays, and the technical detail
// behind "Cómo funciona". Ajustes draws the section's title.

import { useId, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";

import { useAction, useApi } from "../../api/context";
import type { MemoryHome, SyncConflict, SyncDevice, SyncStatus } from "../../api/types";
import { Button, ConfirmButton } from "../../components/ui/Button";
import { EmptyState } from "../../components/ui/EmptyState";
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
import { syncErrorTitle } from "../../sync/errors";
import { SyncErrorBox } from "./SyncError";
import "../../styles/sync.css";

export function SyncSection() {
  const t = useT();
  const status = useSyncStatus();
  const [dialog, setDialog] = useState<"setup" | "join" | null>(null);

  return (
    <section className="card settings__section sync" aria-label={t("sync.title")}>
      {status.data ? (
        <SyncOn status={status.data} />
      ) : status.isError ? (
        // Not knowing the state is not "off": say so, and offer to read it again.
        <EmptyState
          error
          title={t("sync.loadError")}
          text={t("sync.loadErrorText")}
          onRetry={() => void status.refetch()}
        />
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
        <p className="sync__titles muted">{t("sync.tagline")}</p>
        {/* Only once known: while loading it is not "off" yet. */}
        {!loading && <span className="sync__pill">{t("sync.off")}</span>}
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
    : status.lastError
      ? t(syncErrorTitle(status.lastError))
      : state === "review"
        ? t(status.conflicts === 1 ? "sync.toResolve.one" : "sync.toResolve.other", { n: status.conflicts })
        : state === "pending"
          ? t("sync.pending")
          : t("sync.upToDate");

  return (
    <>
      <div className={`sync-hero sync-hero--${state}`} role="status">
        <span className="sync-hero__mark" aria-hidden>
          {state === "ok" ? "✓" : state === "pending" ? "…" : "!"}
        </span>
        <div className="sync-hero__text">
          <strong>{headline}</strong>
          <span className="small muted">
            {last} · {pcs} · <span className="mono">{repoLabel(status.url)}</span>
          </span>
        </div>
        {/* A failure has its own "Reintentar" below. */}
        {!status.lastError && (
          <Button variant="primary" disabled={now.isPending} onClick={() => now.mutate()}>
            {t("sync.now")}
          </Button>
        )}
      </div>
      {status.lastError && (
        <SyncErrorBox message={status.lastError} titled={false} retrying={now.isPending} onRetry={() => now.mutate()} />
      )}

      <Conflicts status={status} />

      <div className="sync__block">
        <div className="row row--between">
          <h2>{t("sync.pcsTitle")}</h2>
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

      <WhatSyncs />

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

/**
 * "Qué se sincroniza": what travels, item by item. Skills, templates and
 * themes always do (the API has no switch for them); a project's memory is
 * chosen in that project's Memoria, so here it is only counted.
 */
function WhatSyncs() {
  const api = useApi();
  const t = useT();
  const homes = useQuery({ queryKey: MEMORY_HOMES_KEY, queryFn: () => api.memoryHomes() }).data;
  const { personal, stay, groups, waiting } = summarize(homes ?? []);
  const always: MessageKey[] = ["sync.what.skills", "sync.what.templates", "sync.what.themes"];
  return (
    <div className="sync__block">
      <h2>{t("sync.what")}</h2>
      <ul className="sync-what">
        <li className="sync-what__row">
          <span className="sync-what__name">
            {t("sync.what.memory")}
            <span className="small muted">
              {homes && (
                <span>{t(personal === 1 ? "sync.syncedCount.one" : "sync.syncedCount.other", { n: personal })}</span>
              )}
              {homes && " · "}
              {t("sync.what.global")}
            </span>
          </span>
          <span className="state-label small">{t("sync.what.on")}</span>
        </li>
        {always.map((key) => (
          <li key={key} className="sync-what__row">
            <span className="sync-what__name">{t(key)}</span>
            <span className="state-label small">{t("sync.what.on")}</span>
          </li>
        ))}
        {homes && (
          <li className="sync-what__row">
            <span className="sync-what__name">
              {t("sync.stay")}
              <span className="small muted">
                <span>{t(stay === 1 ? "sync.stayCount.one" : "sync.stayCount.other", { n: stay })}</span>
                {" · "}
                {groups.length > 0 ? t("sync.stayGroups", { groups: groups.join(", ") }) : t("sync.stayNone")}
                {waiting > 0 && ` · ${t(waiting === 1 ? "sync.waiting.one" : "sync.waiting.other", { n: waiting })}`}
              </span>
            </span>
            <span className="state-label state-label--off small">{t("sync.what.off")}</span>
          </li>
        )}
        <li className="sync-what__row">
          <span className="sync-what__name">
            {t("sync.what.never")}
            <span className="small muted">{t("sync.what.neverDetail")}</span>
          </span>
          <span className="state-label state-label--off small">{t("sync.what.neverState")}</span>
        </li>
      </ul>
      <p className="small muted">{t("sync.what.perProject")}</p>
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
      <h2>{t("sync.conflicts")}</h2>
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

type Side = SyncConflict["kept"];
/** One way out of a conflict: the version it keeps, and whether that is this PC's. */
type Choice = { side: Side; useOther: boolean; mine: boolean };

/**
 * The two choices of a conflict. The other computer's version is the primary
 * one ("Usar la de Oficina") and this computer's the secondary ("Mantener la
 * mía"); when neither or both sides are this computer's, the version not in
 * use is the primary one and the one in use is kept.
 */
export function conflictChoices(
  conflict: SyncConflict,
  isThisPc: (device: string) => boolean,
): { primary: Choice; secondary: Choice } {
  const keptMine = isThisPc(conflict.kept.device);
  const otherMine = isThisPc(conflict.other.device);
  if (keptMine !== otherMine) {
    const theirsIsOther = keptMine;
    return {
      primary: { side: theirsIsOther ? conflict.other : conflict.kept, useOther: theirsIsOther, mine: false },
      secondary: { side: theirsIsOther ? conflict.kept : conflict.other, useOther: !theirsIsOther, mine: true },
    };
  }
  return {
    primary: { side: conflict.other, useOther: true, mine: false },
    secondary: { side: conflict.kept, useOther: false, mine: false },
  };
}

/** Both versions side by side; the one in use is marked, either can win. */
export function ConflictCard({ conflict, status }: { conflict: SyncConflict; status: SyncStatus }) {
  const t = useT();
  const toast = useToast();
  // As in a diff: the other version first, the one in use (the newer) last.
  const sides: { side: Side; inUse: boolean }[] = [
    { side: conflict.other, inUse: false },
    { side: conflict.kept, inUse: true },
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
  const isThisPc = (device: string) => status.devices.some((d) => d.id === device && d.thisPc);
  const { primary, secondary } = conflictChoices(conflict, isThisPc);
  // Two sides from the same PC are told apart by when they were written.
  const samePc = pcName(status, conflict.kept.device, t) === pcName(status, conflict.other.device, t);
  const useLabel = (side: Side) =>
    samePc
      ? t("sync.conflict.useAt", { when: timeAgo(side.at, t) })
      : t("sync.conflict.use", { pc: pcName(status, side.device, t) });
  const keepLabel = secondary.mine ? t("sync.conflict.keepMine") : t("sync.conflict.keepInUse");
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
        <Button small variant="secondary" disabled={resolve.isPending} onClick={() => resolve.mutate(secondary.useOther)}>
          {keepLabel}
        </Button>
        <Button small variant="primary" disabled={resolve.isPending} onClick={() => resolve.mutate(primary.useOther)}>
          {useLabel(primary.side)}
        </Button>
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
