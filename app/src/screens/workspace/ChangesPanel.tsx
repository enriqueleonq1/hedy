import { useState } from "react";
import { useQuery } from "@tanstack/react-query";

import type { Task } from "../../api/types";
import { SidePanel } from "../../code/SidePanel";
import { useApi } from "../../api/context";
import { useTerminals } from "../../components/Terminal/TerminalProvider";
import { Button } from "../../components/ui/Button";
import { useT } from "../../i18n";
import { Changes } from "../../review/Review";

/**
 * The side panel of a project's workspace: the code views, the review of the
 * task's changes (`Changes`), a terminal and the preview.
 */
export function ChangesPanel({
  project,
  task,
  hidden = false,
  onRequestOpen,
}: {
  project: string;
  task: Task | null;
  hidden?: boolean;
  /** A file was opened (or a shortcut asked for a view) while the panel was hidden. */
  onRequestOpen?: () => void;
}) {
  return (
    <SidePanel
      project={project}
      task={task}
      hidden={hidden}
      onRequestOpen={onRequestOpen}
      review={<Changes project={project} task={task} />}
      preview={<Preview project={project} dir={task?.workspace?.worktree ?? project} />}
    />
  );
}

const LOCAL_URL = /^https?:\/\/(localhost|127\.0\.0\.1)(:\d+)?(\/.*)?$/;

/** Its `dev` (else `start`) script, run with the package manager its lock file names (npm without one). */
function devCommand(files: string[], manifest: string): string | null {
  let scripts: Record<string, unknown>;
  try {
    scripts = (JSON.parse(manifest) as { scripts?: Record<string, unknown> }).scripts ?? {};
  } catch {
    return null;
  }
  const script = "dev" in scripts ? "dev" : "start" in scripts ? "start" : null;
  if (!script) return null;
  const has = (name: string) => files.includes(name);
  if (has("pnpm-lock.yaml")) return `pnpm ${script}`;
  if (has("yarn.lock")) return `yarn ${script}`;
  if (has("bun.lockb") || has("bun.lock")) return `bun run ${script}`;
  return `npm run ${script}`;
}

/** The project's dev command, from its `package.json` (none for other kinds of project). */
function useDevCommand(project: string): string | null {
  const api = useApi();
  const files = useQuery({ queryKey: ["projectFiles", project], queryFn: () => api.projectFiles(project), staleTime: 30_000 });
  const hasManifest = !!files.data?.includes("package.json");
  const manifest = useQuery({
    queryKey: ["devManifest", project],
    queryFn: () => api.readFile({ kind: "folder", project }, "package.json"),
    enabled: hasManifest,
  });
  const content = manifest.data;
  return files.data && content?.kind === "text" ? devCommand(files.data, content.text) : null;
}

/** The project's dev server inside Hedy. Local addresses only. */
function Preview({ project, dir }: { project: string; dir: string }) {
  const t = useT();
  const terminals = useTerminals();
  const dev = useDevCommand(project);
  const storageKey = `hedy.preview.${project}`;
  const [url, setUrl] = useState(() => {
    try {
      return localStorage.getItem(storageKey) ?? "http://localhost:3000";
    } catch {
      return "http://localhost:3000";
    }
  });
  const [shown, setShown] = useState<string | null>(null);
  const [reloads, setReloads] = useState(0);
  const valid = LOCAL_URL.test(url.trim());

  return (
    <div className="preview">
      <form
        className="row"
        onSubmit={(e) => {
          e.preventDefault();
          if (!valid) return;
          setShown(url.trim());
          setReloads((n) => n + 1);
          try {
            localStorage.setItem(storageKey, url.trim());
          } catch {
            // Remembering the address is only a convenience.
          }
        }}
      >
        <input aria-label={t("workspace.preview.address")} className="mono preview__url" value={url} onChange={(e) => setUrl(e.target.value)} />
        <Button type="submit" variant="primary" small disabled={!valid}>
          {shown ? t("workspace.preview.reload") : t("workspace.preview.open")}
        </Button>
      </form>
      {!valid && <p className="error-text">{t("workspace.preview.localOnly")}</p>}
      {shown ? (
        <iframe
          key={reloads}
          className="preview__frame"
          src={shown}
          title={t("workspace.tab.preview")}
          sandbox="allow-scripts allow-same-origin allow-forms allow-popups"
        />
      ) : (
        <div className="empty">
          <p>{t("workspace.preview.empty")}</p>
          {dev && (
            <Button
              small
              onClick={() => terminals.open({ kind: "shell", path: dir }, `${t("common.terminal")} · ${dev}`, dev)}
            >
              {t("workspace.preview.start", { command: dev })}
            </Button>
          )}
        </div>
      )}
    </div>
  );
}
