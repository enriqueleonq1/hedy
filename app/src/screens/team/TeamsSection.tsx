// Ajustes → Equipos (docs/decisions.md §35): the list of teams (1b), and a
// team's people as cards with their PCs (8b) with the invite inline among
// them (3b), its layer's usage and leaving in a danger zone. Creating,
// joining, removing and reviewing open dialogs. Ajustes draws the title.

import { useState } from "react";
import { useQuery } from "@tanstack/react-query";

import { errorMessage, useApi } from "../../api/context";
import type { TeamInvite, TeamPerson, TeamStatus, TeamSummary } from "../../api/types";
import { Button, ConfirmButton } from "../../components/ui/Button";
import { Meter } from "../../components/ui/Meter";
import { useToast } from "../../components/ui/Toast";
import { timeAgo } from "../../format";
import { useT, type Translate } from "../../i18n";
import { MEMORY_HOMES_KEY } from "../../sync/hooks";
import {
  roleLabel,
  teamLabel,
  useTeamAction,
  useTeams,
  useTeamStatus,
  useTeamSync,
} from "../../team/hooks";
import { SyncErrorBox } from "../sync/SyncError";
import { CreateTeamDialog, JoinTeamDialog, RemovePersonDialog, ReviewDialog } from "./TeamDialogs";
import "../../styles/team.css";

export function TeamsSection() {
  const t = useT();
  const teams = useTeams();
  const [dialog, setDialog] = useState<"create" | "join" | null>(null);
  const [open, setOpen] = useState<string | null>(null);

  const list = teams.data ?? [];
  const current = list.find((team) => team.id === open && !team.waiting);
  return (
    <section className="card settings__section team" aria-label={t("team.title")}>
      {current ? (
        <TeamPanel id={current.id} onBack={() => setOpen(null)} />
      ) : (
        <>
          <div className="sync__head">
            <span className="sync__icon" aria-hidden>
              <TeamIcon />
            </span>
            <p className="sync__titles muted">{t("team.tagline")}</p>
          </div>
          {list.length > 0 && (
            <ul className="team-list">
              {list.map((team) => (
                <TeamRow key={team.id} team={team} onOpen={() => setOpen(team.id)} />
              ))}
            </ul>
          )}
          <div className="team-start">
            <button type="button" className="team-start__option" onClick={() => setDialog("create")}>
              <strong>{t("team.create.title")}</strong>
              <span className="small muted">{t("team.create.hint")}</span>
            </button>
            <button type="button" className="team-start__option" onClick={() => setDialog("join")}>
              <strong>{t("team.join.title")}</strong>
              <span className="small muted">{t("team.join.hint")}</span>
            </button>
          </div>
          <p className="small muted">{t("team.personalStays")}</p>
        </>
      )}
      {dialog === "create" && <CreateTeamDialog onClose={() => setDialog(null)} />}
      {dialog === "join" && <JoinTeamDialog onClose={() => setDialog(null)} />}
    </section>
  );
}

function TeamIcon() {
  return (
    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
      <circle cx="9" cy="8" r="3.2" />
      <path d="M3 19c.8-3.2 3.2-5 6-5s5.2 1.8 6 5" />
      <circle cx="17" cy="9" r="2.4" />
      <path d="M16 14.2c2.3.2 4 1.8 4.8 4.3" />
    </svg>
  );
}

function Initial({ name }: { name: string }) {
  return (
    <span className="team-initial" aria-hidden>
      {(name.trim()[0] ?? "?").toUpperCase()}
    </span>
  );
}

