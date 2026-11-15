import { useEffect, useId, useState, type ReactNode } from "react";

import { errorMessage } from "../../api/context";
import type { ActivityItem, BoardState, Overview, Phase } from "../../api/types";
import { defaultAccount } from "../../components/Layout";
import { Mascot } from "../../components/Mascot/Mascot";
import { WorkingWord } from "../../components/Mascot/WorkingWord";
import { Button } from "../../components/ui/Button";
import { formatElapsed, timeAgo } from "../../format";
import { useT, type MessageKey } from "../../i18n";
import { needsYou, useActivity, useTaskEvents } from "../../tasks/hooks";
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

export type Tone = "needs" | "working" | "done" | "failed" | "closed";

export const TONE: Record<BoardState, Tone> = {
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

export const MARK: Record<BoardState, string> = {
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
}: {
  overview: Overview;
  onOpenTask: (project: string, taskId: string) => void;
}) {
  const t = useT();
  const board = useActivity();
  const now = useNow(30_000);
  const color = defaultAccount(overview)?.color ?? DEFAULT_ACCOUNT_COLOR;

  const items = board.data ?? [];
  const waiting = items.filter((i) => needsYou(i.state));
  const groups: { title: string; items: ActivityItem[]; note?: string }[] = [
    {
      title: t("activity.group.needsYou"),
      items: waiting,
      // Hedy never switches accounts by itself: the user decides.
      note: waiting.some((i) => i.state === "limited") ? t("activity.limitHint") : undefined,
    },
    { title: t("activity.group.running"), items: items.filter((i) => i.state === "working") },
    {
      title: t("activity.group.recent", { hours: RECENT_HOURS }),
      items: items.filter((i) => !needsYou(i.state) && i.state !== "working"),
    },
  ];

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
          <BoardGroup key={group.title} title={group.title} items={group.items} note={group.note}>
            {(item) => (
              <BoardRow key={item.task.id} item={item} overview={overview} now={now} onOpen={onOpenTask} />
            )}
          </BoardGroup>
        ))}
    </div>
  );
}

function BoardGroup({
  title,
  items,
  note,
  children,
}: {
  title: string;
  items: ActivityItem[];
  note?: string;
  children: (item: ActivityItem) => ReactNode;
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
  return (
    <li>
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
    </li>
  );
}
