import { useCallback, useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { errorMessage, useApi } from "../api/context";
import type { CodeCopy } from "../api/codeTypes";
import type { FileDiff, MemorySource, RuleBreak, RuleCheck, Task } from "../api/types";
import { Button } from "../components/ui/Button";
import { useT, type MessageKey, type Translate } from "../i18n";
import { fileVersion } from "../screens/workspace/reviewed";
import { useReviewComments } from "./comments";
import "../styles/rules.css";

/**
 * "Revisar con tus reglas" (docs/decisions.md §37): a task's changes against
 * the written rules, each warning under its line and citing its note. Only a
 * warning: nothing here blocks accepting the changes.
 *
 * ```tsx
 * const rules = useRuleCheck(task, files);
 * <RulesBar task={task} rules={rules} />
 * <DiffView annotations={rules.annotationsFor(file, copy)} … />
 * ```
 */
interface Stored {
  check: RuleCheck;
  /** The changes as they were checked: when they change, the check is outdated. */
  version: string;
}

/** One warning, with the rule it cites (from the check's `rules`). */
export interface Warning {
  brk: RuleBreak;
  rule: MemorySource | undefined;
  key: string;
}

const STORAGE_PREFIX = "hedy.rules.dismissed.";

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

function changesVersion(files: FileDiff[]): string {
  return files.map(fileVersion).join("\u0001");
}

/** Whether the line is still there, as it was checked (the check cuts long lines with "…"). */
function stillThere(brk: RuleBreak, files: FileDiff[]): boolean {
  const file = files.find((f) => f.path === brk.path);
  const line = file?.hunks.flatMap((h) => h.lines).find((l) => l.kind === "added" && l.newLine === brk.line);
  if (!line) return false;
  const text = line.text.trim();
  return brk.code.endsWith("…") ? text.startsWith(brk.code.slice(0, -1)) : text === brk.code;
}

/** The comment a warning becomes, for the agent to fix it. */
export function warningComment(w: Warning, t: Translate): string {
  const rule = w.rule?.text ?? "";
  return w.brk.fix
    ? t("rules.commentFix", { rule, message: w.brk.message, fix: w.brk.fix })
    : t("rules.comment", { rule, message: w.brk.message });
}

export function useRuleCheck(task: Task | null, files: FileDiff[]) {
  const api = useApi();
  const client = useQueryClient();
  const taskId = task?.id ?? "";
  const queryKey = useMemo(() => ["ruleCheck", taskId], [taskId]);
  const stored = useQuery<Stored | null>({
    queryKey,
    queryFn: () => Promise.resolve(null),
    enabled: false,
    staleTime: Infinity,
    gcTime: Infinity,
  });
  const version = useMemo(() => changesVersion(files), [files]);
  const run = useMutation<RuleCheck, unknown, string>({
    mutationFn: () => api.checkTaskRules(taskId),
    onSuccess: (check, checkedVersion) => client.setQueryData<Stored>(queryKey, { check, version: checkedVersion }),
  });
  const [dismissed, setDismissed] = useState<{ task: string; keys: Set<string> }>(() => ({
    task: taskId,
    keys: loadDismissed(taskId),
  }));
  const keys = dismissed.task === taskId ? dismissed.keys : loadDismissed(taskId);

  const check = stored.data?.check ?? null;
  const outdated = !!stored.data && stored.data.version !== version;
  const all = useMemo<Warning[]>(
    () =>
      (check?.breaks ?? [])
        .filter((brk) => stillThere(brk, files))
        .map((brk) => {
          const rule = check?.rules[brk.rule - 1];
          return { brk, rule, key: [brk.path, brk.line, rule?.text ?? brk.rule, brk.code].join("\u0000") };
        }),
    [check, files],
  );
  const shown = useMemo(() => all.filter((w) => !keys.has(w.key)), [all, keys]);

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
    running: run.isPending,
    error: run.error,
    start: () => run.mutate(version),
    /** Warnings on lines still there, not dismissed. */
    shown,
    /** How many of those warnings the user dismissed ("Está bien así"). */
    dismissedCount: all.length - shown.length,
    dismiss: (w: Warning) => setKeys((k) => k.add(w.key)),
    restoreAll: () => setKeys((k) => k.clear()),
    countFor: (path: string) => shown.filter((w) => w.brk.path === path).length,
    /** The file's warnings by the number of their line. */
    linesOf: (path: string) => {
      const map = new Map<number, Warning[]>();
      for (const w of shown.filter((w) => w.brk.path === path)) map.set(w.brk.line, [...(map.get(w.brk.line) ?? []), w]);
      return map;
    },
  };
}

export type RuleChecking = ReturnType<typeof useRuleCheck>;

/** Where a rule lives and who wrote it, short. */
function ruleWhere(rule: MemorySource, t: Translate): string {
  const parts: string[] = [];
  if (rule.layer === "mine") parts.push(t("rules.yours"));
  else parts.push(t("rules.team", { team: rule.team ?? "" }));
  parts.push(rule.project ?? t("rules.everyProject"));
  if (rule.author) parts.push(`@${rule.author}`);
  return parts.join(" · ");
}

