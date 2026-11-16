import { useState } from "react";

import type { Overview, Task } from "../../../api/types";
import type { View } from "../../../components/Layout";
import { Button } from "../../../components/ui/Button";
import { Field } from "../../../components/ui/controls";
import { Dialog } from "../../../components/ui/Dialog";
import { Select } from "../../../components/ui/Select";
import { useT } from "../../../i18n";
import { useCreate } from "../../create/CreateProvider";
import { projectOption, useConversationContext } from "./queries";
import "./carry.css";

/**
 * "Llevar a un proyecto": an existing project (its new task opens with the
 * conversation as context) or a new one, named after the conversation.
 */
export function CarryDialog({
  overview,
  conversation,
  onClose,
  onNavigate,
}: {
  overview: Overview;
  conversation: Task;
  onClose: () => void;
  onNavigate: (view: View) => void;
}) {
  const t = useT();
  const create = useCreate();
  const context = useConversationContext(conversation.id);
  const options = overview.projects.map((p) => projectOption(p, overview.accounts, t));
  const [project, setProject] = useState(() => options.find((o) => !o.disabled)?.value ?? "");
  const hasProjects = options.length > 0;
  const ready = options.some((o) => o.value === project && !o.disabled);
  const name = context.data?.suggestedName;

  const openExisting = () => {
    if (!ready) return;
    onClose();
    onNavigate({ screen: "workspace", project, task: null, carry: conversation.id });
  };
  const openNew = () => {
    onClose();
    create.newProject({ ...(name ? { name } : {}), carry: conversation.id });
  };

  return (
    <Dialog
      title={t("carry.dialogTitle")}
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>{t("common.cancel")}</Button>
          <Button variant={hasProjects ? "ghost" : "primary"} onClick={openNew}>
            {t("carry.newProject")}
          </Button>
          {hasProjects && (
            <Button variant="primary" disabled={!ready} onClick={openExisting}>
              {t("carry.openTask")}
            </Button>
          )}
        </>
      }
    >
      <div className="carry-dialog">
        <p className="muted">{t("carry.dialogBody")}</p>
        {hasProjects ? (
          <Field label={t("carry.existing")}>
            {(id) => (
              <Select id={id} title={t("carry.existing")} value={project} onChange={setProject} options={options} />
            )}
          </Field>
        ) : (
          <p>{t("carry.noProjects")}</p>
        )}
        {name && <p className="hint">{t("carry.newProjectHint", { name })}</p>}
      </div>
    </Dialog>
  );
}
