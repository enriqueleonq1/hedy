import { useEffect, useId, useState, type ReactNode } from "react";

import { errorMessage, useAction } from "../../api/context";
import type { ActivityItem, BoardState, Decision, Overview, Phase, Task } from "../../api/types";
import { defaultAccount } from "../../components/Layout";
import { Mascot } from "../../components/Mascot/Mascot";
import { WorkingWord } from "../../components/Mascot/WorkingWord";
import { Button } from "../../components/ui/Button";
import { formatElapsed, timeAgo } from "../../format";
import { useT, type MessageKey } from "../../i18n";
import { ACTIVITY_KEY, needsYou, tasksKey, useActivity, useTaskEvents } from "../../tasks/hooks";
import { openPermission, useInbox, type InboxItem } from "../../tasks/inbox";
import { useFilesNotice } from "../memory/skills";
import { useMascot } from "../../tasks/useMascot";
import { DEFAULT_ACCOUNT_COLOR, ERROR_MASCOT_COLOR } from "../../theme/tokens";

/** Finished tasks stay on the board this long (the backend's `RECENT_HOURS`). */
export const RECENT_HOURS = 24;

export const STATE_LABEL: Record<BoardState, MessageKey> = {
  working: "activity.state.working",
  plan: "activity.state.plan",
  permission: "activity.state.permission",
  limited: "activity.state.limited",
  review: "activity.state.review",
  answered: "activity.state.answered",
  accepted: "activity.state.accepted",
  failed: "activity.state.failed",
  cancelled: "activity.state.cancelled",
};

const PHASE_LABEL: Record<Phase, MessageKey> = {
  plan: "activity.phase.plan",
  implement: "activity.phase.implement",
  tests: "activity.phase.tests",
  review: "activity.phase.review",
};

type Tone = "needs" | "working" | "done" | "failed" | "closed";

const TONE: Record<BoardState, Tone> = {
  plan: "needs",
  permission: "needs",
  limited: "needs",
  working: "working",
  review: "done",
  answered: "done",
  failed: "failed",
  accepted: "closed",
  cancelled: "closed",
};

const MARK: Record<BoardState, string> = {
  plan: "◆",
  permission: "◆",
  limited: "■",
  working: "▶",
  review: "✔",
  answered: "✔",
  failed: "✗",
  accepted: "·",
  cancelled: "·",
};

/** The current time in seconds, refreshed every `everyMs`. */
function useNow(everyMs: number): number {
  const [now, setNow] = useState(() => Date.now() / 1000);
  useEffect(() => {
    const timer = setInterval(() => setNow(Date.now() / 1000), everyMs);
    return () => clearInterval(timer);
  }, [everyMs]);
  return now;
}

/**
 * The task board ("Actividad"): every project's running and recent tasks,
 * what needs the user first. A row opens the task in its project's workspace.
 */
export function ActivityScreen({
  overview,
  onOpenTask,
  onOpenMemory,
}: {
  overview: Overview;
  onOpenTask: (project: string, taskId: string) => void;
  /** Opens a project's memory, on its notes or its skills. */
  onOpenMemory?: (project: string, tab: "notes" | "skills") => void;
}) {
  const t = useT();
  const board = useActivity();
  const inbox = useInbox();
  const now = useNow(30_000);
  const color = defaultAccount(overview)?.color ?? DEFAULT_ACCOUNT_COLOR;

  const items = board.data ?? [];
  const limited = inbox.items.some((i) => i.kind === "task" && i.item.state === "limited");
  const groups: { title: string; items: ActivityItem[] }[] = [
    { title: t("activity.group.running"), items: items.filter((i) => i.state === "working") },
    {
      title: t("activity.group.recent", { hours: RECENT_HOURS }),
      items: items.filter((i) => !needsYou(i.state) && i.state !== "working"),
    },
  ];
  const openMemory = onOpenMemory ?? (() => {});

  return (
    <div className="screen board">
      <div className="page-head">
        <div>
          <h1>{t("activity.title")}</h1>
          <p className="muted">{t("activity.subtitle")}</p>
        </div>
      </div>

      {board.isError && (
        <div className="board__empty">
          <Mascot color={ERROR_MASCOT_COLOR} state="sleeping" size={120} />
          <p role="alert">{errorMessage(board.error, t)}</p>
          <Button onClick={() => board.refetch()}>{t("common.retry")}</Button>
        </div>
      )}

      {inbox.count > 0 && (
        <BoardGroup
          title={t("activity.group.needsYou")}
          items={inbox.items}
          // Hedy never switches accounts by itself: the user decides.
          note={limited ? t("activity.limitHint") : undefined}
        >
          {(entry) =>
            entry.kind === "task" ? (
              <BoardRow key={entry.key} item={entry.item} overview={overview} now={now} onOpen={onOpenTask} />
            ) : (
              <ProposalRow key={entry.key} entry={entry} overview={overview} now={now} onOpen={openMemory} />
            )
          }
        </BoardGroup>
      )}

      {board.isSuccess && items.length === 0 && (
        <div className="board__empty">
          <Mascot color={color} state="sleeping" size={160} />
          <p className="board__empty-title">{t("activity.empty", { hours: RECENT_HOURS })}</p>
          <p className="hint">{t("activity.emptyHint")}</p>
        </div>
      )}

      {groups
        .filter((group) => group.items.length > 0)
        .map((group) => (
          <BoardGroup key={group.title} title={group.title} items={group.items}>
            {(item) => (
              <BoardRow key={item.task.id} item={item} overview={overview} now={now} onOpen={onOpenTask} />
            )}
          </BoardGroup>
        ))}
    </div>
  );
}