/** One team in the list: its projects here, its people, its state. */
function TeamRow({ team, onOpen }: { team: TeamSummary; onOpen: () => void }) {
  const t = useT();
  const api = useApi();
  const status = useTeamStatus(team.waiting ? null : team.id);
  const homes = useQuery({ queryKey: MEMORY_HOMES_KEY, queryFn: () => api.memoryHomes() });
  const projects = (homes.data ?? []).filter((h) => h.home?.kind === "team" && h.home.team === team.id).length;
  const people = status.data?.people.length ?? 0;
  const review = status.data?.toReview ?? 0;
  const cancel = useTeamAction((api) => api.teamLeave(team.id, true));
  const state = team.waiting
    ? t("team.state.waiting")
    : team.leaving
      ? t("team.state.leaving")
      : team.lastError
        ? t("team.state.failed")
        : review > 0
          ? t("team.state.review", { n: review })
          : t("team.state.ok");
  return (
    <li className="team-row__item">
      <button type="button" className="team-row" onClick={onOpen} disabled={team.waiting}>
        <Initial name={teamLabel(team)} />
        <span className="team-row__text">
          <strong>{team.waiting ? t("team.state.joiningName") : teamLabel(team)}</strong>
          <span className="small muted">
            {team.waiting
              ? team.url
              : t("team.row.detail", { projects, people, role: roleLabel(team.role, t) })}
          </span>
        </span>
        <span className={review > 0 || team.lastError ? "sync__pill team-pill--warn" : "sync__pill"}>{state}</span>
      </button>
      {team.waiting && (
        <div className="team-row__waiting">
          {team.lastError && (
            <span className="small sync-hero__error" role="status">
              {team.lastError}
            </span>
          )}
          <span className="spacer" />
          <ConfirmButton small disabled={cancel.isPending} confirmLabel={t("team.join.cancelConfirm")} onConfirm={() => cancel.mutate(undefined)}>
            {t("team.join.cancel")}
          </ConfirmButton>
        </div>
      )}
    </li>
  );
}

/** One team: its state, its people with the invite among them, and leaving. */
function TeamPanel({ id, onBack }: { id: string; onBack: () => void }) {
  const t = useT();
  const status = useTeamStatus(id);
  const sync = useTeamSync(id);
  const [renaming, setRenaming] = useState(false);
  const [reviewing, setReviewing] = useState(false);
  const [removing, setRemoving] = useState<TeamPerson | null>(null);
  const leave = useTeamAction((api, here: boolean) => api.teamLeave(id, here));

  if (!status.data) {
    return status.isError ? <p role="alert">{errorMessage(status.error, t)}</p> : <p className="muted">{t("team.loading")}</p>;
  }
  const team = status.data;
  const admin = team.summary.role === "admin";
  const last = team.summary.lastSync === null ? t("sync.notYet") : timeAgo(team.summary.lastSync, t);
  return (
    <>
      <nav className="team-crumbs small" aria-label={t("team.title")}>
        <button type="button" className="link-button" onClick={onBack}>
          {t("team.title")}
        </button>{" "}
        / <span>{team.summary.name}</span>
      </nav>
      <div className="sync__head">
        <Initial name={team.summary.name} />
        {renaming ? (
          <RenameTeam team={team} onDone={() => setRenaming(false)} />
        ) : (
          <div className="sync__titles">
            <h2 tabIndex={-1}>{team.summary.name}</h2>
            <span className="small muted">
              {roleLabel(team.summary.role, t)} · {t("team.last", { when: last })}
            </span>
          </div>
        )}
        {admin && !renaming && (
          <Button small variant="ghost" onClick={() => setRenaming(true)}>
            {t("team.rename")}
          </Button>
        )}
        {!team.summary.lastError && (
          <Button small disabled={sync.isPending} onClick={() => sync.mutate()}>
            {sync.isPending ? t("sync.syncing") : t("sync.now")}
          </Button>
        )}
      </div>
      {team.summary.lastError && (
        <SyncErrorBox message={team.summary.lastError} retrying={sync.isPending} onRetry={() => sync.mutate()} />
      )}
      {team.summary.leaving && (
        <div className="team-alert" role="status">
          <span className="small">{t("team.leaving")}</span>
          {/* It may wait a long while for another admin: it can still go here only. */}
          <ConfirmButton
            small
            confirmLabel={t("team.forgetConfirm")}
            onConfirm={() => leave.mutate(true, { onSuccess: onBack })}
          >
            {t("team.forget")}
          </ConfirmButton>
        </div>
      )}
      {team.failedInvites.map((number) => (
        <FailedInvite key={number} team={id} number={number} />
      ))}
      {team.toReview > 0 && (
        <div className="team-review-bar">
          <span>{t("team.state.review", { n: team.toReview })}</span>
          <Button small variant="primary" onClick={() => setReviewing(true)}>
            {t("team.review.open")}
          </Button>
        </div>
      )}
      <div className="team-layer">
        <span className="small muted">{t("team.layer", { used: team.layerTokens, max: team.layerMax })}</span>
        <Meter percent={team.layerMax > 0 ? (team.layerTokens / team.layerMax) * 100 : 0} label={t("team.layerUsage")} />
      </div>

      <h3 className="team-subtitle">{t("team.people")}</h3>
      <Invite team={team} />
      <ul className="team-people">
        {team.people.map((person) => (
          <PersonCard key={person.person} team={team} person={person} onRemove={() => setRemoving(person)} />
        ))}
      </ul>
      <p className="small muted">{t("team.peopleHint")}</p>

      <details className="sync-how">
        <summary>{t("team.how")}</summary>
        <p className="small muted">{t("team.howText")}</p>
        <p className="small muted mono">{team.summary.url}</p>
      </details>

      <section className="danger-zone" aria-labelledby="team-danger-title">
        <h3 id="team-danger-title">{t("team.danger")}</h3>
        <div className="danger-zone__row">
          <div className="danger-zone__text">
            <strong>{t("team.leave")}</strong>
            <span className="small muted">{t(admin ? "team.leave.textAdmin" : "team.leave.text")}</span>
          </div>
          <ConfirmButton
            small
            disabled={leave.isPending || team.summary.leaving}
            confirmLabel={t(admin ? "team.leaveConfirmAdmin" : "team.leaveConfirm")}
            onConfirm={() => leave.mutate(false, { onSuccess: () => !admin && onBack() })}
          >
            {t("team.leave.button")}
          </ConfirmButton>
        </div>
      </section>
      {reviewing && <ReviewDialog team={team} onClose={() => setReviewing(false)} />}
      {removing && <RemovePersonDialog team={team} person={removing} onClose={() => setRemoving(null)} />}
    </>
  );
}

