// Inicio: one inbox for everything that needs the user (plans, permissions,
// limits, proposals, sync conflicts, team suggestions), answered right where
// it shows; what is running or ready; the history; and the plans at a glance.
// It replaces the old Welcome and Actividad screens.

import { useQueries, useQuery, useQueryClient } from "@tanstack/react-query";
import { useMemo, useRef, useState } from "react";

import { errorMessage, useAction, useApi } from "../../api/context";
import type {
  Account,
  ActivityItem,
  Overview,
  SyncConflict,
  SyncStatus,
  Task,
  UsageSummary,
  UsageWindow,
} from "../../api/types";
import { defaultAccount, type View } from "../../components/Layout";
import { Mascot } from "../../components/Mascot/Mascot";
import {
  headlineWindow,
  planCardState,
  resetLabel,
  usePlanRetry,
  windowLabel,
} from "../../components/planUsage";
import { Button } from "../../components/ui/Button";
import { EmptyState } from "../../components/ui/EmptyState";
import { Meter, meterLevel } from "../../components/ui/Meter";
import { rovingKeyDown } from "../../components/ui/roving";
import { StatusPill } from "../../components/ui/StatusPill";
import { useToast } from "../../components/ui/Toast";
import { taskState } from "../../tasks/state";
import { formatTokens, formatUsd, timeAgo } from "../../format";
import {
  useLanguage,
  useT,
  type Language,
  type Translate,
} from "../../i18n";
import { tasksKey, useActivity, useConversations } from "../../tasks/hooks";
import { useInbox, type InboxItem } from "../../tasks/inbox";
import { pcName, refreshAfterSync, SYNC_CONFLICTS_KEY, useSyncStatus } from "../../sync/hooks";
import { DEFAULT_ACCOUNT_COLOR, ERROR_MASCOT_COLOR } from "../../theme/tokens";
import {
  BoardGroup,
  BoardRow,
  KeysHint,
  ProposalRow,
  useBoardKeys,
  useNow,
} from "../activity/ActivityScreen";
import { useCreate } from "../create/CreateProvider";
import { conflictChoices } from "../sync/SyncSection";
import "../../styles/home.css";

type Filter = "needs" | "working" | "ready" | "history";

/** Closed tasks shown in the history before "Ver más". */
const HISTORY_PAGE = 30;

/** One sentence under the greeting: what waits, and the plan closest to its limit. */
function summary(waiting: number, accounts: Account[], t: Translate): string {
  const read = accounts
    .map((account) => ({ account, w: headlineWindow(account) }))
    .filter((x): x is { account: Account; w: UsageWindow } => x.w !== null)
    .sort((a, b) => b.w.percent - a.w.percent);
  const high = read[0] && read[0].w.percent >= 70 ? read[0] : null;
  const first =
    waiting === 0
      ? t("home.summary.none")
      : waiting === 1
        ? t("home.summary.one")
        : t("home.summary.many", { n: waiting });
  if (!high) return first;
  return `${first} ${t("home.summary.plan", { name: high.account.name, percent: Math.round(high.w.percent), label: windowLabel(high.w, t).toLowerCase() })}`;
}

/** Hedy's one-liner about the week: tokens, the API keys' estimated cost (if any) and the busiest project. */
function weekUsageText(
  week: UsageSummary,
  t: Translate,
  language: Language,
): string | null {
  const tokens = week.projects.reduce((sum, p) => sum + p.tokens, 0);
  if (tokens === 0) return null;
  const costs = week.projects
    .map((p) => p.costUsd)
    .filter((c): c is number => c !== null);
  const cost =
    costs.length > 0
      ? t("welcome.usage.cost", {
          cost: formatUsd(
            costs.reduce((sum, c) => sum + c, 0),
            language,
          ),
        })
      : "";
  const busiest = week.projects[0];
  const where =
    busiest && week.projects.length > 1
      ? t("welcome.usage.busiest", { name: busiest.name })
      : "";
  return t("welcome.usage.total", {
    tokens: formatTokens(tokens),
    cost,
    busiest: where,
  });
}