function BoardGroup<T>({
  title,
  items,
  note,
  children,
}: {
  title: string;
  items: T[];
  note?: string;
  children: (item: T) => ReactNode;
}) {
  const id = useId();
  return (
    <section className="board__group" aria-labelledby={id}>
      <h2 id={id} className="board__group-title">
        {title} <span className="muted">({items.length})</span>
      </h2>
      <ul className="board__list">{items.map(children)}</ul>
      {note && <p className="hint">{note}</p>}
    </section>
  );
}

function BoardRow({
  item,
  overview,
  now,
  onOpen,
}: {
  item: ActivityItem;
  overview: Overview;
  now: number;
  onOpen: (project: string, taskId: string) => void;
}) {
  const t = useT();
  const { task, state } = item;
  const account = overview.accounts.find((a) => a.id === task.account);
  // While it works, the state reads as what it is doing ("Leyendo…").
  const events = useTaskEvents(state === "working" ? task.id : null);
  const { activity } = useMascot(state === "working" ? task : null, events.data ?? []);
  const active = TONE[state] === "needs" || state === "working";
  const time = active
    ? t("activity.runningFor", { time: formatElapsed(now - task.createdAt) })
    : timeAgo(item.updatedAt, t, now);
  const row = (
    <button className={`board-row board-row--${TONE[state]}`} onClick={() => onOpen(task.project, task.id)}>
      <span className="board-row__title">{task.title}</span>
      <span className={`board-state board-state--${TONE[state]}`}>
        <span aria-hidden>{MARK[state]}</span>
        {activity ? (
          <>
            <span className="sr-only">{t(STATE_LABEL[state])}</span>
            <WorkingWord activity={activity} />
          </>
        ) : (
          t(STATE_LABEL[state])
        )}
      </span>
      <span className="board-row__meta">
        <span className="board-row__project">{item.projectName}</span>
        <span aria-hidden>·</span>
        <span className="board-row__who">
          <span className="dot" style={{ background: account?.color ?? DEFAULT_ACCOUNT_COLOR }} aria-hidden />
          {account?.name ?? task.account}
        </span>
        <span aria-hidden>·</span>
        <span>{item.agent}</span>
        {task.mode === "project" && (
          <>
            <span aria-hidden>·</span>
            <span>{t(PHASE_LABEL[item.phase])}</span>
          </>
        )}
        <span aria-hidden>·</span>
        <span>{time}</span>
        {item.queued > 0 && (
          <>
            <span aria-hidden>·</span>
            <span className="board-row__queued">{t("queue.count", { n: item.queued })}</span>
          </>
        )}
      </span>
    </button>
  );
  // A permission is answered right here (once, or no); everything else opens the task.
  if (state !== "permission" || !task.live) return <li>{row}</li>;
  return (
    <li>
      <div className={`board-item board-item--${TONE[state]}`}>
        {row}
        <PermissionActions task={task} />
      </div>
    </li>
  );
}