/** The cards under one line of the diff: what breaks which rule, and what to do. */
export function LineWarnings({
  warnings,
  copy,
  rules,
}: {
  warnings: Warning[];
  copy: CodeCopy;
  rules: RuleChecking;
}) {
  const t = useT();
  const review = useReviewComments();
  return (
    <ul className="rl-warnings">
      {warnings.map((w) => {
        const text = warningComment(w, t);
        const added = review.comments.some((c) => c.path === w.brk.path && c.line === w.brk.line && c.text === text);
        return (
          <li key={w.key} className="rl-warning">
            <p className="rl-warning__head">
              <span className="rl-warning__label">{t("rules.warning")}</span>
              {w.rule && <q className="rl-warning__rule">{w.rule.text}</q>}
            </p>
            <p className="rl-warning__message">{w.brk.message}</p>
            {w.brk.fix && <p className="rl-warning__fix">{t("rules.fixed", { fix: w.brk.fix })}</p>}
            <div className="rl-warning__foot">
              {w.rule && <span className="rl-warning__where">{ruleWhere(w.rule, t)}</span>}
              <span className="rl-warning__actions">
                {added ? (
                  <span className="rl-warning__added">{t("rules.inComments")}</span>
                ) : (
                  <Button
                    small
                    disabled={!review.canSend}
                    onClick={() =>
                      review.add({ copy, path: w.brk.path, line: w.brk.line, side: "new", text, code: w.brk.code })
                    }
                  >
                    {t("rules.fix")}
                  </Button>
                )}
                <Button small variant="ghost" onClick={() => rules.dismiss(w)}>
                  {t("rules.fine")}
                </Button>
              </span>
            </div>
          </li>
        );
      })}
    </ul>
  );
}

/** Above the files: check the changes, and what the last check found. */
export function RulesBar({ task, copy, rules }: { task: Task; copy: CodeCopy; rules: RuleChecking }) {
  const t = useT();
  const review = useReviewComments();
  const running = task.status === "running";
  const { check } = rules;
  const plural = (n: number, one: MessageKey, many: MessageKey) =>
    n === 1 ? t(one) : t(many, { n });

  const button = (
    <Button
      small
      variant={check ? "secondary" : "primary"}
      disabled={rules.running || running}
      title={running ? t("rules.waitAgent") : t("rules.checkHint")}
      onClick={rules.start}
    >
      {check ? t("rules.again") : t("rules.check")}
    </Button>
  );

  const pending = rules.shown.filter(
    (w) => !review.comments.some((c) => c.path === w.brk.path && c.line === w.brk.line && c.text === warningComment(w, t)),
  );
  const lines = new Set(rules.shown.map((w) => `${w.brk.path}:${w.brk.line}`)).size;

  let body: React.ReactNode;
  let tone = "idle";
  if (rules.running) {
    body = <p className="rl-bar__text muted">{t("rules.checking")}</p>;
  } else if (!check) {
    body = <p className="rl-bar__text muted">{t("rules.checkHint")}</p>;
  } else if (check.status === "no_rules") {
    body = (
      <p className="rl-bar__text">
        {t("rules.noRules")} <span className="muted">{t("rules.noRulesHint")}</span>
      </p>
    );
  } else if (check.status === "nothing_to_check") {
    body = <p className="rl-bar__text">{t("rules.nothing")}</p>;
  } else if (check.status === "unread") {
    tone = "warn";
    body = <p className="rl-bar__text">{t("rules.unread")}</p>;
  } else if (rules.shown.length === 0) {
    tone = "ok";
    body = (
      <p className="rl-bar__text">
        {check.rules.length === 1 ? t("rules.cleanOne") : t("rules.clean", { n: check.rules.length })}
      </p>
    );
  } else {
    tone = "warn";
    body = (
      <p className="rl-bar__text">
        <strong>{plural(lines, "rules.linesOne", "rules.lines")}</strong>
      </p>
    );
  }

  return (
    <section className={`rl-bar rl-bar--${tone}`} aria-label={t("rules.check")} aria-busy={rules.running || undefined}>
      <div className="rl-bar__row">
        {body}
        <span className="rl-bar__tools">
          {pending.length > 0 && (
            <Button
              small
              variant="primary"
              disabled={!review.canSend}
              title={t("rules.fixAllHint")}
              onClick={() => {
                for (const w of pending) {
                  review.add({ copy, path: w.brk.path, line: w.brk.line, side: "new", text: warningComment(w, t), code: w.brk.code });
                }
              }}
            >
              {t("rules.fixAll")}
            </Button>
          )}
          {button}
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
      {rules.error !== null && <p className="error-text small">{errorMessage(rules.error, t)}</p>}
      {rules.outdated && check && <p className="muted small">{t("rules.outdated")}</p>}
      {check && check.protected.length > 0 && (
        <p className="muted small">{t("rules.protected", { list: check.protected.join(", ") })}</p>
      )}
      {check?.truncated && <p className="muted small">{t("rules.truncated")}</p>}
    </section>
  );
}
