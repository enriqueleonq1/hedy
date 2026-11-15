// The palette's code commands: open a file, go to a line or a symbol, search
// text, open the side panel's views, and act on the active file (the one
// `useFileNavigator().current` names). Read-only, like code navigation.

import { useQueryClient } from "@tanstack/react-query";
import { useCallback } from "react";

import type { CodeTree } from "../../api/codeTypes";
import { useApi } from "../../api/context";
import { codeKeys, sameCopy, useFileNavigator, type NavigatorTarget } from "../../files/navigator";
import type { Translate } from "../../i18n";
import type { View } from "../Layout";
import { useFolderGit } from "../../screens/workspace/ship/FolderStatus";
import { useToast } from "../ui/Toast";
import type { ActiveCopy } from "./copy";
import type { Command } from "./commands";
import { modKey, shiftKey } from "./keys";
import type { Mode } from "./modes";

/**
 * Shows a file: on a screen without code (a project's memory or status) it goes
 * to the project's workspace first, so the explorer can show it.
 */
export function useOpenFile(active: ActiveCopy | null, onNavigate: (view: View) => void) {
  const nav = useFileNavigator();
  return useCallback(
    (target: NavigatorTarget) => {
      if (active && !active.onScreen) onNavigate({ screen: "workspace", project: active.project, task: null });
      nav.open(target);
    },
    [active, nav, onNavigate],
  );
}

/**
 * The side panel's views answer to VS Code's shortcuts (Ctrl/⌘ ⇧ E, F, G): the
 * palette and the status bar press them, so they need no handle on the panel.
 * `false` when nothing on screen took it.
 */
export function pressPanelShortcut(key: "E" | "F" | "G"): boolean {
  const mac = modKey() === "⌘";
  const event = new KeyboardEvent("keydown", {
    key,
    code: `Key${key}`,
    ctrlKey: !mac,
    metaKey: mac,
    shiftKey: true,
    bubbles: true,
    cancelable: true,
  });
  return !document.body.dispatchEvent(event);
}

function basename(path: string): string {
  return path.slice(path.lastIndexOf("/") + 1);
}

export function useCodeCommands({
  active,
  t,
  onNavigate,
  reopen,
}: {
  active: ActiveCopy | null;
  t: Translate;
  onNavigate: (view: View) => void;
  /** Opens the palette again on this text (after a command closed it). */
  reopen: (text: string) => void;
}): Command[] {
  const api = useApi();
  const nav = useFileNavigator();
  const client = useQueryClient();
  const toast = useToast();
  const openFile = useOpenFile(active, onNavigate);
  // The project's folder: pull and push, as its status in the workspace offers them.
  const folderGit = useFolderGit(active?.project ?? "");
  if (!active) return [];

  const { copy } = active;
  const mod = modKey();
  const shift = shiftKey();
  const current = nav.current;
  const commands: Command[] = [];
  const modeCommand = (id: string, label: string, switchTo: Mode, keys: string[]): Command => ({
    id,
    group: "code",
    label,
    keys,
    switchTo,
  });
  const cachedTree = () => client.getQueryData<CodeTree>(codeKeys.tree(copy));

  commands.push(modeCommand("code:quickOpen", t("palette.code.quickOpen"), "files", [mod, "P"]));
  if (current) {
    commands.push(
      modeCommand("code:line", t("palette.code.line"), "line", [":"]),
      modeCommand("code:symbol", t("palette.code.symbol"), "symbols", ["@"]),
    );
  }
  commands.push(modeCommand("code:searchText", t("palette.code.searchText"), "text", ["#"]));

  // The side panel's views, where it is on screen.
  if (active.onScreen) {
    commands.push(
      {
        id: "code:explorer",
        group: "code",
        label: t("palette.code.explorer"),
        keys: [mod, shift, "E"],
        run: () =>
          setTimeout(() => {
            if (pressPanelShortcut("E")) return;
            const path = current && sameCopy(current.copy, copy) ? current.path : cachedTree()?.entries[0]?.path;
            if (path) nav.open({ copy, path, view: "file" });
          }),
      },
      {
        id: "code:search",
        group: "code",
        label: t("palette.code.search"),
        keys: [mod, shift, "F"],
        run: () => setTimeout(() => pressPanelShortcut("F") || reopen("#")),
      },
      {
        id: "code:review",
        group: "code",
        label: t("palette.code.review"),
        keys: [mod, shift, "G"],
        run: () =>
          setTimeout(() => {
            if (pressPanelShortcut("G")) return;
            const changed = cachedTree()?.entries.find((e) => e.status !== null && !e.locked)?.path;
            const path = changed ?? (current && sameCopy(current.copy, copy) ? current.path : undefined);
            if (path) nav.open({ copy, path, view: "diff" });
          }),
      },
    );
  }

  if (current) {
    const name = basename(current.path);
    const file = { group: "code" as const, detail: name };
    commands.push(
      {
        ...file,
        id: "code:diff",
        label: t("palette.code.diff"),
        run: () => openFile({ copy: current.copy, path: current.path, view: "diff" }),
      },
      ...(current.copy.kind === "task"
        ? [
            {
              ...file,
              id: "code:before",
              label: t("palette.code.before"),
              run: () => openFile({ copy: current.copy, path: current.path, rev: "base", view: "file" }),
            },
          ]
        : []),
      {
        ...file,
        id: "code:vscode",
        label: t("palette.code.vscode"),
        run: () =>
          void api
            .openFileInEditor(current.copy, current.path, current.line)
            .catch(() => toast.info(t("palette.code.vscodeFailed"))),
      },
      {
        ...file,
        id: "code:copyPath",
        label: t("palette.code.copyPath"),
        run: () => void copyText(current.path).then((ok) => toast.info(t(ok ? "palette.copied" : "palette.copyFailed"))),
      },
      {
        ...file,
        id: "code:copyAbsolute",
        label: t("palette.code.copyAbsolute"),
        run: () =>
          void client
            .fetchQuery({ queryKey: codeKeys.info(current.copy), queryFn: () => api.codeCopy(current.copy) })
            .then((info) => copyText(`${info.root.replace(/\/$/, "")}/${current.path}`))
            .catch(() => false)
            .then((ok) => toast.info(t(ok ? "palette.copied" : "palette.copyFailed"))),
      },
      {
        ...file,
        id: "code:reveal",
        label: t("palette.code.reveal"),
        run: () => void api.revealInFileManager(current.copy, current.path).catch(() => {}),
      },
    );
  }
  const branch = folderGit.status.data?.branch ?? undefined;
  commands.push(
    {
      id: "folder:pull",
      group: "code",
      label: t("palette.folder.pull"),
      detail: branch,
      run: () => folderGit.pull.mutate(),
    },
    {
      id: "folder:push",
      group: "code",
      label: t("palette.folder.push"),
      detail: branch,
      run: () => folderGit.push.mutate(),
    },
  );
  return commands;
}

async function copyText(text: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    return false;
  }
}