/** Allow once or deny the waiting request; "allow in this chat" stays on the task's own card. */
function PermissionActions({ task }: { task: Task }) {
  const t = useT();
  const events = useTaskEvents(task.id);
  const request = openPermission(events.data ?? []);
  const answer = useAction(
    (api, decision: Decision) => api.answerPermission(task.id, request?.requestId ?? "", decision),
    { invalidate: [ACTIVITY_KEY, tasksKey(task.project)] },
  );
  if (!request) return null;
  return (
    <div className="board-item__actions" role="group" aria-label={t("activity.inbox.permissionOf", { title: task.title })}>
      <code className="board-item__what">
        {request.tool}: {request.summary}
      </code>
      <Button small variant="primary" disabled={answer.isPending} onClick={() => answer.mutate("allow")}>
        {t("activity.inbox.allowOnce")}
      </Button>
      <Button small className="btn--warn" disabled={answer.isPending} onClick={() => answer.mutate("deny")}>
        {t("activity.inbox.deny")}
      </Button>
    </div>
  );
}

/**
 * A note or skill an AI proposed, approved or rejected right here by the user
 * (rule 6); the row opens the project's memory to read it in full first.
 */
function ProposalRow({
  entry,
  overview,
  now,
  onOpen,
}: {
  entry: Extract<InboxItem, { kind: "note" | "skill" }>;
  overview: Overview;
  now: number;
  onOpen: (project: string, tab: "notes" | "skills") => void;
}) {
  const t = useT();
  const notice = useFilesNotice();
  const isNote = entry.kind === "note";
  const scope = entry.kind === "note" ? entry.proposal.note.project : entry.proposal.skill.project;
  const author = entry.kind === "note" ? entry.proposal.note.author : entry.proposal.skill.author;
  const createdAt = entry.kind === "note" ? entry.proposal.note.createdAt : entry.proposal.skill.createdAt;
  const change = !!entry.proposal.replaces;
  const title =
    entry.kind === "note" ? entry.proposal.note.text : `${entry.proposal.skill.name} — ${entry.proposal.skill.description}`;
  const state = isNote
    ? t(change ? "activity.inbox.noteChange" : "activity.inbox.note")
    : t(change ? "activity.inbox.skillChange" : "activity.inbox.skill");
  const where =
    scope === null
      ? t("activity.inbox.global")
      : (overview.projects.find((p) => p.path === scope)?.name ?? scope.split("/").pop() ?? scope);
  // A global note shows in every project's memory: refresh them all.
  const invalidate = isNote ? [["memory"], ["status"]] : [["skills"]];
  const approve = useAction(
    async (api) => {
      if (entry.kind === "note") return api.approveNote(entry.proposal.note.id);
      const result = await api.approveSkill(entry.proposal.skill.id);
      notice("saved", result.skill.name, result.files, t);
    },
    { invalidate },
  );
  const reject = useAction(
    (api) => (entry.kind === "note" ? api.forgetNote(entry.proposal.note.id) : api.rejectSkill(entry.proposal.skill.id)),
    { invalidate },
  );
  const busy = approve.isPending || reject.isPending;
  const short = title.length > 60 ? `${title.slice(0, 57)}…` : title;

  return (
    <li>
      <div className="board-item board-item--needs board-item--proposal">
        <button className="board-row board-row--needs" onClick={() => onOpen(entry.project, isNote ? "notes" : "skills")}>
          <span className="board-row__title">{title}</span>
          <span className="board-state board-state--needs">
            <span aria-hidden>◇</span>
            {state}
          </span>
          <span className="board-row__meta">
            <span className="board-row__project">{where}</span>
            <span aria-hidden>·</span>
            <span>{t("activity.inbox.proposedBy", { author: author === "user" ? t("memory.note.you") : author })}</span>
            <span aria-hidden>·</span>
            <span>{timeAgo(createdAt, t, now)}</span>
          </span>
        </button>
        <div className="board-item__actions" role="group" aria-label={t("activity.inbox.decide", { title: short })}>
          <Button small variant="primary" disabled={busy} onClick={() => approve.mutate(undefined)}>
            {t(change ? "activity.inbox.approveChange" : "activity.inbox.approve")}
          </Button>
          <Button small variant="ghost" disabled={busy} onClick={() => reject.mutate(undefined)}>
            {t("activity.inbox.reject")}
          </Button>
        </div>
      </div>
    </li>
  );
}