export function HomeScreen({
  overview,
  onNavigate,
  onOpenTask,
}: {
  overview: Overview;
  onNavigate: (view: View) => void;
  onOpenTask: (project: string, taskId: string) => void;
}) {
  const t = useT();
  const create = useCreate();
  const board = useActivity();
  const inbox = useInbox();
  const now = useNow(30_000);
  const items = useMemo(() => board.data ?? [], [board.data]);
  const [filter, setFilter] = useState<Filter>("needs");
  const [project, setProject] = useState<string>("");
  const [query, setQuery] = useState("");
  const root = useRef<HTMLDivElement>(null);

  const hasAccounts = overview.accounts.length > 0;
  // Counts and the sentence come only from a board that loaded: a failed or
  // pending one is not "nothing waits".
  const known = board.isSuccess;
  const color = defaultAccount(overview)?.color ?? DEFAULT_ACCOUNT_COLOR;
  const greeting = overview.userName
    ? t("welcome.greetingNamed", { name: overview.userName })
    : t("welcome.greeting");

  const words = query.trim().toLowerCase();
  const matches = (title: string, path: string) =>
    (!project || path === project) &&
    (!words || title.toLowerCase().includes(words));
  const needs = inbox.items.filter((entry) =>
    matches(inboxTitle(entry), inboxProject(entry)),
  );
  const working = items.filter(
    (i) => i.state === "working" && matches(i.task.title, i.task.project),
  );
  const ready = items.filter(
    (i) =>
      (i.state === "review" || i.state === "answered") &&
      matches(i.task.title, i.task.project),
  );
  // Stopped by an error: out of the badge, but in sight until the board lets it go.
  const failed = items.filter(
    (i) => i.state === "failed" && matches(i.task.title, i.task.project),
  );

  // The latest open task, to pick up where the user left off.
  const latest = items
    .filter((i) => i.state !== "accepted" && i.state !== "cancelled")
    .sort((a, b) => b.updatedAt - a.updatedAt)[0];
  const current =
    overview.projects.find((p) => p.path === latest?.task.project) ??
    overview.projects[0];

  const keys = !known
    ? []
    : filter === "needs"
      ? [...needs.map((e) => e.key), ...ready.map((i) => i.task.id), ...failed.map((i) => i.task.id)]
      : filter === "working"
        ? working.map((i) => i.task.id)
        : filter === "ready"
          ? ready.map((i) => i.task.id)
          : [];
  const selected = useBoardKeys(root, keys);
  // A proposal is decided in Memoria's "Por aprobar", with all its context.
  const openMemory = (path: string) => onNavigate({ screen: "memory", project: path, tab: "pending" });
  const row = (item: ActivityItem) => (
    <BoardRow
      key={item.task.id}
      entryKey={item.task.id}
      selected={selected === item.task.id}
      item={item}
      overview={overview}
      now={now}
      onOpen={onOpenTask}
    />
  );

  const pills: { value: Filter; label: string; count?: number }[] = [
    { value: "needs", label: t("home.filter.needs"), count: known ? needs.length : undefined },
    {
      value: "working",
      label: t("home.filter.working"),
      count: known ? working.length : undefined,
    },
    { value: "ready", label: t("home.filter.ready"), count: known ? ready.length : undefined },
    { value: "history", label: t("home.filter.history") },
  ];

  return (
    <div className="screen home" ref={root}>
      <header className="home__head">
        <div>
          <h1>{greeting}</h1>
          {hasAccounts && known && (
            <p className="muted">{summary(inbox.count, overview.accounts, t)}</p>
          )}
        </div>
        {hasAccounts && (
          <div className="home__actions">
            {latest && (
              <Button
                onClick={() =>
                  onOpenTask(latest.task.project, latest.task.id)
                }
                title={latest.task.title}
              >
                {t("home.continue", {
                  title: shorten(latest.task.title, 28),
                })}
              </Button>
            )}
            {current ? (
              <Button
                variant="primary"
                onClick={() =>
                  onNavigate({
                    screen: "workspace",
                    project: current.path,
                    task: null,
                  })
                }
              >
                {t("home.newTask")}
              </Button>
            ) : (
              <Button variant="primary" onClick={() => create.newProject()}>
                {t("create.newProject")}
              </Button>
            )}
          </div>
        )}
      </header>

      {/* No account yet: why, and the one next step (the sidebar's is quiet). */}
      {!hasAccounts && (
        <EmptyState
          art={<Mascot color={color} state="idle" size={120} />}
          title={t("welcome.empty.title")}
          text={t("welcome.empty.why")}
          actions={
            <Button variant="primary" onClick={() => onNavigate({ screen: "accounts" })}>
              {t("welcome.addFirstAccount")}
            </Button>
          }
        />
      )}

      {hasAccounts && overview.projects.length === 0 && (
        <div className="start-choices">
          <button
            type="button"
            className="card start-choice"
            onClick={() => create.newProject()}
          >
            <strong>{t("create.newProject")}</strong>
            <span className="muted small">{t("create.newProjectHint")}</span>
          </button>
          <button
            type="button"
            className="card start-choice"
            onClick={create.openFolder}
          >
            <strong>{t("create.openFolder")}</strong>
            <span className="muted small">{t("create.openFolderHint")}</span>
          </button>
        </div>
      )}

      {hasAccounts && (
        <div className="home__body">
          <div className="home__main">
            <div
              className="home__filters"
              data-own-keys
              role="toolbar"
              aria-label={t("home.filters")}
            >
              <div
                className="pills"
                role="radiogroup"
                aria-label={t("home.show")}
                onKeyDown={rovingKeyDown}
              >
                {pills.map((pill) => (
                  <button
                    key={pill.value}
                    type="button"
                    role="radio"
                    aria-checked={filter === pill.value}
                    tabIndex={filter === pill.value ? 0 : -1}
                    className={`pill${filter === pill.value ? " pill--on" : ""}`}
                    onClick={() => setFilter(pill.value)}
                  >
                    {pill.label}
                    {pill.count !== undefined && (
                      <span className="pill__count">
                        <span className="sr-only"> </span>
                        {pill.count}
                      </span>
                    )}
                  </button>
                ))}
              </div>
              {overview.projects.length > 1 && (
                <select
                  aria-label={t("home.projectFilter")}
                  value={project}
                  onChange={(e) => setProject(e.target.value)}
                >
                  <option value="">{t("home.allProjects")}</option>
                  {overview.projects.map((p) => (
                    <option key={p.path} value={p.path}>
                      {p.name}
                    </option>
                  ))}
                </select>
              )}
              <input
                type="search"
                className="home__search"
                aria-label={t("home.search")}
                placeholder={t("home.search")}
                value={query}
                onChange={(e) => setQuery(e.target.value)}
              />
            </div>

            {board.isError && filter !== "history" && (
              <EmptyState
                error
                art={<Mascot color={ERROR_MASCOT_COLOR} state="sleeping" size={96} />}
                title={t("home.error.title")}
                text={errorMessage(board.error, t)}
                onRetry={() => void board.refetch()}
              />
            )}

            {filter === "needs" &&
              !board.isError &&
              (board.isPending ? null : needs.length > 0 || ready.length > 0 || failed.length > 0 ? (
                <>
                  {needs.length > 0 && (
                    <BoardGroup
                      title={t("activity.group.needsYou")}
                      items={needs}
                      note={
                        needs.some(isLimited)
                          ? t("activity.limitHint")
                          : undefined
                      }
                    >
                      {(entry) => (
                        <InboxRow
                          key={entry.key}
                          entry={entry}
                          selected={selected === entry.key}
                          overview={overview}
                          now={now}
                          onOpenTask={onOpenTask}
                          onOpenMemory={openMemory}
                          onNavigate={onNavigate}
                        />
                      )}
                    </BoardGroup>
                  )}
                  {ready.length > 0 && (
                    <BoardGroup title={t("home.readyToReview")} items={ready}>
                      {row}
                    </BoardGroup>
                  )}
                  {failed.length > 0 && (
                    <BoardGroup title={t("home.failed")} items={failed}>
                      {row}
                    </BoardGroup>
                  )}
                </>
              ) : (
                <Empty
                  color={color}
                  title={t("home.empty.needs")}
                  hint={t("home.empty.needsHint")}
                />
              ))}
            {filter === "working" &&
              known &&
              (working.length > 0 ? (
                <BoardGroup title={t("home.filter.working")} items={working}>
                  {row}
                </BoardGroup>
              ) : (
                <Empty color={color} title={t("home.empty.working")} />
              ))}
            {filter === "ready" &&
              known &&
              (ready.length > 0 ? (
                <BoardGroup title={t("home.filter.ready")} items={ready}>
                  {row}
                </BoardGroup>
              ) : (
                <Empty color={color} title={t("home.empty.ready")} />
              ))}
            {filter === "history" && (
              <History
                overview={overview}
                project={project}
                words={words}
                now={now}
                onOpenTask={onOpenTask}
                onNavigate={onNavigate}
              />
            )}
            {filter !== "history" && keys.length > 0 && <KeysHint />}
          </div>

          <aside className="home__side" aria-label={t("home.side")}>
            <Plans overview={overview} onNavigate={onNavigate} />
            {working.length > 0 && filter !== "working" && (
              <section className="home-card">
                <h2>{t("home.filter.working")}</h2>
                <ul className="home-card__list">
                  {working.slice(0, 4).map((item) => (
                    <li key={item.task.id}>
                      <button
                        type="button"
                        className="home-mini"
                        onClick={() =>
                          onOpenTask(item.task.project, item.task.id)
                        }
                      >
                        <span className="dot dot--activity" aria-hidden />
                        <span className="home-mini__text">
                          <span className="home-mini__title">
                            {item.task.title}
                          </span>
                          <span className="muted small">
                            {item.projectName}
                          </span>
                        </span>
                      </button>
                    </li>
                  ))}
                </ul>
              </section>
            )}
          </aside>
        </div>
      )}
    </div>
  );
}

