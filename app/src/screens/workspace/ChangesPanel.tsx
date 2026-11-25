import { useEffect, useState } from "react";
import { useQuery } from "@tanstack/react-query";

import type { Task } from "../../api/types";
import { SidePanel } from "../../code/SidePanel";
import { useApi } from "../../api/context";
import { useTerminals } from "../../components/Terminal/TerminalProvider";
import { Button } from "../../components/ui/Button";
import { Segmented } from "../../components/ui/controls";
import { ExpandIcon, ExternalIcon } from "../../components/ui/icons";
import { useT, type MessageKey } from "../../i18n";
import { Changes } from "../../review/Review";
import { isOpen } from "../../tasks/timeline";

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
      // A closed task's copy is gone: its project's folder then.
      preview={<Preview project={project} dir={(isOpen(task) ? task?.workspace?.worktree : undefined) ?? project} />}
    />
  );
}

const LOCAL_URL = /^https?:\/\/(localhost|127\.0\.0\.1)(:\d+)?(\/.*)?$/;

/** Its `dev` (else `start`) script, run with the package manager its lock file names (npm without one). */
function devCommand(files: string[], manifest: string): string | null {
  let scripts: unknown;
  try {
    scripts = (JSON.parse(manifest) as { scripts?: unknown } | null)?.scripts;
  } catch {
    return null;
  }
  // Any JSON parses: only an object of scripts counts.
  if (!scripts || typeof scripts !== "object" || Array.isArray(scripts)) return null;
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
/** How wide the page shows when the preview is expanded: the space it has, or a device's. */
type PreviewSize = "fit" | "phone" | "tablet" | "desktop";
const SIZE_WIDTH: Record<PreviewSize, string> = { fit: "100%", phone: "390px", tablet: "820px", desktop: "1280px" };
const SIZE_LABEL: Record<PreviewSize, MessageKey> = {
  fit: "workspace.preview.size.fit",
  phone: "workspace.preview.size.phone",
  tablet: "workspace.preview.size.tablet",
  desktop: "workspace.preview.size.desktop",
};

function Preview({ project, dir }: { project: string; dir: string }) {
  const t = useT();
  const api = useApi();
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
  // Expanded over the whole workspace. The same element grows, so the page
  // in it keeps its state (a form half filled, a session).
  const [expanded, setExpanded] = useState(false);
  const [size, setSize] = useState<PreviewSize>("fit");
  const valid = LOCAL_URL.test(url.trim());

  useEffect(() => {
    if (!expanded) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        setExpanded(false);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [expanded]);

  return (
    <div className={`preview${expanded ? " preview--expanded" : ""}`} role={expanded ? "dialog" : undefined} aria-label={expanded ? t("workspace.tab.preview") : undefined}>
      <form
        className="row preview__bar"
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
        {shown && expanded && (
          <Segmented<PreviewSize>
            label={t("workspace.preview.size")}
            value={size}
            options={(Object.keys(SIZE_WIDTH) as PreviewSize[]).map((value) => ({ value, label: t(SIZE_LABEL[value]) }))}
            onChange={setSize}
          />
        )}
        {/* In the narrow panel they are icons, so the address keeps its room. */}
        {shown && (
          <Button
            small
            variant="ghost"
            aria-label={t("workspace.preview.browser")}
            title={t("workspace.preview.browserHint")}
            onClick={() => void api.openLink(shown)}
          >
            {expanded ? t("workspace.preview.browser") : <ExternalIcon />}
          </Button>
        )}
        {shown && (
          <Button
            small
            variant={expanded ? "primary" : "ghost"}
            aria-label={expanded ? t("workspace.preview.collapse") : t("workspace.preview.expand")}
            title={expanded ? undefined : t("workspace.preview.expand")}
            aria-keyshortcuts={expanded ? "Escape" : undefined}
            onClick={() => setExpanded((on) => !on)}
          >
            {expanded ? t("workspace.preview.collapse") : <ExpandIcon />}
            {expanded && (
              <kbd className="kbd" aria-hidden>
                Esc
              </kbd>
            )}
          </Button>
        )}
      </form>
      {!valid && <p className="error-text">{t("workspace.preview.localOnly")}</p>}
      {shown ? (
        <div className="preview__stage">
          <iframe
            key={reloads}
            className="preview__frame"
            style={{ width: expanded ? SIZE_WIDTH[size] : "100%" }}
            src={shown}
            title={t("workspace.tab.preview")}
            sandbox="allow-scripts allow-same-origin allow-forms allow-popups"
          />
        </div>
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
