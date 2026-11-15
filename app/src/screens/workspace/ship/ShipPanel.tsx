import { useState } from "react";
import { createPortal } from "react-dom";
import { useQuery } from "@tanstack/react-query";

import { useAction, useApi } from "../../../api/context";
import type { Task } from "../../../api/types";
import { Button } from "../../../components/ui/Button";
import { useToast } from "../../../components/ui/Toast";
import { useT } from "../../../i18n";
import { ACTIVITY_KEY, tasksKey } from "../../../tasks/hooks";
import { firstStep, ShipDialog, type ShipStep } from "./ShipDialog";
import { ExternalIcon, PullRequestIcon, UpIcon } from "./icons";
import { gitKeys, shipKey, takeJustAccepted } from "./useShip";
import "../../../styles/ship.css";

/**
 * An accepted task on its way to the remote: where the work is, whether it
 * is published, its pull request, and the next step as a button. Right
 * after accepting it opens the publish flow by itself.
 */
export function ShipPanel({ task }: { task: Task }) {
  const api = useApi();
  const t = useT();
  const toast = useToast();
  const state = useQuery({
    queryKey: shipKey(task.id),
    queryFn: () => api.shipState(task.id),
    enabled: !!task.ship,
    retry: false,
  });
  const [open, setOpen] = useState<ShipStep | "auto" | null>(() => (takeJustAccepted(task.id) ? "auto" : null));
  const reopen = useAction((a, id: string) => a.undoAccept(id), {
    invalidate: [tasksKey(task.project), ACTIVITY_KEY, ["diff", task.id], ...gitKeys(task.project)],
    onSuccess: () => toast.info(t("ship.reopened")),
  });

  const s = state.data;
  // Right after accepting, the flow opens on the first step left (if any) and
  // stays open from then on, whatever the state says as it moves on.
  if (open === "auto" && s) {
    const step = firstStep(s);
    setOpen(step === "done" ? null : step);
  }

  if (!task.ship || !s) return null;
  const pr = s.pullRequest;

  return (
    <div className="ship-panel">
      {s.remote ? (
        <div className="ship-panel__row">
          <span className={s.pushed ? "ship-panel__ok" : "muted"}>
            {s.pushed && s.upstream ? t("ship.panel.published", { upstream: s.upstream }) : t("ship.panel.unpublished")}
          </span>
          {!s.pushed && (
            <Button small variant="primary" onClick={() => setOpen("publish")}>
              <UpIcon /> {t("ship.panel.publish")}
            </Button>
          )}
        </div>
      ) : (
        <p className="muted small">{t("ship.panel.noRemote")}</p>
      )}
      {pr ? (
        <div className="ship-panel__row">
          <button type="button" className="ship-panel__link" onClick={() => void api.openLink(pr.url)} title={pr.url}>
            <PullRequestIcon />
            {pr.number !== null
              ? pr.draft
                ? t("ship.panel.prDraft", { n: pr.number })
                : t("ship.panel.pr", { n: pr.number })
              : t("ship.done.openAny")}
            <ExternalIcon />
          </button>
        </div>
      ) : s.canOpenPr ? (
        <div className="ship-panel__row">
          <Button small variant={s.pushed ? "primary" : "secondary"} disabled={!s.pushed} onClick={() => setOpen("pr")}>
            <PullRequestIcon /> {t("ship.panel.createPr")}
          </Button>
        </div>
      ) : s.remote && s.gh === "missing" ? (
        <p className="muted small">{t("ship.panel.ghMissing")}</p>
      ) : s.remote && s.gh === "loggedOut" ? (
        <p className="muted small">{t("ship.panel.ghLoggedOut")}</p>
      ) : null}
      {s.kept && (
        <div className="undo-merge">
          <Button small disabled={reopen.isPending} onClick={() => reopen.mutate(task.id)}>
            {t("ship.reopen")}
          </Button>
          <p className="muted small">{t("ship.reopenHint", { branch: s.branch })}</p>
        </div>
      )}
      {/* Outside the side panel: it may be hidden while the flow runs. */}
      {open !== null &&
        open !== "auto" &&
        createPortal(<ShipDialog task={task} initial={open} onClose={() => setOpen(null)} />, document.body)}
    </div>
  );
}
