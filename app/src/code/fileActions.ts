// What every code view can do with a file of a copy: open it in VS Code, show
// it in the file manager, copy its path, ask the AI about it. One hook so the
// explorer, the viewer and the search results behave the same.

import { useMemo } from "react";

import { errorMessage, useApi } from "../api/context";
import type { CodeCopy, CopyInfo } from "../api/codeTypes";
import { useToast } from "../components/ui/Toast";
import { useT } from "../i18n";
import { insertInComposer } from "../screens/workspace/composerBridge";
import { absolutePath, copyText, lineReference } from "./copies";

export interface FileActions {
  openInEditor(path: string, line?: number): void;
  reveal(path: string): void;
  copyPath(path: string): void;
  copyRelativePath(path: string): void;
  copyLineReference(path: string, start: number, end?: number): void;
  copySelection(text: string): void;
  /** Puts `@path ` (or `path:10-24 `) in the composer. */
  askAi(path: string, lines?: { start: number; end: number }): void;
}

export function useFileActions(copy: CodeCopy, info: CopyInfo | undefined): FileActions {
  const api = useApi();
  const toast = useToast();
  const t = useT();

  return useMemo(() => {
    const fail = (error: unknown) => toast.error(errorMessage(error, t));
    const copied = async (text: string, message: string) => {
      if (await copyText(text)) toast.info(message, { durationMs: 2500 });
      else toast.error(t("code.copyFailed"));
    };
    return {
      openInEditor: (path, line) => void api.openFileInEditor(copy, path, line).catch(fail),
      reveal: (path) => void api.revealInFileManager(copy, path).catch(fail),
      copyPath: (path) => void copied(info ? absolutePath(info.root, path) : path, t("code.pathCopied")),
      copyRelativePath: (path) => void copied(path, t("code.pathCopied")),
      copyLineReference: (path, start, end) => void copied(lineReference(path, start, end), t("code.referenceCopied")),
      copySelection: (text) => void copied(text, t("code.textCopied")),
      askAi: (path, lines) => {
        const reference = lines ? lineReference(path, lines.start, lines.end) : `@${path}`;
        if (!insertInComposer(`${reference} `)) toast.error(t("code.noComposer"));
      },
    };
  }, [api, copy, info, toast, t]);
}
