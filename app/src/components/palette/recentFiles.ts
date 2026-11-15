// Files opened recently, per project (folder and task copies share the list:
// the same paths exist in both). A convenience in localStorage: lost storage
// only means no recent files.

const KEY = "hedy.palette.recentFiles";
const MAX = 20;
const MAX_PROJECTS = 30;

type Store = Record<string, string[]>;

function read(): Store {
  try {
    const value: unknown = JSON.parse(localStorage.getItem(KEY) ?? "{}");
    if (!value || typeof value !== "object" || Array.isArray(value)) return {};
    const out: Store = {};
    for (const [project, paths] of Object.entries(value as Record<string, unknown>)) {
      if (Array.isArray(paths)) out[project] = paths.filter((p): p is string => typeof p === "string");
    }
    return out;
  } catch {
    return {};
  }
}

export function recentFiles(project: string): string[] {
  return read()[project] ?? [];
}

export function rememberFile(project: string, path: string) {
  const store = read();
  const list = [path, ...(store[project] ?? []).filter((p) => p !== path)].slice(0, MAX);
  // The project just used goes last, so the oldest ones are dropped first.
  const entries = Object.entries(store).filter(([p]) => p !== project);
  entries.push([project, list]);
  const next: Store = Object.fromEntries(entries.slice(-MAX_PROJECTS));
  try {
    localStorage.setItem(KEY, JSON.stringify(next));
  } catch {
    // Storage blocked: recents are a convenience.
  }
}
