import { useState } from "react";

import type { Task } from "../../api/types";
import { TerminalView } from "../../components/Terminal/TerminalView";
import { Button } from "../../components/ui/Button";
import { useT, type MessageKey } from "../../i18n";
import { Changes } from "../../review/Review";

type Tab = "changes" | "terminal" | "preview";

const TABS: { id: Tab; label: MessageKey }[] = [
  { id: "changes", label: "workspace.tab.changes" },
  { id: "terminal", label: "common.terminal" },
  { id: "preview", label: "workspace.tab.preview" },
];

export function ChangesPanel({ project, task, hidden = false }: { project: string; task: Task | null; hidden?: boolean }) {
  const t = useT();
  const [tab, setTab] = useState<Tab>("changes");
  // Once opened, the terminal stays alive behind the other tabs: unmounting it
  // would kill what runs there (like the dev server of the preview).
  const [terminalOpened, setTerminalOpened] = useState(false);
  const dir = task?.workspace?.worktree ?? project;

  return (
    <aside className="side-panel" aria-label={t("workspace.panel")} hidden={hidden}>
      <div className="side-panel__tabs" role="tablist">
        {TABS.map((item) => (
          <button
            key={item.id}
            role="tab"
            aria-selected={tab === item.id}
            className="side-tab"
            onClick={() => {
              setTab(item.id);
              if (item.id === "terminal") setTerminalOpened(true);
            }}
          >
            {t(item.label)}
          </button>
        ))}
      </div>
      <div className="side-panel__body">
        {tab === "changes" && <Changes project={project} task={task} />}
        {terminalOpened && (
          // One terminal per folder; it opens in the task's worktree.
          <div className="side-terminal" hidden={tab !== "terminal"}>
            <TerminalView key={dir} request={{ kind: "shell", path: dir }} active={tab === "terminal"} onExit={() => {}} />
          </div>
        )}
        {tab === "preview" && <Preview project={project} />}
      </div>
    </aside>
  );
}

const LOCAL_URL = /^https?:\/\/(localhost|127\.0\.0\.1)(:\d+)?(\/.*)?$/;

/** The project's dev server inside Hedy. Local addresses only. */
function Preview({ project }: { project: string }) {
  const t = useT();
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
        <p className="empty">{t("workspace.preview.empty")}</p>
      )}
    </div>
  );
}