function shorten(text: string, max: number): string {
  return text.length > max ? `${text.slice(0, max - 1)}…` : text;
}

function isLimited(entry: InboxItem): boolean {
  return entry.kind === "task" && entry.item.state === "limited";
}

function inboxTitle(entry: InboxItem): string {
  switch (entry.kind) {
    case "task":
      return entry.item.task.title;
    case "note":
      return entry.proposal.note.text;
    case "skill":
      return `${entry.proposal.skill.name} ${entry.proposal.skill.description}`;
    case "team":
      return entry.teamName;
    case "sync":
      return "";
  }
}

function inboxProject(entry: InboxItem): string {
  return entry.kind === "task"
    ? entry.item.task.project
    : entry.kind === "note" || entry.kind === "skill"
      ? entry.project
      : "";
}

/** One row of the inbox: a task, a proposal, or what sync and teams wait for. */
function InboxRow({
  entry,
  selected,
  overview,
  now,
  onOpenTask,
  onOpenMemory,
  onNavigate,
}: {
  entry: InboxItem;
  selected: boolean;
  overview: Overview;
  now: number;
  onOpenTask: (project: string, taskId: string) => void;
  onOpenMemory: (project: string) => void;
  onNavigate: (view: View) => void;
}) {
  const t = useT();
  if (entry.kind === "task") {
    return (
      <BoardRow
        entryKey={entry.key}
        selected={selected}
        item={entry.item}
        overview={overview}
        now={now}
        onOpen={onOpenTask}
      />
    );
  }
  if (entry.kind === "note" || entry.kind === "skill") {
    return (
      <ProposalRow
        entryKey={entry.key}
        selected={selected}
        entry={entry}
        overview={overview}
        now={now}
        onOpen={onOpenMemory}
      />
    );
  }
  const sync = entry.kind === "sync";
  const title = sync
    ? entry.error
      ? t("home.sync.failed")
      : entry.conflicts === 1
      ? t("home.sync.titleOne")
      : t("home.sync.title", { n: entry.conflicts })
    : entry.count === 1
      ? t("home.team.titleOne", { team: entry.teamName })
      : t("home.team.title", { n: entry.count, team: entry.teamName });
  const meta = sync ? (entry.error ? t("home.sync.failedMeta") : t("home.sync.meta")) : t("home.team.meta");
  const open = () =>
    onNavigate({ screen: "settings", section: sync ? "sync" : "teams" });
  return (
    <li
      className={`board-entry${selected ? " board-entry--selected" : ""}`}
      data-entry={entry.key}
    >
      <div className="board-item board-item--needs">
        <button
          className="board-row board-row--needs"
          data-key-action="open"
          onClick={open}
        >
          <span className="board-row__title">{title}</span>
          <span className="board-row__meta">
            <span className="board-state board-state--needs">
              <span aria-hidden>◆</span>
              {t(sync ? "home.sync.kind" : "home.team.kind")}
            </span>
            <span aria-hidden>·</span>
            <span>{meta}</span>
          </span>
        </button>
        {/* One change to resolve: its two choices right here; more, in Ajustes. */}
        {sync && !entry.error && entry.conflicts === 1 ? (
          <SyncChoices onMore={open} />
        ) : (
          <div className="board-item__actions">
            <Button small data-key-action="approve" onClick={open}>
              {t(sync ? (entry.error ? "home.sync.check" : "home.sync.resolve") : "home.team.review")}
            </Button>
          </div>
        )}
      </div>
    </li>
  );
}

