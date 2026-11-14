import { useState } from "react";
import { useQuery } from "@tanstack/react-query";

import { errorMessage, useAction, useApi } from "../../api/context";
import type { Task } from "../../api/types";
import { useProjectActions } from "../../components/projects";
import { TerminalView } from "../../components/Terminal/TerminalView";
import { Button, ConfirmButton } from "../../components/ui/Button";
import { useToast } from "../../components/ui/Toast";
import { useTaskDiff } from "../../tasks/hooks";
import { isOpen } from "../../tasks/timeline";
import { DiffView } from "./DiffView";

type Tab = "changes" | "terminal" | "preview";

const TABS: { id: Tab; label: string }[] = [
  { id: "changes", label: "Cambios" },
  { id: "terminal", label: "Terminal" },
  { id: "preview", label: "Vista previa" },
];

export function ChangesPanel({ project, task }: { project: string; task: Task | null }) {
  const [tab, setTab] = useState<Tab>("changes");
  // Once opened, the terminal stays alive behind the other tabs: unmounting it
  // would kill what runs there (like the dev server of the preview).
  const [terminalOpened, setTerminalOpened] = useState(false);
  const dir = task?.workspace?.worktree ?? project;

  return (
    <aside className="side-panel" aria-label="Cambios, terminal y vista previa">
      <div className="side-panel__tabs" role="tablist">
        {TABS.map((t) => (
          <button
            key={t.id}
            role="tab"
            aria-selected={tab === t.id}
            className="side-tab"
            onClick={() => {
              setTab(t.id);
              if (t.id === "terminal") setTerminalOpened(true);
            }}
          >
            {t.label}
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
        <input aria-label="Dirección del servidor local" className="mono preview__url" value={url} onChange={(e) => setUrl(e.target.value)} />
        <Button type="submit" variant="primary" small disabled={!valid}>
          {shown ? "Recargar" : "Abrir"}
        </Button>
      </form>
      {!valid && <p className="error-text">Solo direcciones locales (localhost o 127.0.0.1).</p>}
      {shown ? (
        <iframe
          key={reloads}
          className="preview__frame"
          src={shown}
          title="Vista previa"
          sandbox="allow-scripts allow-same-origin allow-forms allow-popups"
        />
      ) : (
        <p className="empty">Arranca el servidor de desarrollo en la pestaña Terminal (por ejemplo, npm run dev) y ábrelo aquí.</p>
      )}
    </div>
  );
}

function Changes({ project, task }: { project: string; task: Task | null }) {
  const api = useApi();
  const toast = useToast();
  const actions = useProjectActions();
  const editable = !!task?.workspace && isOpen(task);
  const diff = useTaskDiff(task?.id ?? null, editable);
  const [selected, setSelected] = useState<string | null>(null);
  const commits = useQuery({ queryKey: ["commits", project], queryFn: () => api.recentCommits(project) });

  // Both change the project's branch: its commits and uncommitted files.
  const changed = [
    ["commits", project],
    ["status", project],
  ];
  const accept = useAction((a, id: string) => a.acceptTask(id), {
    invalidate: changed,
    onSuccess: () => toast.info(`Cambios unidos a ${task?.workspace?.baseBranch ?? "la rama base"}.`),
  });
  const discard = useAction((a, id: string) => a.discardTask(id), { invalidate: changed });

  if (!task) return <p className="empty">Cuando pidas algo en modo Proyecto, aquí verás los cambios.</p>;
  if (!task.workspace) return <p className="empty">Las preguntas y la investigación no cambian archivos.</p>;
  if (!isOpen(task)) {
    return (
      <p className="empty">
        {task.status === "accepted" ? "Esta tarea ya se aceptó y se unió a la rama base." : "Esta tarea se descartó."}
      </p>
    );
  }

  const files = diff.data ?? [];
  const current = files.find((f) => f.path === selected) ?? files[0];

  return (
    <div className="changes">
      {diff.isError && <p className="error-text">{errorMessage(diff.error)}</p>}
      {files.length === 0 ? (
        <p className="empty">{diff.isPending ? "Cargando cambios…" : "Todavía no hay cambios."}</p>
      ) : (
        <>
          <ul className="file-list" aria-label="Archivos cambiados">
            {files.map((f) => (
              <li key={f.path}>
                <button
                  className="file-item"
                  aria-current={f === current ? "true" : undefined}
                  onClick={() => setSelected(f.path)}
                >
                  <span className="file-item__path">{f.path}</span>
                  <span className="file-item__stats">
                    <span className="plus">+{f.additions}</span> <span className="minus">-{f.deletions}</span>
                  </span>
                </button>
              </li>
            ))}
          </ul>
          {current && <DiffView file={current} />}
        </>
      )}

      <div className="actions">
        <Button
          variant="primary"
          disabled={files.length === 0 || accept.isPending || task.status === "running"}
          title={task.status === "running" ? "Espera a que el agente termine" : undefined}
          onClick={() => accept.mutate(task.id)}
        >
          Aceptar cambios
        </Button>
        <Button onClick={() => actions.openEditor({ path: task.workspace?.worktree ?? project })}>Abrir en VS Code</Button>
        <ConfirmButton confirmLabel="¿Descartar todo?" onConfirm={() => discard.mutate(task.id)}>
          Descartar
        </ConfirmButton>
      </div>
      <p className="muted small">
        Rama <span className="mono">{task.workspace.branch}</span> desde{" "}
        <span className="mono">{task.workspace.baseBranch}</span>
      </p>
      {commits.data && commits.data.length > 0 && (
        <p className="muted small">
          Últimos commits · {commits.data.map((c) => `${c.hash} ${c.subject}`).join(" · ")}
        </p>
      )}
    </div>
  );
}
