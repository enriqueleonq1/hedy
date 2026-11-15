import { useQuery } from "@tanstack/react-query";

import { errorMessage, useApi } from "../api/context";
import type { CodeCopy } from "../api/codeTypes";
import { Dialog } from "../components/ui/Dialog";
import { codeKeys, useFileNavigator } from "../files/navigator";
import { timeAgo } from "../format";
import { useT } from "../i18n";
import { STATUS_LABEL } from "../tasks/timeline";

/** A file's timeline from the review: its commits (open one to read the file then) and the tasks that touched it. */
export function HistoryDialog({ copy, path, onClose }: { copy: CodeCopy; path: string; onClose: () => void }) {
  const t = useT();
  const api = useApi();
  const nav = useFileNavigator();
  const history = useQuery({ queryKey: codeKeys.history(copy, path), queryFn: () => api.fileHistory(copy, path) });
  const data = history.data;
  const entries = data
    ? [
        ...data.commits.map((c) => ({ kind: "commit" as const, at: c.date, commit: c })),
        ...data.tasks.filter((task) => task.open).map((task) => ({ kind: "task" as const, at: task.at, task })),
      ].sort((a, b) => b.at - a.at)
    : [];

  return (
    <Dialog title={t("reviewer.history.title", { path })} onClose={onClose}>
      {history.isPending && <p className="muted">{t("reviewer.history.loading")}</p>}
      {history.isError && <p className="error-text">{errorMessage(history.error, t)}</p>}
      {data && entries.length === 0 && <p className="muted">{t("reviewer.history.empty")}</p>}
      {entries.length > 0 && (
        <ul className="rv-history">
          {entries.map((e) =>
            e.kind === "commit" ? (
              <li key={e.commit.hash}>
                <button
                  type="button"
                  className="rv-history__item"
                  title={t("reviewer.history.openAt", { rev: e.commit.short })}
                  onClick={() => {
                    nav.open({ copy, path: e.commit.path, rev: e.commit.hash, view: "file" });
                    onClose();
                  }}
                >
                  <span className="rv-history__subject">{e.commit.subject}</span>
                  <span className="rv-history__meta">
                    <span className="mono">{e.commit.short}</span> · {e.commit.author} · {timeAgo(e.commit.date, t)}
                  </span>
                </button>
              </li>
            ) : (
              <li key={e.task.task} className="rv-history__item rv-history__item--task">
                <span className="rv-history__subject">{e.task.title}</span>
                <span className="rv-history__meta">
                  {t(STATUS_LABEL[e.task.status])} · {timeAgo(e.task.at, t)}
                </span>
              </li>
            ),
          )}
        </ul>
      )}
    </Dialog>
  );
}
