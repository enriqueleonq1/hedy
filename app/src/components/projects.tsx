import { useApi } from "../api/context";
import type { Account, Project } from "../api/types";
import { useToast } from "./ui/Toast";
import { useTerminals } from "./Terminal/TerminalProvider";
import { errorMessage } from "../api/context";
import { useT } from "../i18n";

/** Project color, falling back to its account's color. */
export function projectColor(project: Project, accounts: Account[]): string {
  return project.color ?? accounts.find((a) => a.id === project.account)?.color ?? "var(--muted)";
}

export function shortPath(path: string): string {
  return path.replace(/^\/home\/[^/]+/, "~");
}

/** Actions available on any project, wherever it is shown. */
export function useProjectActions() {
  const api = useApi();
  const toast = useToast();
  const t = useT();
  const terminals = useTerminals();
  return {
    openCli: (p: Project) => terminals.open({ kind: "run", path: p.path }, p.name),
    openShell: (p: Project) => terminals.open({ kind: "shell", path: p.path }, `${p.name} · shell`),
    /** A project or any folder of it, like a task's worktree. */
    openEditor: (p: Pick<Project, "path">) =>
      api.openInEditor(p.path).catch((e: unknown) => toast.error(errorMessage(e, t))),
  };
}
