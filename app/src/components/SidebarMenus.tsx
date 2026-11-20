// Right-click (or Shift+F10, or the menu key) on a project or a chat in the
// sidebar: the quick actions on it, removing included, always confirmed.

import { useEffect, useRef, useState, type KeyboardEvent, type MouseEvent } from "react";
import { useQueryClient } from "@tanstack/react-query";

import { errorMessage, OVERVIEW_KEY, useApi } from "../api/context";
import type { Project, Task } from "../api/types";
import { useT } from "../i18n";
import { CONVERSATIONS_KEY, isBusy } from "../tasks/hooks";
import { viewProject, type View } from "./Layout";
import { useTerminals } from "./Terminal/TerminalProvider";
import { Button } from "./ui/Button";
import { ContextMenu, isMenuKey, pointFor, type ContextMenuItem, type MenuPoint } from "./ui/ContextMenu";
import { Dialog } from "./ui/Dialog";
import { useToast } from "./ui/Toast";

type Target = { kind: "project"; project: Project } | { kind: "chat"; task: Task };

/** The open menu and the removal waiting for its confirmation. */
export function useSidebarMenus({
  view,
  onNavigate,
}: {
  view: View;
  onNavigate: (view: View) => void;
}) {
  const t = useT();
  const api = useApi();
  const toast = useToast();
  const client = useQueryClient();
  const terminals = useTerminals();
  const [menu, setMenu] = useState<{ at: MenuPoint; target: Target } | null>(null);
  const [removing, setRemoving] = useState<Target | null>(null);
  const [busy, setBusy] = useState(false);
  // Where to go once the confirmation is closed (navigating waits for open dialogs).
  const then = useRef<View | null>(null);
  const setThen = (next: View) => {
    then.current = next;
  };
  useEffect(() => {
    if (removing || !then.current) return;
    const next = then.current;
    then.current = null;
    onNavigate(next);
  }, [removing, onNavigate]);

  /** Props for a row's button: right click and the keyboard's menu key open its menu. */
  const trigger = (target: Target) => ({
    onContextMenu: (e: MouseEvent<HTMLElement>) => {
      e.preventDefault();
      // From the keyboard (the menu key) there is no pointer: open by the row.
      const keyboard = e.clientX === 0 && e.clientY === 0;
      setMenu({ at: keyboard ? pointFor(e.currentTarget) : { x: e.clientX, y: e.clientY }, target });
    },
    onKeyDown: (e: KeyboardEvent<HTMLElement>) => {
      if (!isMenuKey(e)) return;
      e.preventDefault();
      setMenu({ at: pointFor(e.currentTarget), target });
    },
  });

  const copy = (text: string, done: string) =>
    void navigator.clipboard?.writeText(text).then(
      () => toast.info(done),
      () => toast.info(t("palette.copyFailed")),
    );

  const items = (target: Target): ContextMenuItem[] => {
    if (target.kind === "project") {
      const { project } = target;
      return [
        { label: t("shell.menu.newTask"), onSelect: () => onNavigate({ screen: "workspace", project: project.path, task: null }) },
        {
          label: t("shell.menu.terminal"),
          onSelect: () => terminals.open({ kind: "shell", path: project.path }, `${t("common.terminal")} · ${project.name}`),
        },
        { label: t("common.openInVsCode"), onSelect: () => void api.openInEditor(project.path) },
        { label: t("shell.menu.memory"), separatorBefore: true, onSelect: () => onNavigate({ screen: "memory", project: project.path }) },
        { label: t("shell.menu.summary"), onSelect: () => onNavigate({ screen: "status", project: project.path }) },
        { label: t("shell.menu.settings"), onSelect: () => onNavigate({ screen: "projectSettings", project: project.path }) },
        { label: t("shell.menu.copyPath"), onSelect: () => copy(project.path, t("shell.menu.pathCopied")) },
        { label: t("shell.menu.removeProject"), danger: true, separatorBefore: true, onSelect: () => setRemoving(target) },
      ];
    }
    const { task } = target;
    const answering = isBusy(task);
    return [
      { label: t("shell.menu.open"), onSelect: () => onNavigate({ screen: "conversation", task: task.id }) },
      { label: t("shell.menu.askAgain"), hint: t("shell.menu.askAgainHint"), onSelect: () => onNavigate({ screen: "conversation", draft: task.prompt }) },
      { label: t("shell.menu.copyQuestion"), onSelect: () => copy(task.prompt, t("shell.menu.questionCopied")) },
      {
        label: t("shell.menu.removeChat"),
        danger: true,
        separatorBefore: true,
        disabled: answering,
        hint: answering ? t("shell.menu.removeChatBusy") : undefined,
        onSelect: () => setRemoving(target),
      },
    ];
  };

  const remove = async (target: Target) => {
    setBusy(true);
    try {
      if (target.kind === "project") {
        // Read before the overview refreshes: then the screen on show has already fallen back.
        const onIt = viewProject(view) === target.project.path;
        await api.removeProject(target.project.path);
        await client.invalidateQueries({ queryKey: OVERVIEW_KEY });
        // Its screens are gone with it.
        if (onIt) setThen({ screen: "welcome" });
        toast.info(t("shell.menu.projectRemoved", { name: target.project.name }));
      } else {
        await api.deleteConversation(target.task.id);
        await client.invalidateQueries({ queryKey: CONVERSATIONS_KEY });
        if (view.screen === "conversation" && view.task === target.task.id) setThen({ screen: "conversation" });
        toast.info(t("shell.menu.chatRemoved"));
      }
      setRemoving(null);
    } catch (e) {
      toast.error(errorMessage(e, t));
    } finally {
      setBusy(false);
    }
  };

  const element = (
    <>
      {menu && (
        <ContextMenu
          at={menu.at}
          label={
            menu.target.kind === "project"
              ? t("shell.menu.projectLabel", { name: menu.target.project.name })
              : t("shell.menu.chatLabel", { title: menu.target.task.title })
          }
          items={items(menu.target)}
          onClose={() => setMenu(null)}
        />
      )}
      {removing && (
        <Dialog
          title={
            removing.kind === "project"
              ? t("shell.menu.removeProjectTitle", { name: removing.project.name })
              : t("conversation.deleteTitle")
          }
          onClose={() => !busy && setRemoving(null)}
          dismissible={!busy}
          footer={
            <>
              <Button variant="ghost" disabled={busy} onClick={() => setRemoving(null)}>
                {t("common.cancel")}
              </Button>
              <Button variant="danger" disabled={busy} onClick={() => void remove(removing)}>
                {removing.kind === "project" ? t("shell.menu.removeProjectConfirm") : t("conversation.delete")}
              </Button>
            </>
          }
        >
          <p>
            {removing.kind === "project"
              ? t("shell.menu.removeProjectText")
              : t("shell.menu.removeChatText", { title: removing.task.title })}
          </p>
        </Dialog>
      )}
    </>
  );

  return { trigger, element };
}
