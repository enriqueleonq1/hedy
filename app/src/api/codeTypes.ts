// Code navigation and review (read-only). Mirrors `src-tauri/src/code.rs` and
// `hedy-core::domain::code`. Every call names the copy it reads explicitly, so
// a view never mixes the user's folder with a task's worktree.

import type { DiffLine, TaskStatus } from "./types";

/**
 * Which copy of the code to read: the user's own folder (a registered project,
 * as it is on disk) or a task's worktree. Once a task is accepted its worktree
 * is gone and its files are read from the task's last commit (`CopyInfo.live`
 * is `false`); a discarded task has no files left (the call rejects).
 */
export type CodeCopy = { kind: "folder"; project: string } | { kind: "task"; task: string };

/**
 * A revision to read or compare with: `"base"` is the copy's base (for a task,
 * the commit it started from — "antes de la IA"; for the folder, `HEAD`);
 * anything else is a commit hash, branch or tag (`HEAD~1`). Never an option or
 * a range: those are rejected.
 */
export type Rev = "base" | (string & {});

/** What a copy is, for the header of every code view ("Tu carpeta" / "Tarea: …"). */
export interface CopyInfo {
  kind: "folder" | "task";
  project: string;
  task: string | null;
  title: string | null;
  /** Absolute folder holding its files (join with a relative path to copy the absolute one). */
  root: string;
  /** `false`: a read-only snapshot of `snapshot` (an accepted task); nothing on disk to edit. */
  live: boolean;
  snapshot: string | null;
  /** The commit "base" stands for; `null` in a repository without commits. */
  base: string | null;
  /** The task's branch, or the folder's current one. */
  branch: string | null;
  /** The branch the task forked from. */
  baseBranch: string | null;
  /** The task's agent is working on these files (or waits for an answer): lock editing. */
  agentWorking: boolean;
}

/**
 * How a file differs from the copy's base. A task never reports `untracked`
 * (a file it created is `added`); the folder does for files git does not know yet.
 */
export type ChangeKind = "added" | "modified" | "deleted" | "renamed" | "untracked";

export interface TreeEntry {
  /** Relative, `/`-separated. Ignored folders end with `/` (their contents are not listed). */
  path: string;
  /** `null`: unchanged since the base. Deleted files are included with `deleted`. */
  status: ChangeKind | null;
  /** The previous path of a rename. */
  oldPath: string | null;
  /** A protected file (`.env`, keys…): shown, never read, diffed, blamed or searched. */
  locked: boolean;
  /** Ignored by git; only present when asked for with `{ ignored: true }`. */
  ignored: boolean;
}

export interface CodeTree {
  copy: CopyInfo;
  /** Every file, sorted by path (folders are implied by the paths). */
  entries: TreeEntry[];
  /** Cut at 100 000 entries. */
  truncated: boolean;
}

/** A file's contents as the UI can show them. */
export type FileContent =
  | { kind: "text"; text: string; size: number }
  /** PNG, JPEG, GIF or WebP up to 5 MB; `dataUrl` goes straight into an `<img>`. SVG comes as text. */
  | { kind: "image"; mime: string; size: number; dataUrl: string }
  | { kind: "binary"; size: number }
  /** Text over 2 MB (or an image over 5 MB): offer "open in editor". */
  | { kind: "tooLarge"; size: number }
  /** Protected: never read. */
  | { kind: "locked" }
  /** Not in that copy or revision (e.g. "before the AI" of a new file). */
  | { kind: "missing" };

export interface DiffHunk {
  /** `@@ -12,7 +12,9 @@ context`. */
  header: string;
  lines: DiffLine[];
}

/** The shape of one file's diff. */
export type DiffBody =
  /** No hunks: the file is the same on both sides. */
  | { kind: "text"; additions: number; deletions: number; hunks: DiffHunk[] }
  | { kind: "binary" }
  | { kind: "tooLarge"; size: number }
  | { kind: "locked" };

