import { useQuery } from "@tanstack/react-query";
import type { AgentEvent, Task } from "../../api/types";
import { useAction, useApi } from "../../api/context";
import { Button } from "../../components/ui/Button";
import { useToast } from "../../components/ui/Toast";
import { formatBytes } from "../../format";
import { useLanguage, useT } from "../../i18n";

/**
 * What a question wrote in its own folder, outside the project (a document,
 * a page), once its turn is over: open it, or save a copy somewhere.
 * Nothing when it wrote nothing.
 */
export function OutputsCard({ task, events }: { task: Task; events: AgentEvent[] }) {
  const t = useT();
  const api = useApi();
  const language = useLanguage();
  const toast = useToast();
  const idle = task.mode !== "project" && !task.live && task.status !== "running" && task.status !== "discarded";
  // Read again after each turn, or a stop: a new file comes with its end.
  const turns = events.filter((e) => e.kind === "finished").length;
  const files = useQuery({
    queryKey: ["taskOutputs", task.id, turns, task.status, Boolean(task.live)],
    queryFn: () => api.taskOutputs(task.id),
    enabled: idle,
  });
  const open = useAction((api, path: string) => api.openOutput(task.id, path), { invalidate: [] });
  const save = useAction(
    async (api, path: string) => {
      const folder = await api.pickFolder();
      return folder === null ? null : api.saveOutput(task.id, path, folder);
    },
    {
      invalidate: [],
      onSuccess: (saved) => {
        if (saved) toast.info(t("outputs.saved", { path: saved }));
      },
    },
  );
  if (!idle || !files.data?.length) return null;
  return (
    <section className="end-card" aria-label={t("outputs.title")}>
      <h3>{t("outputs.title")}</h3>
      <p className="muted">{t("outputs.hint")}</p>
      <ul className="outputs">
        {files.data.map((file) => (
          <li key={file.path} className="outputs__row">
            <span className="outputs__name mono">{file.path}</span>
            <span className="outputs__size">{formatBytes(file.bytes, language)}</span>
            {file.opens && (
              <Button
                small
                variant="ghost"
                aria-label={t("outputs.openNamed", { path: file.path })}
                disabled={open.isPending}
                onClick={() => open.mutate(file.path)}
              >
                {t("outputs.open")}
              </Button>
            )}
            <Button
              small
              variant="ghost"
              aria-label={t("outputs.saveNamed", { path: file.path })}
              disabled={save.isPending}
              onClick={() => save.mutate(file.path)}
            >
              {t("outputs.save")}
            </Button>
          </li>
        ))}
      </ul>
    </section>
  );
}