/** An admin renames the team in place. */
function RenameTeam({ team, onDone }: { team: TeamStatus; onDone: () => void }) {
  const t = useT();
  const toast = useToast();
  const [name, setName] = useState(team.summary.name);
  const rename = useTeamAction((api, next: string) => api.teamRename(team.summary.id, next));
  const next = name.trim();
  return (
    <form
      className="team-rename"
      onSubmit={(e) => {
        e.preventDefault();
        if (!next || next === team.summary.name) return onDone();
        rename.mutate(next, {
          onSuccess: () => {
            toast.info(t("team.renamed", { name: next }));
            onDone();
          },
        });
      }}
    >
      <label htmlFor="team-rename" className="sr-only">
        {t("team.create.name")}
      </label>
      <input
        id="team-rename"
        value={name}
        maxLength={60}
        autoFocus
        onChange={(e) => setName(e.target.value)}
        onKeyDown={(e) => e.key === "Escape" && (e.stopPropagation(), onDone())}
      />
      <Button small variant="primary" type="submit" disabled={!next || rename.isPending}>
        {t("common.save")}
      </Button>
      <Button small variant="ghost" disabled={rename.isPending} onClick={onDone}>
        {t("common.cancel")}
      </Button>
    </form>
  );
}

function FailedInvite({ team, number }: { team: string; number: number }) {
  const t = useT();
  const dismiss = useTeamAction((api) => api.teamDismissFailedInvite(team, number));
  return (
    <div className="team-alert" role="alert">
      <span className="small">{t("team.invite.failed", { n: number })}</span>
      <button type="button" className="link-button small" onClick={() => dismiss.mutate(undefined)}>
        {t("common.close")}
      </button>
    </div>
  );
}

/** "Invitar a…": the invite appears as a row of the people list (3b). */
function Invite({ team }: { team: TeamStatus }) {
  const t = useT();
  const toast = useToast();
  const id = team.summary.id;
  const admin = team.summary.role === "admin";
  const [who, setWho] = useState("");
  const [asAdmin, setAsAdmin] = useState(false);
  const create = useTeamAction((api, mine: boolean) => api.teamInvite(id, asAdmin, mine));
  const cancel = useTeamAction((api, number: number) => api.teamCancelInvite(id, number));
  return (
    <div className="team-invite">
      {admin ? (
        <form
          className="team-invite__form"
          onSubmit={(e) => {
            e.preventDefault();
            create.mutate(false);
          }}
        >
          <label htmlFor="team-invite-who" className="sr-only">
            {t("team.invite.who")}
          </label>
          <input
            id="team-invite-who"
            placeholder={t("team.invite.who")}
            value={who}
            maxLength={60}
            onChange={(e) => setWho(e.target.value)}
          />
          <label className="small team-invite__admin">
            <input type="checkbox" checked={asAdmin} onChange={(e) => setAsAdmin(e.target.checked)} />
            {t("team.invite.asAdmin")}
          </label>
          <Button small variant="primary" type="submit" disabled={create.isPending}>
            {t("team.invite.create")}
          </Button>
        </form>
      ) : null}
      <div className="row">
        <Button small disabled={create.isPending} onClick={() => create.mutate(true)}>
          {t("team.invite.mine")}
        </Button>
      </div>
      {team.invites.length > 0 && (
        <ul className="team-invites">
          {team.invites.map((invite) => (
            <InviteRow
              key={invite.number}
              invite={invite}
              who={invite.ownPc ? t("team.invite.yourPc") : who.trim() || t("team.invite.someone")}
              onCopy={() =>
                void navigator.clipboard?.writeText(inviteMessage(team, invite, t)).then(
                  () => toast.info(t("team.invite.copied")),
                  (e: unknown) => toast.error(errorMessage(e, t)),
                )
              }
              onCancel={() => cancel.mutate(invite.number)}
            />
          ))}
        </ul>
      )}
      {team.invites.length > 0 && <p className="small muted">{t("team.invite.hint")}</p>}
    </div>
  );
}

