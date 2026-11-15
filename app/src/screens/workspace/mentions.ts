// `@file` mentions in a project's composer: the word with the caret, the
// files that match it and the text once one is picked.

/** The `@` word the caret is in: where the `@` is and what follows it, up to the caret. */
export function mentionAt(text: string, caret: number): { start: number; query: string } | null {
  let start = Math.min(caret, text.length);
  while (start > 0 && !/\s/.test(text[start - 1] ?? "")) start--;
  if (text[start] !== "@") return null;
  const query = text.slice(start + 1, caret);
  // `@@`, or an address such as `ana@x.com` before the caret, is not a mention.
  if (query.includes("@")) return null;
  return { start, query };
}

const basename = (path: string) => path.slice(path.lastIndexOf("/") + 1);

/**
 * Files matching `query` (no case): file names that start with it first,
 * then names that contain it, then paths that contain it; shorter paths
 * first within each group.
 */
export function rankFiles(files: string[], query: string, limit = 8): string[] {
  const q = query.toLowerCase();
  if (!q) return files.slice(0, limit);
  const scored: { path: string; score: number }[] = [];
  for (const path of files) {
    const lower = path.toLowerCase();
    const name = basename(lower);
    const score = name.startsWith(q) ? 0 : name.includes(q) ? 1 : lower.includes(q) ? 2 : -1;
    if (score >= 0) scored.push({ path, score });
  }
  return scored
    .sort((a, b) => a.score - b.score || a.path.length - b.path.length || a.path.localeCompare(b.path))
    .slice(0, limit)
    .map((s) => s.path);
}

/** `text` with the mention from `start` to `caret` replaced by `@path `, and where the caret goes. */
export function insertMention(text: string, start: number, caret: number, path: string): { text: string; caret: number } {
  const rest = text.slice(caret);
  const inserted = `@${path}${/^\s/.test(rest) ? "" : " "}`;
  const next = text.slice(0, start) + inserted + rest;
  return { text: next, caret: start + inserted.length + (/^\s/.test(rest) ? 1 : 0) };
}

/** A path split for display: the file name, and the folder it is in. */
export function splitPath(path: string): { name: string; dir: string } {
  const cut = path.lastIndexOf("/");
  return cut < 0 ? { name: path, dir: "" } : { name: path.slice(cut + 1), dir: path.slice(0, cut + 1) };
}
