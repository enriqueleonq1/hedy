// Helpers shared by the skills tab, its cards and its dialog.

import { useAction } from "../../api/context";
import type { HedyApi } from "../../api/HedyApi";
import type { Skill, SkillFiles } from "../../api/types";
import { useToast } from "../../components/ui/Toast";
import type { Translate } from "../../i18n";

export const skillsKey = (project: string) => ["skills", project] as const;

/** Runs a skills action and refreshes the project's skills. */
export function useSkillAction<TVars, TResult>(project: string, action: (api: HedyApi, vars: TVars) => Promise<TResult>) {
  return useAction(action, { invalidate: [skillsKey(project)] });
}

/** Tells what happened to the CLIs' files, warning about the user's own files left alone. */
export function useFilesNotice() {
  const toast = useToast();
  return (kind: "saved" | "removed", name: string, files: SkillFiles, t: Translate) => {
    if (kind === "saved") {
      const n = files.written.length;
      toast.info(n > 0 ? t("skills.files.written", { name, n }) : t("skills.files.writtenNone", { name }));
    } else {
      const n = files.removed.length;
      toast.info(n > 0 ? t("skills.files.removed", { name, n }) : t("skills.files.removedNone", { name }));
    }
    for (const path of files.skipped) toast.error(t("skills.files.skipped", { path }));
  };
}

export function skillAuthor(skill: Skill, t: Translate): string {
  return skill.author === "user" ? t("memory.note.you") : skill.author;
}

export type DiffLine = { text: string; kind: "same" | "added" | "removed" };

/**
 * A line diff of two bodies (longest common subsequence): enough to show
 * what a proposed update changes, line by line.
 */
export function lineDiff(before: string, after: string): DiffLine[] {
  const a = before.split("\n");
  const b = after.split("\n");
  const width = b.length + 1;
  // lcs[i * width + j]: longest common run of a[i..] and b[j..].
  const lcs = new Array<number>((a.length + 1) * width).fill(0);
  const at = (i: number, j: number) => lcs[i * width + j] ?? 0;
  for (let i = a.length - 1; i >= 0; i--) {
    for (let j = b.length - 1; j >= 0; j--) {
      lcs[i * width + j] = a[i] === b[j] ? at(i + 1, j + 1) + 1 : Math.max(at(i + 1, j), at(i, j + 1));
    }
  }
  const out: DiffLine[] = [];
  let i = 0;
  let j = 0;
  while (i < a.length || j < b.length) {
    const old = a[i];
    const next = b[j];
    if (old !== undefined && old === next) {
      out.push({ text: old, kind: "same" });
      i++;
      j++;
    } else if (old !== undefined && (next === undefined || at(i + 1, j) >= at(i, j + 1))) {
      out.push({ text: old, kind: "removed" });
      i++;
    } else if (next !== undefined) {
      out.push({ text: next, kind: "added" });
      j++;
    }
  }
  return out;
}
