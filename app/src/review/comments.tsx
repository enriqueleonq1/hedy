import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";

import { useAction, useApi } from "../api/context";
import type { HedyApi } from "../api/HedyApi";
import type { CodeCopy } from "../api/codeTypes";
import type { Task } from "../api/types";
import { copyKey, sameCopy, useFileNavigator } from "../files/navigator";
import { useT, type Translate } from "../i18n";
import { isOpen } from "../tasks/timeline";
import { listKeyFor, useSetQueue } from "../tasks/hooks";

/**
 * Review comments on lines of a task's changes, like a pull request with the AI
 * on the other side: written on the diff (or from the file viewer), then sent to
 * the task as one follow-up message with each comment's file, lines and code.
 *
 * ```tsx
 * const review = useReviewComments();
 * review.add({ copy: { kind: "task", task: id }, path: "facturas/impuestos.py", line: 16, side: "new", text: "¿Y si es cero?" });
 * review.selectFile("facturas/impuestos.py"); // shows it in the Revisión tab
 * await review.sendToAi(); // one message with every pending comment
 * ```
 */
export type CommentSide = "old" | "new";

export interface ReviewComment {
  id: string;
  /** The copy whose lines it is about (a task's worktree, usually). */
  copy: CodeCopy;
  path: string;
  /** 1-based, on `side`. */
  line: number;
  /** Last line of a range; `null` for one line. */
  endLine: number | null;
  /** `"new"`: the task's version; `"old"`: the base ("antes de la IA"), for a removed line. */
  side: CommentSide;
  /** Markdown. */
  text: string;
  /** The commented lines as they were when written (quoted to the AI); filled in when unknown. */
  code: string | null;
  /** Unix milliseconds. */
  createdAt: number;
  /** When it went to the AI (then it is history: dimmed, collapsed); `null` while pending. */
  sentAt: number | null;
  /** What it came from, when not written by hand (a rule check's warning). */
  source?: string | null;
}

export interface NewReviewComment {
  copy: CodeCopy;
  path: string;
  line: number;
  endLine?: number | null;
  side: CommentSide;
  text: string;
  /** The lines' text, when the caller has it at hand (else it is read from the copy). */
  code?: string;
  /** What it comes from, when not written by hand (a rule check's warning). */
  source?: string;
}

export interface ReviewComments {
  /** The current task's comments, oldest first (pending and sent). */
  comments: ReviewComment[];
  /** Those not sent yet. */
  pending: ReviewComment[];
  /** Any copy's comments (a file viewer of the user's folder), optionally of one file. */
  commentsFor(copy: CodeCopy, path?: string): ReviewComment[];
  add(input: NewReviewComment): ReviewComment;
  update(id: string, text: string): void;
  remove(id: string): void;
  /** The follow-up message `sendToAi` would send (for a preview). */
  message(ids?: string[]): string;
  /**
   * Sends the pending comments (or only `ids`) to the task as ONE follow-up,
   * through the composer's path: queued while the agent works. Resolves `true`
   * once sent; a failure is shown as a toast and resolves `false`.
   */
  sendToAi(ids?: string[]): Promise<boolean>;
  sending: boolean;
  /** A task that can still take a follow-up (open, with a worktree). */
  canSend: boolean;
  /** Shows a file in the Revisión tab (and scrolls to `line` of its new side). */
  selectFile(path: string, line?: number): void;
  selectedFile: string | null;
  /** The line asked for with the last `selectFile`, if any. */
  selectedLine: number | null;
  /** Counts `selectFile` calls, so asking again for the same file scrolls again. */
  selectSeq: number;
}

const STORAGE_PREFIX = "hedy.review.comments.";

function load(key: string): ReviewComment[] {
  try {
    const raw = localStorage.getItem(STORAGE_PREFIX + key);
    const list: unknown = raw ? JSON.parse(raw) : [];
    return Array.isArray(list) ? list.filter(isComment) : [];
  } catch {
    return [];
  }
}

function save(key: string, list: ReviewComment[]) {
  try {
    if (list.length === 0) localStorage.removeItem(STORAGE_PREFIX + key);
    else localStorage.setItem(STORAGE_PREFIX + key, JSON.stringify(list));
  } catch {
    // Storage is a convenience: the comments still work for this session.
  }
}

function isComment(value: unknown): value is ReviewComment {
  if (!value || typeof value !== "object") return false;
  const c = value as Partial<ReviewComment>;
  return typeof c.id === "string" && typeof c.path === "string" && typeof c.line === "number" && typeof c.text === "string" && !!c.copy;
}

let counter = 0;
function newId(): string {
  counter += 1;
  return `${Date.now().toString(36)}-${counter.toString(36)}-${Math.random().toString(36).slice(2, 6)}`;
}