/**
 * One file's diff against a revision. To "expand context" between two hunks,
 * either ask again with `{ full: true }` (the whole file as context) or read the
 * new side with `readFile(copy, path)`: lines between hunks are unchanged, so
 * line `n` of the new file is line `n - offset` of the old one, where `offset`
 * is `newLine - oldLine` of the context line just before the gap.
 */
export interface CodeDiff {
  path: string;
  oldPath: string | null;
  /** `null`: the same on both sides. */
  status: ChangeKind | null;
  /** The commit compared against, resolved (full hash when git knows it). */
  against: string;
  body: DiffBody;
}

export interface DiffOptions {
  /** Default `"base"`. */
  against?: Rev;
  /** Lines of context around each change (3 by default). */
  context?: number;
  /** The whole file as context. */
  full?: boolean;
}

export interface SearchOptions {
  caseSensitive?: boolean;
  wholeWord?: boolean;
  /** POSIX extended regular expression (`git grep -E`); an invalid one rejects with git's message. */
  regex?: boolean;
  /** Comma-separated globs (`*.py, src/**`); a glob without `/` matches in any folder; `!` excludes. */
  glob?: string;
}

/** Where a match is: UTF-16 offsets in `text` (what `String.prototype.slice` takes), end exclusive. */
export interface MatchRange {
  start: number;
  end: number;
}

export interface LineMatch {
  /** 1-based. */
  line: number;
  /** The line, cut to 400 characters. */
  text: string;
  /** Every match in `text`; empty when it could not be located exactly. */
  ranges: MatchRange[];
}

export interface FileMatches {
  path: string;
  matches: LineMatch[];
}

export interface SearchResults {
  /** In path order. Protected files are never searched. */
  files: FileMatches[];
  /** Matching lines returned. */
  total: number;
  /** More than 2 000 lines matched: refine the query. */
  truncated: boolean;
}

export interface CommitInfo {
  hash: string;
  short: string;
  author: string;
  /** Author date, Unix seconds. */
  date: number;
  subject: string;
  /** The file's path in that commit (differs before a rename). */
  path: string;
}

export interface HistoryCommit extends CommitInfo {
  /** The Hedy task this commit came from, when it was one. */
  task: string | null;
}

/** A Hedy task of the project that changed the file. */
export interface TaskTouch {
  task: string;
  title: string;
  status: TaskStatus;
  /** When it was accepted, or created while still open (Unix seconds). */
  at: number;
  /** Its commit in `commits`, once accepted. */
  commit: string | null;
  /** Still open: the change only lives in its worktree. */
  open: boolean;
}

/** A file's timeline: merge `commits` and `tasks` by date (`date` / `at`). */
export interface FileHistory {
  /** Newest first, at most 200, following renames. */
  commits: HistoryCommit[];
  /** Newest first. */
  tasks: TaskTouch[];
}

export interface BlameCommit {
  hash: string;
  short: string;
  author: string;
  /** Author date, Unix seconds. */
  date: number;
  summary: string;
  /** Lines the copy changed and nobody committed yet (show "sin commit", not `author`). */
  uncommitted: boolean;
}

export interface BlameRange {
  /** 1-based first line. */
  start: number;
  count: number;
  /** A hash in `Blame.commits`. */
  commit: string;
}

export interface Blame {
  commits: BlameCommit[];
  /** Consecutive lines by the same commit, in order, covering the file (or the range asked for). */
  ranges: BlameRange[];
}

/** Lines `start..=end`, 1-based. */
export interface LineRange {
  start: number;
  end: number;
}

export type SymbolKind =
  | "function"
  | "method"
  | "class"
  | "struct"
  | "enum"
  | "trait"
  | "interface"
  | "type"
  | "module"
  | "constant"
  | "heading";

/**
 * A function, type or heading of a file. A light outline read line by line
 * (Rust, Python, JS/TS, Go, Ruby, the C-like family by keywords, Markdown
 * headings), not a parser: multi-line signatures and Java/C# methods without a
 * keyword are missed.
 */
export interface CodeSymbol {
  name: string;
  kind: SymbolKind;
  /** 1-based. */
  line: number;
  /** Nesting by indentation (0 = top level); headings: `#` count − 1. */
  depth: number;
}
