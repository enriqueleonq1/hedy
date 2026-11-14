import { useId, useState } from "react";

import { useAction } from "../../api/context";
import type { PromptTemplate } from "../../api/types";
import { Button } from "../../components/ui/Button";
import { Segmented } from "../../components/ui/controls";
import { Dialog } from "../../components/ui/Dialog";
import { TEMPLATES_KEY } from "../../tasks/hooks";
import { placeholders } from "../../templates/placeholders";
import { useT } from "../../i18n";

const GLOBAL = "";

/**
 * Creates a template (choosing where it applies) or edits one (its name and
 * text; the scope stays).
 */
export function TemplateDialog({
  template,
  initialText = "",
  projects,
  defaultProject = null,
  title,
  onClose,
}: {
  /** The template to edit; without it, a new one. */
  template?: PromptTemplate;
  initialText?: string;
  /** Projects a new template can belong to. */
  projects: { path: string; name: string }[];
  defaultProject?: string | null;
  title?: string;
  onClose: () => void;
}) {
  const t = useT();
  const nameId = useId();
  const textId = useId();
  const whereId = useId();
  const [name, setName] = useState(template?.name ?? "");
  const [text, setText] = useState(template?.text ?? initialText);
  const [project, setProject] = useState<string>(defaultProject ?? GLOBAL);
  const save = useAction(
    (api) =>
      template
        ? api.updateTemplate(template.id, name, text)
        : api.addTemplate({ project: project || null, name, text }),
    { invalidate: [TEMPLATES_KEY], onSuccess: () => onClose() },
  );
  const asks = placeholders(text);
  const example = `{${t("templates.exampleName")}}`;

  return (
    <Dialog
      title={title ?? (template ? t("templates.editTitle") : t("templates.new"))}
      onClose={onClose}
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            {t("common.cancel")}
          </Button>
          <Button variant="primary" disabled={!name.trim() || !text.trim() || save.isPending} onClick={() => save.mutate(undefined)}>
            {t("templates.save")}
          </Button>
        </>
      }
    >
      <div className="field">
        <label htmlFor={nameId}>{t("templates.name")}</label>
        <input id={nameId} value={name} maxLength={60} onChange={(e) => setName(e.target.value)} />
      </div>
      <div className="field">
        <label htmlFor={textId}>{t("templates.text")}</label>
        <textarea id={textId} value={text} rows={5} onChange={(e) => setText(e.target.value)} aria-describedby={`${textId}-hint`} />
        <p className="hint" id={`${textId}-hint`}>
          {asks.length > 0
            ? t("templates.placeholders", { list: asks.map((p) => `{${p}}`).join(", ") })
            : t("templates.textHint", { example })}
        </p>
      </div>
      {!template && projects.length === 1 && (
        <div className="field">
          <span className="field__label">{t("templates.where")}</span>
          <Segmented<string>
            label={t("templates.where")}
            value={project}
            options={[
              { value: projects[0]?.path ?? GLOBAL, label: t("templates.thisProject") },
              { value: GLOBAL, label: t("templates.global") },
            ]}
            onChange={setProject}
          />
        </div>
      )}
      {!template && projects.length > 1 && (
        <div className="field">
          <label htmlFor={whereId}>{t("templates.where")}</label>
          <select id={whereId} value={project} onChange={(e) => setProject(e.target.value)}>
            <option value={GLOBAL}>{t("templates.global")}</option>
            {projects.map((p) => (
              <option key={p.path} value={p.path}>
                {t("templates.ofProject", { project: p.name })}
              </option>
            ))}
          </select>
        </div>
      )}
    </Dialog>
  );
}