/**
 * "Copiar invitación": one short message with the repo's address and the
 * code, to paste in a chat; "Unirme con una invitación" reads both back.
 */
export function inviteMessage(team: TeamStatus, invite: TeamInvite, t: Translate): string {
  return t(invite.ownPc ? "team.invite.messageMine" : "team.invite.message", {
    team: team.summary.name,
    url: team.summary.url,
    code: invite.code,
    hours: hoursLeft(invite.expiresAt),
  });
}

/** Whole hours until `at` (Unix seconds), never below zero. */
function hoursLeft(at: number, now = Date.now() / 1000): number {
  return Math.max(0, Math.round((at - now) / 3600));
}

function InviteRow({
  invite,
  who,
  onCopy,
  onCancel,
}: {
  invite: TeamInvite;
  who: string;
  onCopy: () => void;
  onCancel: () => void;
}) {
  const t = useT();
  const hours = hoursLeft(invite.expiresAt);
  // The code shows when asked: it is a secret until it is used.
  const [shown, setShown] = useState(false);
  return (
    <li className="team-invite__row">
      <span className="team-invite__who">{who}</span>
      <span className="mono team-invite__code">
        {shown ? invite.code : `${invite.number}-••••`}
      </span>
      {!shown && (
        <Button small variant="ghost" onClick={() => setShown(true)}>
          {t("team.invite.show")}
        </Button>
      )}
      <span className="small muted">
        {invite.answered ? t("team.invite.answered") : t("team.invite.unused", { hours })}
      </span>
      <span className="spacer" />
      <Button small onClick={onCopy}>
        {t("team.invite.copy")}
      </Button>
      <Button small variant="ghost" onClick={onCancel}>
        {t("team.invite.cancel")}
      </Button>
    </li>
  );
}

/** A person and their PCs (8b); an admin changes their role or removes them. */
function PersonCard({ team, person, onRemove }: { team: TeamStatus; person: TeamPerson; onRemove: () => void }) {
  const t = useT();
  const id = team.summary.id;
  const admin = team.summary.role === "admin";
  const role = useTeamAction((api, makeAdmin: boolean) => api.teamSetRole(id, person.person, makeAdmin));
  const removePc = useTeamAction((api, device: string) => api.teamRemovePc(id, device));
  return (
    <li className="team-person">
      <Initial name={person.name} />
      <div className="team-person__body">
        <div className="team-person__head">
          <strong>{person.me ? t("team.you") : person.name}</strong>
          <span className={person.role === "admin" ? "team-tag team-tag--admin" : "team-tag"}>{roleLabel(person.role, t)}</span>
        </div>
        <ul className="team-pcs">
          {person.devices.map((pc) => (
            <li key={pc.id} className="team-pc">
              <span>{pc.thisPc ? t("sync.thisPc") : pc.name.trim() || t("sync.unnamed")}</span>
              <span className="muted">· {pc.lastSync === null ? t("sync.notYet") : timeAgo(pc.lastSync, t)}</span>
              {(admin || person.me) && !pc.thisPc && (
                <button
                  type="button"
                  className="team-pc__remove"
                  aria-label={t("team.pc.remove", { pc: pc.name })}
                  onClick={() => removePc.mutate(pc.id)}
                >
                  ×
                </button>
              )}
            </li>
          ))}
        </ul>
      </div>
      {admin && !person.me && (
        <div className="team-person__actions">
          <Button small variant="ghost" disabled={role.isPending} onClick={() => role.mutate(person.role !== "admin")}>
            {t(person.role === "admin" ? "team.role.makeMember" : "team.role.makeAdmin")}
          </Button>
          <Button small variant="ghost" onClick={onRemove}>
            {t("team.remove.button")}
          </Button>
        </div>
      )}
    </li>
  );
}
