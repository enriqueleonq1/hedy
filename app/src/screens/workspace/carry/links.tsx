// Both ends of a carried conversation: the task says where it came from, the
// conversation lists the tasks that continue it; plus the quiet suggestion
// under a finished answer and the context shown in the task's first message.

import { Fragment, useId, useState } from "react";

import type { Overview, TaskOrigin } from "../../../api/types";
import type { View } from "../../../components/Layout";
import { Button } from "../../../components/ui/Button";
import { ChevronIcon } from "../../../components/ui/icons";
import { useT } from "../../../i18n";
import { useConversations } from "../../../tasks/hooks";
import { CarryIcon, NoteIcon } from "../icons";
import { useContinuations } from "./queries";
import "./carry.css";

/** "Viene de: conversación «…»", a link while the conversation exists. */
export function OriginLink({ origin, onNavigate }: { origin: TaskOrigin; onNavigate: (view: View) => void }) {
  const t = useT();
  const conversations = useConversations(true);
  const exists = conversations.data?.some((c) => c.id === origin.conversation) ?? true;
  if (!exists) return <span className="carry-link carry-link--gone">{t("carry.fromDeleted", { title: origin.title })}</span>;
  return (
    <button
      type="button"
      className="carry-link"
      aria-label={t("carry.fromLabel", { title: origin.title })}
      title={t("carry.fromLabel", { title: origin.title })}
      onClick={() => onNavigate({ screen: "conversation", task: origin.conversation })}
    >
      {t("carry.from", { title: origin.title })}
    </button>
  );
}

/** "Continuada en: <proyecto> · <tarea>" for each task that continues the conversation. */
export function ContinuedIn({
  conversation,
  overview,
  onNavigate,
}: {
  conversation: string;
  overview: Overview;
  onNavigate: (view: View) => void;
}) {
  const t = useT();
  const tasks = useContinuations(conversation).data ?? [];
  if (tasks.length === 0) return null;
  return (
    <span className="carry-continued">
      {t("carry.continued")}{" "}
      {tasks.map((task, i) => {
        const project = overview.projects.find((p) => p.path === task.project)?.name ?? task.project;
        return (
          <Fragment key={task.id}>
            {i > 0 && ", "}
            <button
              type="button"
              className="carry-link"
              aria-label={t("carry.continuedLabel", { task: task.title, project })}
              title={t("carry.continuedLabel", { task: task.title, project })}
              onClick={() => onNavigate({ screen: "workspace", project: task.project, task: task.id })}
            >
              {project} · {task.title}
            </button>
          </Fragment>
        );
      })}
    </span>
  );
}

/** The quiet next steps under a finished answer of a conversation. */
export function CarrySuggestion({ onCarry, onNote }: { onCarry: () => void; onNote: () => void }) {
  const t = useT();
  return (
    <div className="carry-suggest" role="group" aria-label={t("carry.suggest")}>
      <span className="carry-suggest__label">{t("carry.suggest")}</span>
      <Button small variant="ghost" onClick={onCarry}>
        <CarryIcon />
        {t("carry.action")}
      </Button>
      <Button small variant="ghost" onClick={onNote}>
        <NoteIcon />
        {t("carry.saveNote")}
      </Button>
    </div>
  );
}

/**
 * The first message of a carried task: the instruction, with the context the
 * agent got before it folded away ("Contexto: conversación «…»").
 */
export function CarriedContext({ origin }: { origin: TaskOrigin }) {
  const t = useT();
  const id = useId();
  const [open, setOpen] = useState(false);
  return (
    <div className="carried-context">
      <button
        type="button"
        className="carried-context__toggle"
        aria-expanded={open}
        aria-controls={id}
        title={open ? t("carry.collapse") : t("carry.expand")}
        onClick={() => setOpen((o) => !o)}
      >
        <span className={open ? "carry-card__chevron carry-card__chevron--open" : "carry-card__chevron"} aria-hidden>
          <ChevronIcon />
        </span>
        {t("carry.contextTitle", { title: origin.title })}
      </button>
      <pre id={id} className="carried-context__text" hidden={!open} aria-label={t("carry.contextShown")}>
        {origin.context}
      </pre>
    </div>
  );
}