/** `path:16` or `path:16-18`. */
export function lineRef(comment: Pick<ReviewComment, "path" | "line" | "endLine">): string {
  return `${comment.path}:${comment.line}${comment.endLine && comment.endLine !== comment.line ? `-${comment.endLine}` : ""}`;
}

/** A fence that the quoted code cannot close early. */
function fence(code: string): string {
  let marks = "```";
  while (code.includes(marks)) marks += "`";
  return marks;
}

/**
 * The follow-up message for some comments: a short introduction, then per
 * comment its `path:line(-end)`, the quoted code and what the user wrote.
 */
export function formatReviewMessage(comments: ReviewComment[], t: Translate): string {
  const intro = comments.length === 1 ? t("reviewer.message.introOne") : t("reviewer.message.intro", { n: comments.length });
  const parts = comments.map((c, i) => {
    const where = `${i + 1}. \`${lineRef(c)}\`${c.side === "old" ? ` ${t("reviewer.message.oldSide")}` : ""}`;
    const code = c.code?.replace(/\s+$/, "");
    const quote = code ? `\n${fence(code)}\n${code}\n${fence(code)}` : "";
    return `${where}${quote}\n${c.text.trim()}`;
  });
  return [intro, ...parts].join("\n\n");
}

/** The lines a comment is about, read from its copy (the base for the old side). */
async function readLines(api: HedyApi, comment: ReviewComment): Promise<string | null> {
  try {
    const content = await api.readFile(comment.copy, comment.path, comment.side === "old" ? "base" : undefined);
    if (content.kind !== "text") return null;
    const all = content.text.split("\n");
    const lines = all.slice(comment.line - 1, comment.endLine ?? comment.line);
    return lines.length ? lines.join("\n") : null;
  } catch {
    return null;
  }
}

const INERT: ReviewComments = {
  comments: [],
  pending: [],
  commentsFor: () => [],
  add: (input) => ({
    id: "",
    copy: input.copy,
    path: input.path,
    line: input.line,
    endLine: input.endLine ?? null,
    side: input.side,
    text: input.text,
    code: input.code ?? null,
    createdAt: 0,
    sentAt: null,
  }),
  update: () => {},
  remove: () => {},
  message: () => "",
  sendToAi: async () => false,
  sending: false,
  canSend: false,
  selectFile: () => {},
  selectedFile: null,
  selectedLine: null,
  selectSeq: 0,
};

const CommentsContext = createContext<ReviewComments>(INERT);
const ShownTaskContext = createContext<(task: Task | null) => void>(() => {});

/**
 * Holds the review comments of the task on screen (and of any other copy asked
 * about), kept in the browser storage per copy so they survive a reload.
 * Mounted once in `App`; the workspace says which task it shows with
 * `useReviewTask`, or a test passes `task`.
 */
export function ReviewCommentsProvider({ task: fixed, children }: { task?: Task | null; children: ReactNode }) {
  const [shown, setShown] = useState<Task | null>(null);
  const task = fixed === undefined ? shown : fixed;
  return (
    <ShownTaskContext.Provider value={setShown}>
      <CommentsStore task={task}>{children}</CommentsStore>
    </ShownTaskContext.Provider>
  );
}

/** The workspace tells the review store which task it shows (`null`: none). */
export function useReviewTask(task: Task | null) {
  const setShown = useContext(ShownTaskContext);
  useEffect(() => {
    setShown(task);
  }, [setShown, task]);
  useEffect(() => () => setShown(null), [setShown]);
}

