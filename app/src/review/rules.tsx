import { useCallback, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { useMutation, useMutationState, useQuery, useQueryClient } from "@tanstack/react-query";

import { errorMessage, useApi } from "../api/context";
import type { CodeCopy } from "../api/codeTypes";
import type { FileDiff, MemorySource, RuleBreak, RuleCheck, Task } from "../api/types";
import { Button } from "../components/ui/Button";
import { useT, type MessageKey, type Translate } from "../i18n";
import { useReviewComments } from "./comments";
import { RuleChangeDialog } from "./RuleChange";
import "../styles/rules.css";

// "Revisar con tus reglas" (docs/decisions.md §37): a task's changes against
// the written rules, each warning under its line and citing its note. Only a
// warning: nothing here blocks accepting the changes.
//
//   const rules = useRuleCheck(task, files);
//   <RulesBar task={task} copy={copy} project={project} rules={rules} />
//   <DiffView annotated={rules.linesOf(path)} annotate={(line) => …} … />

/** A check as kept for its task. */
interface Stored {
  check: RuleCheck;
  /** The changes' added lines as they were checked: when they change, the check is outdated. */
  version: string;
  /** Rules changed, removed or excepted since this check (by note): their warnings wait for a new one. */
  settled: string[];
}

interface Run {
  task: string;
  version: string;
}

/** One warning, with the rule it cites (from the check's `rules`). */
export interface Warning {
  brk: RuleBreak;
  rule: MemorySource | undefined;
  /** A fingerprint of the warning: what was dismissed, and its comment. */
  key: string;
}

const STORAGE_PREFIX = "hedy.rules.dismissed.";
const NO_LINES: ReadonlyMap<number, Warning[]> = new Map();

function loadDismissed(task: string): Set<string> {
  try {
    const raw = localStorage.getItem(STORAGE_PREFIX + task);
    const list: unknown = raw ? JSON.parse(raw) : [];
    return new Set(Array.isArray(list) ? list.filter((v): v is string => typeof v === "string") : []);
  } catch {
    return new Set();
  }
}

function saveDismissed(task: string, keys: Set<string>) {
  try {
    if (keys.size === 0) localStorage.removeItem(STORAGE_PREFIX + task);
    else localStorage.setItem(STORAGE_PREFIX + task, JSON.stringify([...keys]));
  } catch {
    // Remembering what was dismissed is only a convenience.
  }
}

/** A closed task's dismissed warnings are of no use any more. */
export function forgetDismissed(task: string) {
  try {
    localStorage.removeItem(STORAGE_PREFIX + task);
  } catch {
    // Nothing kept.
  }
}

/** A short fingerprint (cyrb53): what was dismissed, without keeping the line's text. */
function fingerprint(text: string): string {
  let h1 = 0xdeadbeef;
  let h2 = 0x41c6ce57;
  for (let i = 0; i < text.length; i++) {
    const c = text.charCodeAt(i);
    h1 = Math.imul(h1 ^ c, 2654435761);
    h2 = Math.imul(h2 ^ c, 1597334677);
  }
  h1 = Math.imul(h1 ^ (h1 >>> 16), 2246822507) ^ Math.imul(h2 ^ (h2 >>> 13), 3266489909);
  h2 = Math.imul(h2 ^ (h2 >>> 16), 2246822507) ^ Math.imul(h1 ^ (h1 >>> 13), 3266489909);
  return (4294967296 * (2097151 & h2) + (h1 >>> 0)).toString(36);
}

/** The added lines' text: an in-place fix keeps the counts but not this. */
function changesVersion(files: FileDiff[]): string {
  return fingerprint(
    files
      .map(
        (f) =>
          `${f.path}\u0000${f.hunks
            .flatMap((h) => h.lines)
            .filter((l) => l.kind === "added")
            .map((l) => `${l.newLine}:${l.text}`)
            .join("\u0001")}`,
      )
      .join("\u0002"),
  );
}

/** Whether the line is still there, exactly as it was checked. */
function stillThere(brk: RuleBreak, files: FileDiff[]): boolean {
  const file = files.find((f) => f.path === brk.path);
  const line = file?.hunks.flatMap((h) => h.lines).find((l) => l.kind === "added" && l.newLine === brk.line);
  return line?.text === brk.raw;
}

/**
 * Text a model wrote, as a comment shows it and the agent reads it alike: no
 * link, image or HTML (a hidden `<!-- … -->` would be sent, not shown).
 */
function plain(text: string): string {
  return (
    text
      // `&#x202E;` would turn into a hidden character, `[a](…)` and `<…>` into a link or HTML.
      .replace(/[\\[\]<>&]/g, (c) => `\\${c}`)
      // A bare address would turn into a link too.
      .replace(/:\/\//g, ":\\/\\/")
      .replace(/\bwww\./gi, (w) => `${w.slice(0, 3)}\\.`)
  );
}

/**
 * The comment a warning becomes, for the agent to fix it: marked as the
 * checker's (it can be wrong), the approved rule quoted, and the checker's
 * words as what they are, never as the user's order.
 */
export function warningComment(w: Warning, t: Translate): string {
  const rule = plain(w.rule?.text ?? "");
  const message = plain(w.brk.message);
  return w.brk.fix ? t("rules.commentFix", { rule, message, fix: plain(w.brk.fix) }) : t("rules.comment", { rule, message });
}

export function useRuleCheck(task: Task | null, files: FileDiff[]) {
  const api = useApi();
  const client = useQueryClient();
  const taskId = task?.id ?? "";
  const stored = useQuery<Stored | null>({
    queryKey: ["ruleCheck", taskId],
    queryFn: () => Promise.resolve(null),
    enabled: false,
    staleTime: Infinity,
    gcTime: Infinity,
  });
  const version = useMemo(() => changesVersion(files), [files]);
  // The task travels with the run: another task may be on screen when it ends.
  const run = useMutation<RuleCheck, unknown, Run>({
    mutationKey: ["ruleCheck"],
    mutationFn: (vars) => api.checkTaskRules(vars.task),
    // A new check starts with no rule settled: it read the rules as they are now.
    onSuccess: (check, vars) => client.setQueryData<Stored>(["ruleCheck", vars.task], { check, version: vars.version, settled: [] }),
  });
  // A run outlives this view (another tab, another task): its state is read
  // from the cache, by task, so a second click never spends twice.
  const runs = useMutationState({
    filters: { mutationKey: ["ruleCheck"] },
    select: (m) => ({
      task: (m.state.variables as Run | undefined)?.task,
      status: m.state.status,
      error: m.state.error,
      at: m.state.submittedAt,
    }),
  });
  const last = runs.filter((r) => r.task === taskId).sort((a, b) => b.at - a.at)[0];
  const running = last?.status === "pending";

  const [dismissed, setDismissed] = useState<{
    task: string;
    keys: Set<string>;
  }>(() => ({
    task: taskId,
    keys: loadDismissed(taskId),
  }));
  const keys = useMemo(() => (dismissed.task === taskId ? dismissed.keys : loadDismissed(taskId)), [dismissed, taskId]);

  const check = stored.data?.check ?? null;
  const outdated = !!stored.data && stored.data.version !== version;
  const all = useMemo<Warning[]>(
    () =>
      (check?.breaks ?? [])
        .filter((brk) => stillThere(brk, files))
        .map((brk) => {
          const rule = check?.rules[brk.rule - 1];
          return {
            brk,
            rule,
            key: fingerprint([brk.path, brk.line, rule?.text ?? brk.rule, brk.raw].join("\u0000")),
          };
        }),
    [check, files],
  );
  // Rules changed, removed or excepted here: their warnings wait for a new
  // check (in memory only; nothing the user dismissed).
  const settledRefs = useMemo(() => new Set(stored.data?.settled ?? []), [stored.data]);
  const [changing, setChanging] = useState<{ task: string; rule: MemorySource } | null>(null);
  const isSettled = useCallback((w: Warning) => !!w.rule && settledRefs.has(noteRef(w.rule)), [settledRefs]);
  const shown = useMemo(() => all.filter((w) => !keys.has(w.key) && !isSettled(w)), [all, keys, isSettled]);
  const byPath = useMemo(() => {
    const map = new Map<string, Map<number, Warning[]>>();
    for (const w of shown) {
      const lines = map.get(w.brk.path) ?? new Map<number, Warning[]>();
      lines.set(w.brk.line, [...(lines.get(w.brk.line) ?? []), w]);
      map.set(w.brk.path, lines);
    }
    return map;
  }, [shown]);

  const setKeys = useCallback(
    (edit: (keys: Set<string>) => void) => {
      const next = new Set(keys);
      edit(next);
      saveDismissed(taskId, next);
      setDismissed({ task: taskId, keys: next });
    },
    [keys, taskId],
  );

  return {
    check,
    outdated,
    running,
    error: last?.status === "error" ? last.error : null,
    start: () => {
      if (!running && taskId) run.mutate({ task: taskId, version });
    },
    /** Warnings on lines still there, not dismissed. */
    shown,
    /** Warnings whose lines changed since the check (not a clean change: check again). */
    gone: (check?.breaks.length ?? 0) - all.length,
    /** How many of those warnings the user dismissed ("Está bien así"). */
    dismissedCount: all.filter((w) => keys.has(w.key)).length,
    dismiss: (w: Warning) => setKeys((k) => k.add(w.key)),
    /** Warnings of rules changed here since the check: they wait for a new one. */
    settledCount: all.filter((w) => !keys.has(w.key) && isSettled(w)).length,
    /** The rule was changed, removed or excepted: its warnings wait for a new check. */
    settle: (rule: MemorySource) =>
      client.setQueryData<Stored | null>(["ruleCheck", taskId], (s) =>
        s ? { ...s, settled: [...new Set([...s.settled, noteRef(rule)])] } : s,
      ),
    /** The rule the "Ya no aplica" dialog is open for. */
    changing: changing?.task === taskId ? changing.rule : null,
    openChange: (rule: MemorySource) => setChanging({ task: taskId, rule }),
    closeChange: () => setChanging(null),
    restoreAll: () => setKeys((k) => k.clear()),
    countFor: (path: string) => [...(byPath.get(path)?.values() ?? [])].reduce((n, list) => n + list.length, 0),
    /** The file's warnings by the number of their line (the same map while they do not change). */
    linesOf: (path: string): ReadonlyMap<number, Warning[]> => byPath.get(path) ?? NO_LINES,
  };
}

export type RuleChecking = ReturnType<typeof useRuleCheck>;

/** Which note a rule is (one text may be in two layers). */
function noteRef(rule: MemorySource): string {
  return rule.note.kind === "local" ? `local:${rule.note.id}` : `team:${rule.note.team}:${rule.note.uid}`;
}

/** Where a rule lives and who wrote it, short. */
function ruleWhere(rule: MemorySource, t: Translate): string {
  const parts: string[] = [];
  if (rule.layer === "mine") parts.push(t("rules.yours"));
  else parts.push(rule.team ? t("rules.team", { team: rule.team }) : t("rules.teamNoName"));
  parts.push(rule.project ?? t("rules.everyProject"));
  if (rule.author) parts.push(`@${rule.author}`);
  return parts.join(" · ");
}

/** The comment a warning became, if the user asked for it (even edited since). */
function commentOf(review: ReturnType<typeof useReviewComments>, w: Warning) {
  return review.comments.find((c) => c.source === w.key);
}

/** The cards under one line of the diff: what breaks which rule, and what to do. */
export function LineWarnings({ warnings, copy, rules }: { warnings: Warning[]; copy: CodeCopy; rules: RuleChecking }) {
  const t = useT();
  const review = useReviewComments();
  return (
    <ul className="rl-warnings">
      {warnings.map((w) => (
        <li key={w.key} className="rl-warning">
          <p className="rl-warning__head">
            <span className="rl-warning__label">{t("rules.warning")}</span>
            {w.rule && <span className="rl-warning__rule">{t("rules.quoted", { text: w.rule.text })}</span>}
          </p>
          <p className="rl-warning__message">{w.brk.message}</p>
          {w.brk.fix && <p className="rl-warning__fix">{t("rules.fixed", { fix: w.brk.fix })}</p>}
          <div className="rl-warning__foot">
            {w.rule && <span className="rl-warning__where">{ruleWhere(w.rule, t)}</span>}
            <span className="rl-warning__actions">
              {commentOf(review, w) ? (
                <span className="rl-warning__added">{t("rules.inComments")}</span>
              ) : (
                <Button
                  small
                  disabled={!review.canSend}
                  onClick={() =>
                    review.add({
                      copy,
                      path: w.brk.path,
                      line: w.brk.line,
                      side: "new",
                      text: warningComment(w, t),
                      code: w.brk.code,
                      source: w.key,
                    })
                  }
                >
                  {t("rules.fix")}
                </Button>
              )}
              <Button small variant="ghost" onClick={() => rules.dismiss(w)}>
                {t("rules.fine")}
              </Button>
              {w.rule && (
                <Button small variant="ghost" onClick={() => w.rule && rules.openChange(w.rule)}>
                  {t("rules.change.open")}
                </Button>
              )}
            </span>
          </div>
        </li>
      ))}
    </ul>
  );
}

/** Above the files: check the changes, and what the last check found. */
export function RulesBar({
  task,
  copy,
  project,
  rules,
  onAddedAll,
}: {
  task: Task;
  copy: CodeCopy;
  /** The task's project: an exception to a rule is written there. */
  project: string;
  rules: RuleChecking;
  /** Every warning became a comment: show what will be sent. */
  onAddedAll?: () => void;
}) {
  const t = useT();
  const review = useReviewComments();
  const agentRunning = task.status === "running";
  const { check } = rules;
  const plural = (n: number, one: MessageKey, many: MessageKey) => (n === 1 ? t(one) : t(many, { n }));

  const pending = rules.shown.filter((w) => !commentOf(review, w));
  const lines = new Set(rules.shown.map((w) => `${w.brk.path}:${w.brk.line}`)).size;
  // What changed since the check was never checked: never "clean" then.
  const stale = rules.outdated || rules.gone > 0 || rules.settledCount > 0;
  const bar = useRef<HTMLElement>(null);

  let text: React.ReactNode;
  let tone = "idle";
  if (rules.running) {
    text = <span className="muted">{t("rules.checking")}</span>;
  } else if (!check) {
    text = <span className="muted">{t("rules.checkHint")}</span>;
  } else if (check.status === "no_rules") {
    text = (
      <>
        {t("rules.noRules")} <span className="muted">{t("rules.noRulesHint")}</span>
      </>
    );
  } else if (check.status === "nothing_to_check" && !rules.outdated) {
    text = t("rules.nothing");
  } else if (check.status === "nothing_to_check") {
    // It added no line then; it may now.
    tone = "warn";
    text = t("rules.outdatedNow");
  } else if (check.status === "unread") {
    tone = "warn";
    text = t("rules.unread");
  } else if (lines > 0) {
    tone = "warn";
    text = <strong>{plural(lines, "rules.linesOne", "rules.lines")}</strong>;
  } else if (stale) {
    tone = "warn";
    // Warned lines that changed, or only new lines never checked.
    text = rules.settledCount > 0 ? t("rules.ruleChanged") : rules.gone > 0 ? t("rules.linesChanged") : t("rules.outdatedNow");
  } else if (rules.dismissedCount > 0) {
    text = t("rules.nothingPending");
  } else {
    tone = "ok";
    text = check.rules.length === 1 ? t("rules.cleanOne") : t("rules.clean", { n: check.rules.length });
  }

  // Never checked: one quiet row with the button; the card is for results.
  if (!check && !rules.running && rules.error === null) {
    return (
      <section ref={bar} tabIndex={-1} className="rl-bar rl-bar--compact" aria-label={t("rules.check")}>
        <Button
          small
          disabled={agentRunning}
          onClick={() => {
            rules.start();
            // The row becomes the card: the focus goes with it, not to the page.
            requestAnimationFrame(() => bar.current?.focus({ preventScroll: true }));
          }}
        >
          {t("rules.check")}
        </Button>
        <span className="muted small">{agentRunning ? t("rules.waitAgent") : t("rules.checkCost")}</span>
      </section>
    );
  }

  return (
    <section
      ref={bar}
      tabIndex={-1}
      className={`rl-bar rl-bar--${tone}`}
      aria-label={t("rules.check")}
      aria-busy={rules.running || undefined}
    >
      <div className="rl-bar__row">
        <p className="rl-bar__text" role="status">
          {text}
        </p>
        <span className="rl-bar__tools">
          {pending.length > 0 && (
            <Button
              small
              disabled={!review.canSend}
              title={t("rules.fixAllHint")}
              onClick={() => {
                for (const w of pending) {
                  review.add({
                    copy,
                    path: w.brk.path,
                    line: w.brk.line,
                    side: "new",
                    text: warningComment(w, t),
                    code: w.brk.code,
                    source: w.key,
                  });
                }
                onAddedAll?.();
              }}
            >
              {t("rules.fixAll")}
            </Button>
          )}
          <Button small variant={check && stale ? "primary" : "secondary"} disabled={rules.running || agentRunning} onClick={rules.start}>
            {check ? t("rules.again") : t("rules.check")}
          </Button>
        </span>
      </div>
      {check?.account && (
        <p className="rl-bar__meta muted small">
          {t("rules.checked", {
            files: plural(check.files, "rules.filesOne", "rules.files"),
            rules: plural(check.rules.length, "rules.rulesOne", "rules.rules"),
            account: check.account,
          })}
          {rules.dismissedCount > 0 && (
            <>
              {" · "}
              {plural(rules.dismissedCount, "rules.dismissedOne", "rules.dismissed")}{" "}
              <button type="button" className="rl-bar__link" onClick={rules.restoreAll}>
                {t("rules.showDismissed")}
              </button>
            </>
          )}
        </p>
      )}
      {agentRunning && <p className="muted small">{t("rules.waitAgent")}</p>}
      {rules.error !== null && <p className="error-text small">{errorMessage(rules.error, t)}</p>}
      {rules.outdated && check && lines > 0 && <p className="muted small">{t("rules.outdated")}</p>}
      {check && check.protected.length > 0 && <p className="muted small">{t("rules.protected", { list: check.protected.join(", ") })}</p>}
      {check?.truncated && <p className="muted small">{t("rules.truncated")}</p>}
      {rules.changing &&
        createPortal(
          <RuleChangeDialog
            rule={rules.changing}
            project={project}
            onClose={() => {
              rules.closeChange();
              // Its warning may be gone with the change: back to the bar, not to
              // nowhere (where its button is still there, the dialog went back to it).
              requestAnimationFrame(() => {
                const at = document.activeElement;
                if (!at || at === document.body || !at.isConnected) bar.current?.focus({ preventScroll: true });
              });
            }}
            onDone={rules.settle}
          />,
          document.body,
        )}
    </section>
  );
}
