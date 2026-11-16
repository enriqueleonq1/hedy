import { useState } from "react";

import { useAction } from "../../../api/context";
import type { NoteCategory, Overview, Task } from "../../../api/types";
import type { View } from "../../../components/Layout";
import { Button } from "../../../components/ui/Button";
import { Field } from "../../../components/ui/controls";
import { Dialog } from "../../../components/ui/Dialog";
import { Select } from "../../../components/ui/Select";
import { useToast } from "../../../components/ui/Toast";
import { useT, type MessageKey } from "../../../i18n";
import { useCreate } from "../../create/CreateProvider";
import { projectOption, useConversationContext } from "./queries";
import "./carry.css";

const CATEGORIES: NoteCategory[] = ["decision", "convention", "rule", "preference"];
const CATEGORY_LABEL: Record<NoteCategory, MessageKey> = {
  decision: "memory.category.decision",
  convention: "memory.category.convention",
  rule: "memory.category.rule",
  preference: "memory.category.preference",
};

/**
 * "Guardar como nota del proyecto": the conversation's final answer (editable),
 * titled after it, in a project's memory. The user writes it, so it is
 * approved right away (rule 6 only makes agents propose).
 */
export function NoteDialog({
  overview,
  conversation,
  defaultProject,
  onClose,
  onNavigate,
}: {
  overview: Overview;
  conversation: Task;
  /** Preselected: where the conversation was continued, if anywhere. */
  defaultProject?: string;
  onClose: () => void;
  onNavigate: (view: View) => void;
}) {
  const t = useT();
  const toast = useToast();
  const create = useCreate();
  const context = useConversationContext(conversation.id);
  const options = overview.projects.map((p) => ({ ...projectOption(p, overview.accounts, t), disabled: false }));
  const [project, setProject] = useState(
    () => options.find((o) => o.value === defaultProject)?.value ?? options[0]?.value ?? "",
  );
  const [title, setTitle] = useState(conversation.title);
  // The final answer, once loaded; the user's edits win from then on.
  const [text, setText] = useState<string | null>(null);
  const answer = context.data?.turns.at(-1)?.answer ?? "";
  const body = text ?? answer;
  const [category, setCategory] = useState<NoteCategory>("decision");
  const name = overview.projects.find((p) => p.path === project)?.name ?? project;

  const save = useAction(
    (api, vars: { project: string; title: string; text: string; category: NoteCategory }) =>
      api.saveConversationNote(conversation.id, vars.project, vars.title, vars.text, vars.category),
    {
      invalidate: [["memory"]],
      onSuccess: (_note, vars) => {
        onClose();
        toast.info(t("carry.note.saved", { project: name }), {
          action: {
            label: t("carry.note.open"),
            onClick: () => onNavigate({ screen: "memory", project: vars.project, tab: "notes" }),
          },
        });
      },
    },
  );
  const ready = !!project && !!(title.trim() || body.trim()) && !save.isPending;
  const submit = () => {
    if (ready) save.mutate({ project, title: title.trim(), text: body.trim(), category });
  };

  return (
    <Dialog
      title={t("carry.note.title")}
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>{t("common.cancel")}</Button>
          {options.length === 0 ? (
            <Button
              variant="primary"
              onClick={() => {
                onClose();
                create.newProject({ ...(context.data ? { name: context.data.suggestedName } : {}) });
              }}
            >
              {t("carry.newProject")}
            </Button>
          ) : (
            <Button variant="primary" disabled={!ready} onClick={submit}>
              {t("carry.note.save")}
            </Button>
          )}
        </>
      }
    >
      {options.length === 0 ? (
        <p>{t("carry.note.noProjects")}</p>
      ) : (
        <form
          className="carry-note"
          onSubmit={(e) => {
            e.preventDefault();
            submit();
          }}
        >
          <Field label={t("carry.note.project")}>
            {(id) => <Select id={id} title={t("carry.note.project")} value={project} onChange={setProject} options={options} />}
          </Field>
          <Field label={t("carry.note.noteTitle")}>
            {(id) => <input id={id} value={title} autoComplete="off" onChange={(e) => setTitle(e.target.value)} />}
          </Field>
          <Field label={t("carry.note.text")}>
            {(id) => (
              <textarea
                id={id}
                className="carry-note__text"
                rows={8}
                value={body}
                onChange={(e) => setText(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
                    e.preventDefault();
                    submit();
                  }
                }}
              />
            )}
          </Field>
          <Field label={t("carry.note.category")}>
            {(id) => (
              <Select<NoteCategory>
                id={id}
                title={t("carry.note.category")}
                value={category}
                onChange={setCategory}
                options={CATEGORIES.map((value) => ({ value, label: t(CATEGORY_LABEL[value]) }))}
              />
            )}
          </Field>
          <p className="hint">{t("carry.note.hint")}</p>
        </form>
      )}
    </Dialog>
  );
}