function CommentsStore({ task, children }: { task: Task | null; children: ReactNode }) {
  const api = useApi();
  const t = useT();
  const nav = useFileNavigator();
  const setQueue = useSetQueue();
  const taskCopy = useMemo<CodeCopy | null>(() => (task ? { kind: "task", task: task.id } : null), [task]);
  const currentKey = taskCopy ? copyKey(taskCopy) : null;
  const [store, setStore] = useState<Record<string, ReviewComment[]>>({});
  const [selection, setSelection] = useState<{ key: string | null; path: string | null; line: number | null; seq: number }>(
    { key: null, path: null, line: null, seq: 0 },
  );

  const listOf = useCallback((key: string) => store[key] ?? load(key), [store]);
  const comments = useMemo(() => (currentKey ? listOf(currentKey) : []), [currentKey, listOf]);
  const pending = useMemo(() => comments.filter((c) => c.sentAt === null), [comments]);

  // A copy's comments as first read from storage, once: the update below may
  // run twice (React's strict mode) and must not read back what it saved.
  const disk = useRef(new Map<string, ReviewComment[]>());
  const change = useCallback((key: string, edit: (list: ReviewComment[]) => ReviewComment[]) => {
    if (!disk.current.has(key)) disk.current.set(key, load(key));
    const stored = disk.current.get(key) ?? [];
    setStore((s) => {
      const next = edit(s[key] ?? stored);
      save(key, next);
      return { ...s, [key]: next };
    });
  }, []);

  /** Every copy whose comments are in memory, the current one first. */
  const keyOf = useCallback(
    (id: string) => {
      const keys = [...new Set([...(currentKey ? [currentKey] : []), ...Object.keys(store)])];
      return keys.find((key) => listOf(key).some((c) => c.id === id)) ?? null;
    },
    [currentKey, store, listOf],
  );

  const add = useCallback(
    (input: NewReviewComment) => {
      const comment: ReviewComment = {
        id: newId(),
        copy: input.copy,
        path: input.path,
        line: input.line,
        endLine: input.endLine && input.endLine !== input.line ? input.endLine : null,
        side: input.side,
        text: input.text.trim(),
        code: input.code ?? null,
        createdAt: Date.now(),
        sentAt: null,
        source: input.source ?? null,
      };
      const key = copyKey(input.copy);
      change(key, (list) => [...list, comment]);
      if (comment.code === null) {
        // Quote the lines as they are now, before the agent changes them again.
        void readLines(api, comment).then((code) => {
          if (code !== null) change(key, (list) => list.map((c) => (c.id === comment.id ? { ...c, code } : c)));
        });
      }
      return comment;
    },
    [api, change],
  );

  const update = useCallback(
    (id: string, text: string) => {
      const key = keyOf(id);
      if (key) change(key, (list) => list.map((c) => (c.id === id ? { ...c, text: text.trim() } : c)));
    },
    [keyOf, change],
  );

  const remove = useCallback(
    (id: string) => {
      const key = keyOf(id);
      if (key) change(key, (list) => list.filter((c) => c.id !== id));
    },
    [keyOf, change],
  );

  const targets = useCallback(
    (ids?: string[]) => (ids ? pending.filter((c) => ids.includes(c.id)) : pending),
    [pending],
  );

  const message = useCallback((ids?: string[]) => formatReviewMessage(targets(ids), t), [targets, t]);

  const send = useAction(
    (a, vars: { task: Task; text: string; ids: string[] }) => a.queueMessage(vars.task.id, vars.text),
    {
      invalidate: task ? [listKeyFor(task)] : [],
      onSuccess: (left, vars) => {
        setQueue(vars.task.id, left);
        const sentAt = Date.now();
        change(copyKey({ kind: "task", task: vars.task.id }), (list) =>
          list.map((c) => (vars.ids.includes(c.id) ? { ...c, sentAt } : c)),
        );
      },
    },
  );

  const canSend = !!task?.workspace && isOpen(task);

  const sendToAi = useCallback(
    async (ids?: string[]) => {
      const chosen = targets(ids);
      if (!task || !canSend || chosen.length === 0) return false;
      // Quote lines that could not be read when the comment was written.
      const filled = await Promise.all(chosen.map(async (c) => (c.code !== null ? c : { ...c, code: await readLines(api, c) })));
      try {
        await send.mutateAsync({ task, text: formatReviewMessage(filled, t), ids: chosen.map((c) => c.id) });
        return true;
      } catch {
        // `useAction` already showed the error.
        return false;
      }
    },
    [targets, task, canSend, api, send, t],
  );

  const commentsFor = useCallback(
    (copy: CodeCopy, path?: string) => listOf(copyKey(copy)).filter((c) => path === undefined || c.path === path),
    [listOf],
  );

  const selectFile = useCallback(
    (path: string, line?: number) => setSelection((s) => ({ key: currentKey, path, line: line ?? null, seq: s.seq + 1 })),
    [currentKey],
  );

  // "Open the diff of this file" from anywhere (search, explorer, palette).
  const [navSeen, setNavSeen] = useState(nav.seq);
  if (navSeen !== nav.seq) {
    setNavSeen(nav.seq);
    const target = nav.current;
    if (target?.view === "diff" && taskCopy && sameCopy(target.copy, taskCopy)) {
      setSelection((s) => ({ key: currentKey, path: target.path, line: target.line ?? null, seq: s.seq + 1 }));
    }
  }

  const own = selection.key === currentKey;
  const value = useMemo<ReviewComments>(
    () => ({
      comments,
      pending,
      commentsFor,
      add,
      update,
      remove,
      message,
      sendToAi,
      sending: send.isPending,
      canSend,
      selectFile,
      selectedFile: own ? selection.path : null,
      selectedLine: own ? selection.line : null,
      selectSeq: own ? selection.seq : 0,
    }),
    [comments, pending, commentsFor, add, update, remove, message, sendToAi, send.isPending, canSend, selectFile, own, selection],
  );
  return <CommentsContext.Provider value={value}>{children}</CommentsContext.Provider>;
}

/** The review comments; outside `ReviewCommentsProvider`, an inert store (tests of single components). */
export function useReviewComments(): ReviewComments {
  return useContext(CommentsContext);
}