/**
 * The one sync conflict's two choices, as in Ajustes → Sincronización: the
 * other computer's version first ("Usar la de Oficina"), or keep this one's.
 * Until the conflict is read (or if it cannot be), "Resolver" goes there.
 */
function SyncChoices({ onMore }: { onMore: () => void }) {
  const t = useT();
  const api = useApi();
  const status = useSyncStatus().data;
  const conflicts = useQuery({
    queryKey: [...SYNC_CONFLICTS_KEY, status?.conflicts ?? 0],
    queryFn: () => api.syncConflicts(),
    enabled: status?.conflicts === 1,
  }).data;
  const conflict = conflicts?.length === 1 ? conflicts[0] : undefined;
  if (!status || !conflict) {
    return (
      <div className="board-item__actions">
        <Button small data-key-action="approve" onClick={onMore}>
          {t("home.sync.resolve")}
        </Button>
      </div>
    );
  }
  return <ConflictChoices conflict={conflict} status={status} />;
}

function ConflictChoices({ conflict, status }: { conflict: SyncConflict; status: SyncStatus }) {
  const t = useT();
  const toast = useToast();
  const client = useQueryClient();
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
  const useLabel = samePc
    ? t("sync.conflict.useAt", { when: timeAgo(primary.side.at, t) })
    : t("sync.conflict.use", { pc: pcName(status, primary.side.device, t) });
  return (
    <div className="board-item__actions" role="group" aria-label={t("home.sync.choices")}>
      <Button small data-key-action="reject" disabled={resolve.isPending} onClick={() => resolve.mutate(secondary.useOther)}>
        {secondary.mine ? t("sync.conflict.keepMine") : t("sync.conflict.keepInUse")}
      </Button>
      <Button
        small
        variant="primary"
        data-key-action="approve"
        disabled={resolve.isPending}
        onClick={() => resolve.mutate(primary.useOther)}
      >
        {useLabel}
      </Button>
    </div>
  );
}

