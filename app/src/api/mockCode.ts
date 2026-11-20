// Code navigation for `MockApi`: the same rules as the backend (copies, base,
// protected files, limits) over the sample repositories in `mockCodeSamples.ts`.
// Older commits are approximated: a file exists in a commit when one of its
// commits is that old, with its `HEAD` contents.

import type {
  Blame,
  BlameCommit,
  ChangeKind,
  CodeCopy,
  CodeDiff,
  CodeSymbol,
  CodeTree,
  CopyInfo,
  DiffOptions,
  FileContent,
  FileHistory,
  LineMatch,
  LineRange,
  Rev,
  SearchOptions,
  SearchResults,
  SymbolKind,
  TaskTouch,
  TreeEntry,
  WriteResult,
} from "./codeTypes";
import { MISSING_VERSION } from "./codeTypes";
import { diffOps, lines, textDiff } from "./lineDiff";

export { textDiff };
import { SAMPLE_REPOS, type SampleFile, type SampleRepo } from "./mockCodeSamples";
import type { FileDiff, Task } from "./types";

/** Same as `permissions::SUGGESTED_BLOCKED` in the backend: the sample projects keep these from the AI. */
export const SUGGESTED_BLOCKED = [".env", ".env.*", "*.pem", "*.key", "id_rsa*", "id_ed25519*", ".npmrc", ".pypirc", "credentials*"];
const MAX_TEXT_BYTES = 2 * 1024 * 1024;
const MAX_SEARCH_MATCHES = 2_000;
const MAX_LINE_CHARS = 400;
const UNCOMMITTED = "0".repeat(40);

function globToRegExp(glob: string): RegExp {
  let out = "";
  for (let i = 0; i < glob.length; i++) {
    const c = glob.charAt(i);
    if (c === "*" && glob[i + 1] === "*") {
      out += ".*";
      i++;
      if (glob[i + 1] === "/") i++;
    } else if (c === "*") out += "[^/]*";
    else if (c === "?") out += "[^/]";
    else out += c.replace(/[.+^${}()|[\]\\]/g, "\\$&");
  }
  return new RegExp(`^${out}$`);
}

/** A protected file (`.env`, keys…): never read. */
export function isProtectedPath(path: string): boolean {
  const name = path.replace(/\/$/, "").split("/").pop() ?? path;
  return SUGGESTED_BLOCKED.some((pattern) => globToRegExp(pattern).test(name));
}

/** The backend's `normalize_code_path`: relative, inside the copy, no `.git`. */
export function normalizeCodePath(path: string): string | null {
  if (!path || path.includes("\0")) return null;
  const clean = path.replace(/\\/g, "/");
  if (clean.startsWith("/") || clean.startsWith("~") || /^[A-Za-z]:/.test(clean)) return null;
  const parts: string[] = [];
  for (const part of clean.split("/")) {
    if (part === "" || part === ".") continue;
    if (part === ".." || part.toLowerCase() === ".git") return null;
    parts.push(part);
  }
  return parts.length ? parts.join("/") : null;
}

function isSafeRev(rev: string): boolean {
  return rev.length > 0 && rev.length <= 200 && !rev.startsWith("-") && !rev.includes("..") && /^[A-Za-z0-9._/~^-]+$/.test(rev);
}

function byteLength(text: string): number {
  return new TextEncoder().encode(text).length;
}

/** A file's version like the backend's (`content_version`): a hash of its text and its length. */
export function textVersion(text: string): string {
  let hash = 0x811c9dc5;
  for (let i = 0; i < text.length; i++) {
    hash ^= text.charCodeAt(i);
    hash = Math.imul(hash, 0x01000193) >>> 0;
  }
  return `${hash.toString(16).padStart(8, "0")}-${byteLength(text)}`;
}

