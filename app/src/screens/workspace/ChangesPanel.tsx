import { useState } from "react";

import type { Task } from "../../api/types";
import { SidePanel } from "../../code/SidePanel";
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
      preview={<Preview project={project} />}
    />
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