function Empty({
  color,
  title,
  hint,
}: {
  color: string;
  title: string;
  hint?: string;
}) {
  return (
    <div className="board__empty">
      <Mascot color={color} state="sleeping" size={96} />
      <p className="board__empty-title">{title}</p>
      {hint && <p className="hint">{hint}</p>}
    </div>
  );
}

/** Every task of every project (and the conversations), newest first, with the filters applied. */
function History({
  overview,
  project,
  words,
  now,
  onOpenTask,
  onNavigate,
}: {
  overview: Overview;
  project: string;
  words: string;
  now: number;
  onOpenTask: (project: string, taskId: string) => void;
  onNavigate: (view: View) => void;
}) {
  const t = useT();
  const api = useApi();
  const create = useCreate();
  const [shown, setShown] = useState(HISTORY_PAGE);
  const lists = useQueries({
    queries: overview.projects.map((p) => ({
      queryKey: tasksKey(p.path),
      queryFn: () => api.tasks(p.path),
    })),
  });
  const conversations = useConversations().data ?? [];
  // The board knows the state of recent tasks (a limit, a plan); the rest, their status.
  const byId = new Map((useActivity().data ?? []).map((item) => [item.task.id, item]));
  const name = (path: string) =>
    overview.projects.find((p) => p.path === path)?.name ??
    t("home.history.noProject");
  const all: Task[] = [
    ...lists.flatMap((q) => q.data ?? []),
    ...(project ? [] : conversations),
  ]
    .filter(
      (task) =>
        (!project || task.project === project) &&
        (!words || task.title.toLowerCase().includes(words)),
    )
    .sort((a, b) => b.createdAt - a.createdAt);
  // A project whose tasks did not load: say so, never "no history yet".
  const failed = lists.find((q) => q.isError);
  if (failed && all.length === 0) {
    return (
      <EmptyState
        error
        title={t("home.error.title")}
        text={errorMessage(failed.error, t)}
        onRetry={() => lists.forEach((q) => q.isError && void q.refetch())}
      />
    );
  }
  if (all.length === 0) {
    // Still reading: not "empty" yet.
    if (lists.some((q) => q.isPending)) return null;
    // Filtered to nothing: no next step to offer but clearing the filters.
    if (project || words) return <p className="empty">{t("home.history.empty")}</p>;
    const first = overview.projects[0];
    return (
      <EmptyState
        title={t("home.history.empty")}
        actions={
          first ? (
            <Button onClick={() => onNavigate({ screen: "workspace", project: first.path, task: null })}>
              {t("home.newTask")}
            </Button>
          ) : (
            <Button onClick={() => create.newProject()}>{t("create.newProject")}</Button>
          )
        }
      />
    );
  }
  return (
    <section className="board__group" aria-label={t("home.filter.history")}>
      <ul className="board__list">
        {all.slice(0, shown).map((task) => (
          <li key={task.id} className="board-entry">
            <button
              className="board-row board-row--closed"
              onClick={() => onOpenTask(task.project, task.id)}
            >
              <span className="board-row__title">{task.title}</span>
              <span className="board-row__meta">
                <StatusPill state={taskState(task, byId.get(task.id))} task={task} />
                <span aria-hidden>·</span>
                <span className="board-row__project">{name(task.project)}</span>
                <span aria-hidden>·</span>
                <span>{timeAgo(task.createdAt, t, now)}</span>
              </span>
            </button>
          </li>
        ))}
      </ul>
      {all.length > shown && (
        <Button
          variant="ghost"
          small
          onClick={() => setShown((n) => n + HISTORY_PAGE)}
        >
          {t("home.history.more")}
        </Button>
      )}
    </section>
  );
}

