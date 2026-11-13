import { useApi } from "../api/context";
import type { Account, Project } from "../api/types";
import { useToast } from "./ui/Toast";
import { useTerminals } from "./Terminal/TerminalProvider";
import { errorMessage } from "../api/context";

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
  const terminals = useTerminals();
  return {
    openCli: (p: Project) => terminals.open({ kind: "run", path: p.path }, p.name),
    openShell: (p: Project) => terminals.open({ kind: "shell", path: p.path }, `${p.name} · shell`),
    openEditor: (p: Project) =>
      api.openInEditor(p.path).catch((e: unknown) => toast.error(errorMessage(e))),
  };
}