/** Like the backend's `is_valid_file_name`. */
function validName(name: string): boolean {
  if (!name || name === "." || name === ".." || /[. ]$/.test(name) || name.startsWith(" ")) return false;
  // eslint-disable-next-line no-control-regex
  return !/[\u0000-\u001f/\\:*?"<>|]/.test(name);
}

/** A file of a copy: the sample's kind, or text that replaced it. */
type Entry = SampleFile | { kind: "text"; text: string; commits: string[] };

interface Resolved {
  info: CopyInfo;
  repo: SampleRepo | null;
  /** Changes against `HEAD`; `null` = deleted. */
  overrides: Record<string, string | null>;
  untracked: Set<string>;
  task: boolean;
}

export interface MockCodeHost {
  task(id: string): Task | undefined;
  tasks(project: string): Task[];
  /** Throws when `path` is not a registered project. */
  checkProject(path: string): void;
}

/** The code-navigation half of `MockApi`. */
export class MockCode {
  /** Tests: what `openFileInEditor` and `revealInFileManager` were asked. */
  opened: { path: string; line: number | null }[] = [];
  revealed: string[] = [];

  constructor(
    private host: MockCodeHost,
    repos: Record<string, SampleRepo> = SAMPLE_REPOS,
  ) {
    // Edits change the repositories: each instance gets its own.
    this.repos = structuredClone(repos);
  }

  private repos: Record<string, SampleRepo>;
  /** Folders created in each copy (git lists no empty folders). */
  private folders = new Map<string, Set<string>>();
  /** Tests: what was moved to the trash (`<root>/<path>`). */
  trashed: string[] = [];

  private folder(project: string, task: Task | null): Resolved {
    const repo = this.repos[project] ?? null;
    return {
      info: {
        kind: task ? "task" : "folder",
        project,
        task: task?.id ?? null,
        title: task?.title ?? null,
        root: project,
        live: true,
        snapshot: null,
        base: repo?.commits[0]?.hash ?? null,
        branch: repo?.branch ?? null,
        baseBranch: null,
        agentWorking: !!task && (task.status === "running" || task.status === "waitingApproval"),
      },
      repo,
      overrides: { ...(repo?.folderChanges ?? {}), ...(repo?.untracked ?? {}) },
      untracked: new Set(Object.keys(repo?.untracked ?? {})),
      task: false,
    };
  }

  private resolve(copy: CodeCopy): Resolved {
    if (copy.kind === "folder") {
      this.host.checkProject(copy.project);
      return this.folder(copy.project, null);
    }
    const task = this.host.task(copy.task);
    if (!task) throw `no existe la tarea ${copy.task}`;
    const unavailable = `los archivos de la tarea ${task.id} ya no están disponibles`;
    if (task.conversation) throw unavailable;
    const ws = task.workspace;
    if (!ws) return this.folder(task.project, task);
    if (task.status === "discarded" || (task.status === "accepted" && !task.merge)) throw unavailable;
    const repo = this.repos[task.project] ?? null;
    const accepted = task.status === "accepted";
    return {
      info: {
        kind: "task",
        project: task.project,
        task: task.id,
        title: task.title,
        root: accepted ? task.project : ws.worktree,
        live: !accepted,
        snapshot: accepted ? (task.merge?.tip ?? null) : null,
        base: ws.baseCommit,
        branch: ws.branch,
        baseBranch: ws.baseBranch,
        agentWorking: task.status === "running" || task.status === "waitingApproval",
      },
      repo,
      overrides: { ...(repo?.tasks[task.id] ?? {}) },
      untracked: new Set(),
      task: true,
    };
  }

  /** Files at `HEAD`, or at an older sample commit. */
  private baseFiles(repo: SampleRepo | null, rev: string | null): Map<string, Entry> {
    const files = new Map<string, Entry>();
    if (!repo) return files;
    const index = rev === null ? 0 : repo.commits.findIndex((c) => c.hash === rev || rev.startsWith(c.hash));
    for (const [path, file] of Object.entries(repo.files)) {
      const created = file.commits.length ? repo.commits.findIndex((c) => c.hash === file.commits[file.commits.length - 1]) : 0;
      if (created >= index || created === -1) files.set(path, file);
    }
    return files;
  }

  private files(copy: Resolved, rev?: Rev): Map<string, Entry> {
    if (rev === undefined || rev === null || rev === "") {
      const files = this.baseFiles(copy.repo, null);
      for (const [path, text] of Object.entries(copy.overrides)) {
        if (text === null) files.delete(path);
        else files.set(path, { kind: "text", text, commits: files.get(path)?.commits ?? [] });
      }
      return files;
    }
    const target = rev === "base" ? copy.info.base : rev;
    if (!isSafeRev(rev)) throw `«${rev}» no es un commit válido`;
    if (target === "HEAD" || target === copy.info.base) return this.baseFiles(copy.repo, null);
    if (copy.repo?.commits.some((c) => target && (c.hash === target || target.startsWith(c.hash)))) {
      return this.baseFiles(copy.repo, target);
    }
    throw `git no conoce el commit «${rev}»`;
  }

  private changes(copy: Resolved): Map<string, ChangeKind> {
    const base = this.baseFiles(copy.repo, null);
    const out = new Map<string, ChangeKind>();
    for (const [path, text] of Object.entries(copy.overrides)) {
      if (text === null) out.set(path, "deleted");
      else if (!base.has(path)) out.set(path, copy.untracked.has(path) && !copy.task ? "untracked" : "added");
      else out.set(path, "modified");
    }
    return out;
  }

  private checked(path: string): string {
    const clean = normalizeCodePath(path);
    if (!clean) throw `«${path}» no es un archivo dentro de esta copia`;
    return clean;
  }

  copy(copy: CodeCopy): CopyInfo {
    return { ...this.resolve(copy).info };
  }

  tree(copy: CodeCopy, ignored = false): CodeTree {
    const resolved = this.resolve(copy);
    const changes = this.changes(resolved);
    const paths = new Set([...this.files(resolved).keys(), ...changes.keys()]);
    const entries: TreeEntry[] = [...paths].map((path) => ({
      path,
      status: changes.get(path) ?? null,
      oldPath: null,
      locked: isProtectedPath(path),
      ignored: false,
    }));
    if (ignored && resolved.info.live) {
      for (const path of resolved.repo?.ignored ?? []) {
        entries.push({ path, status: null, oldPath: null, locked: isProtectedPath(path), ignored: true });
      }
    }
    entries.sort((a, b) => (a.path < b.path ? -1 : a.path > b.path ? 1 : 0));
    return { copy: resolved.info, entries, truncated: false };
  }

  private content(entry: Entry | undefined, path: string): FileContent {
    if (isProtectedPath(path)) return { kind: "locked" };
    if (!entry) return { kind: "missing" };
    switch (entry.kind) {
      case "text": {
        const size = byteLength(entry.text);
        return size > MAX_TEXT_BYTES
          ? { kind: "tooLarge", size }
          : { kind: "text", text: entry.text, size, version: textVersion(entry.text) };
      }
      case "image": {
        const size = Math.floor((entry.base64.length * 3) / 4) - (entry.base64.match(/=+$/)?.[0].length ?? 0);
        return { kind: "image", mime: entry.mime, size, dataUrl: `data:${entry.mime};base64,${entry.base64}` };
      }
      case "binary":
        return { kind: "binary", size: entry.size };
      case "large":
        return { kind: "tooLarge", size: entry.size };
      case "locked":
        return { kind: "locked" };
    }
  }

  readFile(copy: CodeCopy, path: string, rev?: Rev): FileContent {
    const resolved = this.resolve(copy);
    const clean = this.checked(path);
    return this.content(this.files(resolved, rev).get(clean), clean);
  }

  fileDiff(copy: CodeCopy, path: string, options: DiffOptions = {}): CodeDiff {
    const resolved = this.resolve(copy);
    const clean = this.checked(path);
    const against = options.against ?? "base";
    const before = this.files(resolved, against).get(clean);
    const after = this.files(resolved).get(clean);
    const status = against === "base" ? (this.changes(resolved).get(clean) ?? null) : null;
    const result: CodeDiff = {
      path: clean,
      oldPath: null,
      status,
      against: against === "base" ? (resolved.info.base ?? "") : against,
      body: { kind: "locked" },
    };
    if (isProtectedPath(clean)) return result;
    const side = (entry: Entry | undefined) => this.content(entry, clean);
    const [old, now] = [side(before), side(after)];
    const tooLarge = [old, now].find((c) => c.kind === "tooLarge");
    if (tooLarge && tooLarge.kind === "tooLarge") return { ...result, body: { kind: "tooLarge", size: tooLarge.size } };
    if (old.kind === "image" || old.kind === "binary" || now.kind === "image" || now.kind === "binary") {
      const same = before === after;
      return { ...result, status: same ? null : (result.status ?? "modified"), body: same ? { kind: "text", additions: 0, deletions: 0, hunks: [] } : { kind: "binary" } };
    }
    const text = (c: FileContent) => (c.kind === "text" ? c.text : null);
    const context = options.full ? Number.MAX_SAFE_INTEGER : (options.context ?? 3);
    const diff = textDiff(clean, text(old), text(now), context);
    if (!diff) return { ...result, status: null, body: { kind: "text", additions: 0, deletions: 0, hunks: [] } };
    return {
      ...result,
      status: result.status ?? diff.status,
      body: { kind: "text", additions: diff.additions, deletions: diff.deletions, hunks: diff.hunks },
    };
  }

  searchText(copy: CodeCopy, query: string, options: SearchOptions = {}): SearchResults {
    const resolved = this.resolve(copy);
    if (!query.trim()) return { files: [], total: 0, truncated: false };
    const source = options.regex ? query : query.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
    let matcher: RegExp;
    try {
      matcher = new RegExp(options.wholeWord ? `\\b(?:${source})\\b` : source, options.caseSensitive ? "g" : "gi");
    } catch {
      throw `git grep: expresión regular no válida: ${query}`;
    }
    const globs = (options.glob ?? "")
      .split(",")
      .map((g) => g.trim())
      .filter(Boolean)
      .map((g) => {
        const exclude = g.startsWith("!");
        const glob = exclude ? g.slice(1).trim() : g;
        return { exclude, re: globToRegExp(glob.includes("/") ? glob.replace(/^\//, "") : `**/${glob}`) };
      });
    const included = (path: string) =>
      (globs.every((g) => g.exclude) || globs.some((g) => !g.exclude && g.re.test(path))) &&
      !globs.some((g) => g.exclude && g.re.test(path));
    const results: SearchResults = { files: [], total: 0, truncated: false };
    const files = [...this.files(resolved).entries()].sort(([a], [b]) => (a < b ? -1 : 1));
    for (const [path, entry] of files) {
      if (entry.kind !== "text" || isProtectedPath(path) || !included(path)) continue;
      const matches: LineMatch[] = [];
      lines(entry.text).forEach((line, i) => {
        const text = line.slice(0, MAX_LINE_CHARS);
        const ranges = [...text.matchAll(matcher)].filter((m) => m[0].length > 0).map((m) => ({ start: m.index ?? 0, end: (m.index ?? 0) + m[0].length }));
        if (ranges.length) matches.push({ line: i + 1, text, ranges });
      });
      for (const match of matches) {
        if (results.total === MAX_SEARCH_MATCHES) {
          results.truncated = true;
          return results;
        }
        const last = results.files[results.files.length - 1];
        if (last?.path === path) last.matches.push(match);
        else results.files.push({ path, matches: [match] });
        results.total++;
      }
    }
    return results;
  }

  /** Who wrote each line of the sample file at `HEAD`, spread over its commits. */
  private baseBlame(repo: SampleRepo | null, path: string, count: number): string[] {
    const commits = (repo?.files[path]?.commits ?? []).filter((hash) => repo?.commits.some((c) => c.hash === hash));
    if (!commits.length) return Array<string>(count).fill(UNCOMMITTED);
    const owners = Array<string>(count).fill(commits.at(-1) ?? UNCOMMITTED);
    commits.slice(0, -1).forEach((hash, i) => {
      const start = Math.floor(((i + 1) * count) / (commits.length + 1));
      const length = Math.max(1, Math.floor(count / (commits.length + 2)));
      for (let line = start; line < Math.min(count, start + length); line++) owners[line] = hash;
    });
    return owners;
  }

  /** Line owners of the copy's file (or of `rev`). */
  private owners(resolved: Resolved, path: string, rev?: Rev): string[] {
    const entry = this.files(resolved, rev).get(path);
    if (!entry || entry.kind !== "text") throw `git blame: no existe ${path}`;
    const baseEntry = this.baseFiles(resolved.repo, null).get(path);
    const baseText = baseEntry?.kind === "text" ? baseEntry.text : null;
    const base = this.baseBlame(resolved.repo, path, lines(baseText ?? "").length);
    if (baseText === entry.text) return base;
    return diffOps(lines(baseText ?? ""), lines(entry.text))
      .filter((op) => op.kind !== "removed")
      .map((op) => (op.kind === "added" ? UNCOMMITTED : (base[(op.oldLine ?? 0) - 1] ?? UNCOMMITTED)));
  }

  blame(copy: CodeCopy, path: string, options: { rev?: Rev; lines?: LineRange } = {}): Blame {
    const resolved = this.resolve(copy);
    const clean = this.checked(path);
    if (isProtectedPath(clean)) throw `${clean} está protegido: Hedy no lo lee; ábrelo en tu editor`;
    const owners = this.owners(resolved, clean, options.rev);
    const first = Math.max(1, options.lines?.start ?? 1);
    const last = Math.min(owners.length, options.lines?.end ?? owners.length);
    const blame: Blame = { commits: [], ranges: [] };
    for (let line = first; line <= last; line++) {
      const hash = owners[line - 1] ?? UNCOMMITTED;
      if (!blame.commits.some((c) => c.hash === hash)) blame.commits.push(this.blameCommit(resolved.repo, hash));
      const range = blame.ranges[blame.ranges.length - 1];
      if (range && range.commit === hash && range.start + range.count === line) range.count++;
      else blame.ranges.push({ start: line, count: 1, commit: hash });
    }
    return blame;
  }

  private blameCommit(repo: SampleRepo | null, hash: string): BlameCommit {
    const commit = repo?.commits.find((c) => c.hash === hash);
    return {
      hash,
      short: hash.slice(0, 7),
      author: commit?.author ?? "",
      date: commit?.date ?? 0,
      summary: commit?.subject ?? "",
      uncommitted: !commit,
    };
  }

  fileHistory(copy: CodeCopy, path: string, range?: LineRange): FileHistory {
    const resolved = this.resolve(copy);
    const clean = this.checked(path);
    const repo = resolved.repo;
    let hashes = repo?.files[clean]?.commits ?? [];
    if (range && repo?.files[clean]?.kind === "text") {
      const owners = this.baseBlame(repo, clean, lines((repo.files[clean] as { text: string }).text).length);
      const touched = new Set(owners.slice(Math.max(0, range.start - 1), range.end));
      hashes = hashes.filter((h) => touched.has(h));
    }
    const commits = hashes
      .map((hash) => repo?.commits.find((c) => c.hash === hash))
      .filter((c) => c !== undefined)
      .map((c) => ({ hash: c.hash, short: c.hash.slice(0, 7), author: c.author, date: c.date, subject: c.subject, path: clean, task: null }));
    const tasks: TaskTouch[] = this.host
      .tasks(resolved.info.project)
      .filter((t) => t.workspace && !t.conversation && t.status !== "discarded" && clean in (repo?.tasks[t.id] ?? {}))
      .map((t) => ({
        task: t.id,
        title: t.title,
        status: t.status,
        at: t.merge?.at ?? t.createdAt,
        commit: t.status === "accepted" ? (t.merge?.tip ?? null) : null,
        open: t.status !== "accepted",
      }))
      .sort((a, b) => b.at - a.at);
    return { commits, tasks };
  }

  fileSymbols(copy: CodeCopy, path: string): CodeSymbol[] {
    const content = this.readFile(copy, path);
    return content.kind === "text" ? outline(this.checked(path), content.text) : [];
  }

  openFileInEditor(copy: CodeCopy, path: string, line?: number): void {
    const resolved = this.resolve(copy);
    this.opened.push({ path: `${resolved.info.root}/${this.checked(path)}`, line: line ?? null });
  }

  revealInFileManager(copy: CodeCopy, path: string): void {
    const resolved = this.resolve(copy);
    const file = `${resolved.info.root}/${this.checked(path)}`;
    this.revealed.push(file.slice(0, file.lastIndexOf("/")));
  }

  // ---------------------------------------------------------------- editing

  private copyKey(info: CopyInfo): string {
    return info.task ? `task:${info.task}` : `folder:${info.project}`;
  }

  /** A copy that can be written now (like the backend's `writable_copy`). */
  private writable(copy: CodeCopy): Resolved {
    const resolved = this.resolve(copy);
    if (!resolved.info.live) throw "es una instantánea de solo lectura (la tarea ya se integró): edita en tu carpeta";
    if (resolved.info.agentWorking) {
      throw `la IA está trabajando en la copia de la tarea ${resolved.info.task ?? ""}; podrás editar cuando termine`;
    }
    return resolved;
  }

  private writablePath(path: string): string {
    const clean = this.checked(path);
    if (clean === ".hedy" || clean.startsWith(".hedy/")) throw `«${path}» no es un archivo dentro de esta copia`;
    if (isProtectedPath(clean)) throw `${clean} está protegido: Hedy no lo lee; ábrelo en tu editor`;
    return clean;
  }

  /** Where a copy's own changes live in the sample repository. */
  private changeSet(resolved: Resolved): { repo: SampleRepo; changes: Record<string, string | null> } {
    const project = resolved.info.project;
    const repo = (this.repos[project] ??= {
      branch: "main",
      commits: [],
      files: {},
      folderChanges: {},
      untracked: {},
      ignored: [],
      tasks: {},
    });
    if (resolved.task && resolved.info.task) return { repo, changes: (repo.tasks[resolved.info.task] ??= {}) };
    return { repo, changes: repo.folderChanges };
  }

  /** Sets (or, with `null`, deletes) a working file of the copy. */
  private put(resolved: Resolved, path: string, text: string | null): void {
    const { repo, changes } = this.changeSet(resolved);
    const base = repo.files[path];
    const folderCopy = !resolved.task;
    if (folderCopy) Reflect.deleteProperty(repo.untracked, path);
    if (text === null) {
      if (base) changes[path] = null;
      else Reflect.deleteProperty(changes, path);
    } else if (folderCopy && !base) {
      repo.untracked[path] = text;
      Reflect.deleteProperty(changes, path);
    } else if (base?.kind === "text" && base.text === text) {
      Reflect.deleteProperty(changes, path);
    } else {
      changes[path] = text;
    }
  }

  private kindAt(resolved: Resolved, path: string): "file" | "folder" | null {
    const files = this.files(resolved);
    if (files.has(path)) return "file";
    const prefix = `${path}/`;
    if ([...files.keys()].some((p) => p.startsWith(prefix))) return "folder";
    return this.folders.get(this.copyKey(resolved.info))?.has(path) ? "folder" : null;
  }

  private checkNewName(path: string): void {
    const name = path.slice(path.lastIndexOf("/") + 1);
    if (!validName(name)) throw `«${name}» no es un nombre válido para un archivo o carpeta`;
  }

  /** Tests: someone else (an agent) changed a file of the copy (`null` deletes it). */
  setText(copy: CodeCopy, path: string, text: string | null): void {
    this.put(this.resolve(copy), this.checked(path), text);
  }

  writeFile(copy: CodeCopy, path: string, text: string, expected: string): WriteResult {
    const resolved = this.writable(copy);
    const clean = this.writablePath(path);
    const entry = this.files(resolved).get(clean);
    if (!entry) {
      if (expected !== MISSING_VERSION) return { kind: "conflict", text: null, version: MISSING_VERSION };
      this.put(resolved, clean, text);
      return { kind: "saved", version: textVersion(text), size: byteLength(text) };
    }
    if (entry.kind !== "text") throw `${clean} no se puede editar en Hedy (binario, muy grande o no es UTF-8): ábrelo en tu editor`;
    const version = textVersion(entry.text);
    if (version !== expected) return { kind: "conflict", text: entry.text, version };
    const next = entry.text.endsWith("\n") && text !== "" && !text.endsWith("\n") ? `${text}\n` : text;
    this.put(resolved, clean, next);
    return { kind: "saved", version: textVersion(next), size: byteLength(next) };
  }

  createFile(copy: CodeCopy, path: string): { version: string; size: number } {
    const resolved = this.writable(copy);
    const clean = this.writablePath(path);
    this.checkNewName(clean);
    if (this.kindAt(resolved, clean)) throw `«${clean}» ya existe`;
    this.put(resolved, clean, "");
    return { version: textVersion(""), size: 0 };
  }

  createFolder(copy: CodeCopy, path: string): void {
    const resolved = this.writable(copy);
    const clean = this.writablePath(path);
    this.checkNewName(clean);
    if (this.kindAt(resolved, clean)) throw `«${clean}» ya existe`;
    const key = this.copyKey(resolved.info);
    const set = this.folders.get(key) ?? new Set<string>();
    set.add(clean);
    this.folders.set(key, set);
  }

  /** The files under a path (itself when it is a file), refusing protected ones inside a folder. */
  private movable(resolved: Resolved, path: string): string[] {
    const kind = this.kindAt(resolved, path);
    if (!kind) throw `«${path}» no existe`;
    if (kind === "file") return [path];
    const inside = [...this.files(resolved).keys()].filter((p) => p.startsWith(`${path}/`));
    const locked = inside.find((p) => isProtectedPath(p));
    if (locked) throw `${locked} está protegido: Hedy no lo lee; ábrelo en tu editor`;
    return inside;
  }

  renamePath(copy: CodeCopy, from: string, to: string): void {
    const resolved = this.writable(copy);
    const source = this.writablePath(from);
    const target = this.writablePath(to);
    if (source === target) return;
    this.checkNewName(target);
    const moved = this.movable(resolved, source);
    if (source.toLowerCase() !== target.toLowerCase() && this.kindAt(resolved, target)) throw `«${target}» ya existe`;
    const files = this.files(resolved);
    for (const path of moved) {
      const entry = files.get(path);
      const text = entry?.kind === "text" ? entry.text : "";
      this.put(resolved, path, null);
      this.put(resolved, `${target}${path.slice(source.length)}`, text);
    }
    const set = this.folders.get(this.copyKey(resolved.info));
    if (set?.delete(source)) set.add(target);
  }

  deletePath(copy: CodeCopy, path: string): void {
    const resolved = this.writable(copy);
    const clean = this.writablePath(path);
    for (const file of this.movable(resolved, clean)) this.put(resolved, file, null);
    this.folders.get(this.copyKey(resolved.info))?.delete(clean);
    this.trashed.push(`${resolved.info.root}/${clean}`);
  }

  duplicateFile(copy: CodeCopy, path: string): string {
    const resolved = this.writable(copy);
    const clean = this.writablePath(path);
    const entry = this.files(resolved).get(clean);
    if (!entry) throw `«${clean}» no existe`;
    if (entry.kind !== "text") throw `${clean} no se puede editar en Hedy (binario, muy grande o no es UTF-8): ábrelo en tu editor`;
    const name = clean.slice(clean.lastIndexOf("/") + 1);
    const dot = name.lastIndexOf(".");
    const cut = dot > 0 ? clean.length - (name.length - dot) : clean.length;
    const [stem, ext] = [clean.slice(0, cut), clean.slice(cut)];
    for (let n = 1; n <= 100; n++) {
      const candidate = n === 1 ? `${stem} copia${ext}` : `${stem} copia ${n}${ext}`;
      if (!this.kindAt(resolved, candidate)) {
        this.put(resolved, candidate, entry.text);
        return candidate;
      }
    }
    throw `«${stem} copia${ext}» ya existe`;
  }

  /** Whether a task's changes come from the sample repository (its diff is computed from them). */
  hasTaskChanges(project: string, task: string): boolean {
    return !!this.repos[project]?.tasks[task];
  }

  /** A task's changes as the review panel shows them, from the sample repository. */
  taskDiff(project: string, task: string): FileDiff[] {
    const repo = this.repos[project];
    const changes = repo?.tasks[task];
    if (!repo || !changes) return [];
    return Object.entries(changes)
      .sort(([a], [b]) => (a < b ? -1 : 1))
      .map(([path, after]) => {
        const base = repo.files[path];
        return textDiff(path, base?.kind === "text" ? base.text : null, after);
      })
      .filter((d): d is FileDiff => d !== null);
  }
}

const SYMBOL_RULES: Record<string, [RegExp, SymbolKind][]> = {
  python: [
    [/^(?:async\s+)?def\s+([A-Za-z_]\w*)/, "function"],
    [/^class\s+([A-Za-z_]\w*)/, "class"],
  ],
  js: [
    [/^(?:export\s+)?(?:default\s+)?(?:async\s+)?function\*?\s+([A-Za-z_$][\w$]*)/, "function"],
    [/^(?:export\s+)?(?:default\s+)?(?:abstract\s+)?class\s+([A-Za-z_$][\w$]*)/, "class"],
    [/^(?:export\s+)?interface\s+([A-Za-z_$][\w$]*)/, "interface"],
    [/^(?:export\s+)?type\s+([A-Za-z_$][\w$]*)\s*[=<]/, "type"],
    [/^(?:export\s+)?enum\s+([A-Za-z_$][\w$]*)/, "enum"],
    [/^(?:export\s+)?(?:const|let)\s+([A-Za-z_$][\w$]*)\s*=\s*(?:async\s*)?(?:\([^)]*\)|[A-Za-z_$][\w$]*)\s*(?::[^=]+)?=>/, "function"],
    [/^(?:public\s+|private\s+|protected\s+)?(?:async\s+)?([A-Za-z_$][\w$]*)\s*\([^)]*\)\s*(?::[^{]+)?\{$/, "method"],
  ],
};

/** A light outline like the backend's (`domain::code::outline`), for the sample languages. */
function outline(path: string, text: string): CodeSymbol[] {
  const ext = path.split(".").pop()?.toLowerCase() ?? "";
  const symbols: CodeSymbol[] = [];
  if (["md", "markdown", "mdx"].includes(ext)) {
    let fence = false;
    lines(text).forEach((raw, i) => {
      const line = raw.trim();
      if (line.startsWith("```")) fence = !fence;
      const heading = /^(#{1,6})\s+(.+?)\s*#*$/.exec(line);
      if (!fence && heading?.[1] && heading[2]) {
        symbols.push({ name: heading[2], kind: "heading", line: i + 1, depth: heading[1].length - 1 });
      }
    });
    return symbols;
  }
  const language = ext === "py" ? "python" : ["ts", "tsx", "js", "jsx", "mjs", "cjs"].includes(ext) ? "js" : null;
  if (!language) return symbols;
  lines(text).forEach((raw, i) => {
    const indent = raw.length - raw.trimStart().length;
    const line = raw.trim();
    for (const [re, kind] of SYMBOL_RULES[language] ?? []) {
      const found = re.exec(line);
      const name = found?.[1];
      if (!name || ["if", "for", "while", "switch", "catch", "return"].includes(name)) continue;
      if (kind === "method" && indent === 0) continue;
      const depth = Math.min(8, Math.floor(indent / 2));
      symbols.push({ name, kind: kind === "function" && depth > 0 ? "method" : kind, line: i + 1, depth });
      break;
    }
  });
  return symbols;
}