/** A plan that could not be read: ask its CLI again, for that account only. */
function PlanRetry({ id, name }: { id: string; name: string }) {
  const t = useT();
  const { retry, retrying } = usePlanRetry(id);
  return (
    <Button small variant="ghost" className="plan-row__retry" disabled={retrying} aria-label={t("welcome.plans.retryLabel", { name })} onClick={retry}>
      {t("welcome.plans.retry")}
    </Button>
  );
}

/** Every account's plan as one meter: the window closest to its limit, and when it renews. */
function Plans({
  overview,
  onNavigate,
}: {
  overview: Overview;
  onNavigate: (view: View) => void;
}) {
  const t = useT();
  const api = useApi();
  const language = useLanguage();
  const week = useQuery({
    queryKey: ["usageSummary", 7],
    queryFn: () => api.usageSummary(7),
  }).data;
  const weekText = week ? weekUsageText(week, t, language) : null;
  return (
    <section className="home-card" aria-labelledby="home-plans">
      <div className="home-card__head">
        <h2 id="home-plans">{t("welcome.plans.title")}</h2>
        <button
          type="button"
          className="link-button"
          onClick={() => onNavigate({ screen: "accounts" })}
        >
          {t("home.accounts")}
        </button>
      </div>
      <ul className="home-card__list">
        {overview.accounts.map((account) => {
          const w = headlineWindow(account);
          const state = planCardState(account);
          const detail = w
            ? [windowLabel(w, t), resetLabel(w, t, language)]
                .filter(Boolean)
                .join(" · ")
            : state === "payAsYouGo"
              ? t("welcome.plans.payAsYouGo")
              : state === "loading"
                ? t("accounts.plan.checking")
                : t("welcome.plans.noData");
          return (
            <li key={account.id}>
              <button
                type="button"
                className="plan-row"
                onClick={() =>
                  onNavigate({ screen: "accounts", account: account.id })
                }
              >
                <span className="plan-row__top">
                  <span
                    className="dot"
                    style={{ background: account.color }}
                    aria-hidden
                  />
                  <span className="plan-row__name">{account.name}</span>
                  {w && (
                    <span
                      className={`plan-row__pct plan-row__pct--${meterLevel(w.percent)}`}
                    >
                      {Math.round(w.percent)} %
                    </span>
                  )}
                </span>
                {w && (
                  <Meter
                    percent={w.percent}
                    label={`${account.name}: ${windowLabel(w, t)} ${Math.round(w.percent)} %`}
                  />
                )}
                <span className="muted small">{detail}</span>
              </button>
              {state === "noData" && <PlanRetry id={account.id} name={account.name} />}
            </li>
          );
        })}
      </ul>
      {weekText && <p className="muted small">{weekText}</p>}
    </section>
  );
}
