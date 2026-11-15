import { useMutation, useQueryClient } from "@tanstack/react-query";
import type { MouseEvent } from "react";

import { errorMessage, useApi } from "../../../api/context";
import type { PullOutcome, PushOutcome, ShipFailure } from "../../../api/types";
import { useProjectActions } from "../../../components/projects";
import { MenuButton, type MenuItem } from "../../../components/ui/Menu";
import { useToast } from "../../../components/ui/Toast";
import { useT } from "../../../i18n";
import { BranchIcon, DownIcon, UpIcon } from "./icons";
import { gitKeys, nextStep, useFolderStatus } from "./useShip";
import "../../../styles/ship.css";

/**
 * Push and pull (fast-forward only) of the user's folder, with what to do
 * next when they fail: a rejected push offers "Traer cambios", a divergence
 * offers VS Code. Never forces, never merges.
 */
export function useFolderGit(project: string) {
  const api = useApi();
  const t = useT();
  const toast = useToast();
  const client = useQueryClient();
  const actions = useProjectActions();
  const refresh = () => Promise.all(gitKeys(project).map((queryKey) => client.invalidateQueries({ queryKey })));

  const report = (failure: ShipFailure, pull: () => void) => {
    const step = nextStep(failure);
    if (step === "pull") {
      toast.info(failure.message, { durationMs: 10_000, action: { label: t("ship.folder.pull"), onClick: pull } });
    } else if (step === "editor") {
      toast.info(failure.message, {
        durationMs: 10_000,
        action: { label: t("common.openInVsCode"), onClick: () => actions.openEditor({ path: project }) },
      });
    } else {
      toast.error(failure.message);
    }
  };

  const pull = useMutation<PullOutcome>({
    mutationFn: () => api.pullFolder(project),
    onSettled: refresh,
    onError: (e) => toast.error(errorMessage(e, t)),
    onSuccess: (out) => {
      if (out.failure) return report(out.failure, () => pull.mutate());
      const branch = status.data?.branch ?? "";
      toast.info(out.updated ? t("ship.folder.pulled", { branch }) : t("ship.folder.upToDate", { branch }));
    },
  });
  const push = useMutation<PushOutcome>({
    mutationFn: () => api.pushFolder(project),
    onSettled: refresh,
    onError: (e) => toast.error(errorMessage(e, t)),
    onSuccess: (out) => {
      if (out.failure) return report(out.failure, () => pull.mutate());
      if (!out.pushed) return;
      const { upstream, upstreamSet } = out.pushed;
      toast.info(upstreamSet ? t("ship.folder.pushedNew", { upstream }) : t("ship.folder.pushed", { upstream }));
    },
  });
  const status = useFolderStatus(project);
  return { status, pull, push, busy: pull.isPending || push.isPending };
}

/** Where the folder's status sits: the window's status bar, or (without one) the workspace header. */
export type FolderStatusPlacement = "header" | "bar";

/**
 * The user's folder, quiet: its branch (a menu with Pull, Push, VS Code and
 * copying the name; also on right-click), the uncommitted changes, and ↓ / ↑
 * counters that pull or push when clicked. Always the folder, never a task's
 * copy. It lives in the status bar; there the uncommitted changes are the
 * bar's own "changes" item, which opens the review.
 */
export function FolderStatus({ project, placement = "bar" }: { project: string; placement?: FolderStatusPlacement }) {
  return <FolderGitStatus project={project} placement={placement} />;
}

function FolderGitStatus({ project, placement }: { project: string; placement: FolderStatusPlacement }) {
  const t = useT();
  const toast = useToast();
  const actions = useProjectActions();
  const { status, pull, push, busy } = useFolderGit(project);
  const s = status.data;
  if (!s) return null;

  const branch = s.branch ?? t("ship.folder.detached");
  const upstream = s.upstream ?? "";
  const remote = s.remote ?? "";
  const canPush = !!s.remote && !!s.branch && !busy;
  const unpublished = !!s.remote && !!s.branch && !s.upstream;
  const pushLabel = unpublished
    ? t("ship.folder.publishBranch", { remote, branch })
    : t("ship.folder.ahead", { n: s.ahead, upstream });

  const copy = () => {
    if (!s.branch) return;
    const done = () => toast.info(t("ship.folder.copied", { branch: s.branch ?? "" }));
    void navigator.clipboard?.writeText(s.branch).then(done, (e: unknown) => toast.error(errorMessage(e, t)));
  };

  const items: MenuItem[] = [
    {
      label: t("ship.folder.pull"),
      hint: s.upstream ? t("ship.folder.pullHint", { upstream }) : t("ship.folder.pullNoUpstream"),
      disabled: !s.upstream || busy,
      onSelect: () => pull.mutate(),
    },
    {
      label: t("ship.folder.push"),
      hint: !s.remote
        ? t("ship.folder.noRemote")
        : unpublished
          ? t("ship.folder.pushNew", { remote, branch })
          : t("ship.folder.pushHint", { upstream }),
      disabled: !canPush,
      onSelect: () => push.mutate(),
    },
    { label: t("common.openInVsCode"), onSelect: () => actions.openEditor({ path: project }), separatorBefore: true },
    { label: t("ship.folder.copy"), disabled: !s.branch, onSelect: copy },
  ];

  // Right-click anywhere on it opens the same menu.
  const onContextMenu = (e: MouseEvent<HTMLDivElement>) => {
    e.preventDefault();
    e.currentTarget.querySelector<HTMLButtonElement>(".folder-status__branch")?.click();
  };

  const changes = t("ship.folder.changesDetail", {
    n: s.changes,
    staged: s.staged,
    unstaged: s.unstaged,
    untracked: s.untracked,
  });

  const bar = placement === "bar";
  const button = bar ? "statusbar__item" : "tool-btn";

  return (
    <div
      className={bar ? "folder-status folder-status--bar" : "folder-status"}
      role="group"
      aria-label={t("ship.folder.menuTitle")}
      aria-busy={busy || undefined}
      onContextMenu={onContextMenu}
    >
      <MenuButton
        label={t("ship.folder.menu", { branch })}
        title={t("ship.folder.menuTitle")}
        items={items}
        className={`${button} folder-status__branch`}
      >
        <BranchIcon />
        <span className="folder-status__name">{branch}</span>
      </MenuButton>
      {!bar && s.changes > 0 && (
        <span className="folder-status__changes" title={changes}>
          <span className="folder-status__dot" aria-hidden />
          <span aria-hidden>{s.changes}</span>
          <span className="sr-only">{changes}</span>
        </span>
      )}
      {s.behind > 0 && (
        <button
          type="button"
          className={`${button} folder-status__count`}
          title={t("ship.folder.behind", { n: s.behind, upstream })}
          aria-label={t("ship.folder.behind", { n: s.behind, upstream })}
          disabled={busy}
          onClick={() => pull.mutate()}
        >
          <DownIcon />
          {s.behind}
        </button>
      )}
      {(s.ahead > 0 || unpublished) && (
        <button
          type="button"
          className={`${button} folder-status__count`}
          title={pushLabel}
          aria-label={pushLabel}
          disabled={!canPush}
          onClick={() => push.mutate()}
        >
          <UpIcon />
          {s.ahead > 0 && s.ahead}
        </button>
      )}
      {busy && (
        <span className="sr-only" role="status">
          {t("ship.folder.working")}
        </span>
      )}
    </div>
  );
}
