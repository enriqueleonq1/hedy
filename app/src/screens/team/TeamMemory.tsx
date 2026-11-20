// A team project's memory (docs/decisions.md §35): one list with a layer
// filter (5b) — Equipo, Proyecto, Solo tú — and the "¿A quién le sirve?"
// sheet (6a) for a note of this PC.

import { useState } from "react";
import { useQueryClient } from "@tanstack/react-query";

import { errorMessage, useApi } from "../../api/context";
import type { Note, TeamNote } from "../../api/types";
import { Button } from "../../components/ui/Button";
import { Dialog } from "../../components/ui/Dialog";
import { Segmented } from "../../components/ui/controls";
import { useToast } from "../../components/ui/Toast";
import { useT } from "../../i18n";
import { estimateTokens, refreshTeams, useTeamNotes, useTeamStatus } from "../../team/hooks";
import { useMemoryHome } from "../sync/ProjectSync";
import { CATEGORY_LABEL } from "../memory/notes";
import "../../styles/team.css";

export type Layer = "all" | "team" | "project" | "mine";

/** The team whose memory the project holds, if any. */
export function useProjectTeam(project: string): string | null {
  const home = useMemoryHome(project).data?.home;
  return home?.kind === "team" ? home.team : null;
}

export function LayerFilter({ value, onChange, counts }: { value: Layer; onChange: (layer: Layer) => void; counts: Record<Layer, number> }) {
  const t = useT();
  const option = (layer: Layer, key: "team.layer.all" | "team.layer.team" | "team.layer.project" | "team.layer.mine") => ({
    value: layer,
    label: `${t(key)} · ${counts[layer]}`,
  });
  return (
    <Segmented<Layer>
      label={t("team.layer.label")}
      value={value}
      onChange={onChange}
      options={[
        option("all", "team.layer.all"),
        option("team", "team.layer.team"),
        option("project", "team.layer.project"),
        option("mine", "team.layer.mine"),
      ]}
    />
  );
}

/** The team's notes for this project: the team layer's and the project's. */
export function useTeamProjectNotes(team: string | null, project: string): TeamNote[] {
  const notes = useTeamNotes(team).data ?? [];
  return notes.filter((n) => n.teamLayer || n.project === project);
}

/** The team's notes of a layer, with their byline and what this person may do. */
export function TeamNotes({ team, notes, layer }: { team: string; notes: TeamNote[]; layer: Layer }) {
  const t = useT();
  const status = useTeamStatus(team).data;
  const shown = notes.filter((n) => layer === "all" || (layer === "team" ? n.teamLayer : !n.teamLayer));
  if (shown.length === 0) return null;
  return (
    <section aria-labelledby="team-notes">
      <h2 id="team-notes" className="memory__group-title">
        {t("team.notes.title", { team: status?.summary.name ?? "" })}
      </h2>
      <ul className="memory__list team-notes">
        {shown.map((note) => (
          <TeamNoteRow key={note.uid} team={team} note={note} />
        ))}
      </ul>
      {status && <p className="small muted">{t("team.layer", { used: status.layerTokens, max: status.layerMax })}</p>}
    </section>
  );
}

function TeamNoteRow({ team, note }: { team: string; note: TeamNote }) {
  const api = useApi();
  const t = useT();
  const toast = useToast();
  const client = useQueryClient();
  const [editing, setEditing] = useState(false);
  const [text, setText] = useState(note.text);
  const [error, setError] = useState<string | null>(null);
  const save = async () => {
    try {
      const write = await api.teamEditNote(team, note.uid, text.trim());
      toast.info(t(write.published ? "team.note.saved" : "team.note.suggested"));
      setEditing(false);
      setError(null);
      await refreshTeams(client);
    } catch (e) {
      setError(errorMessage(e, t));
    }
  };
  return (
    <li className="note team-note">
      {editing ? (
        <form
          className="note__form"
          onSubmit={(e) => {
            e.preventDefault();
            void save();
          }}
        >
          <textarea aria-label={t("memory.note.text")} value={text} rows={2} onChange={(e) => setText(e.target.value)} />
          {error && (
            <p className="error-text" role="alert">
              {error}
            </p>
          )}
          <div className="row">
            <Button small variant="primary" type="submit" disabled={!text.trim()}>
              {t(note.canEdit ? "common.save" : "team.note.suggest")}
            </Button>
            <Button small variant="ghost" onClick={() => setEditing(false)}>
              {t("common.cancel")}
            </Button>
          </div>
        </form>
      ) : (
        <>
          <p className="note__text">{note.text}</p>
          <div className="note__meta">
            <span className={`category category--${note.category}`}>{t(CATEGORY_LABEL[note.category])}</span>
            <span className={note.teamLayer ? "team-tag team-tag--team" : "team-tag"}>{t(note.teamLayer ? "team.layer.team" : "team.layer.project")}</span>
            <span className="small muted">{note.mine ? t("team.you") : note.author}</span>
            <span className="spacer" />
            <Button
              small
              onClick={() => {
                setText(note.text);
                setEditing(true);
              }}
            >
              {t(note.canEdit ? "common.edit" : "team.note.suggestChange")}
            </Button>
          </div>
        </>
      )}
    </li>
  );
}

/**
 * "¿A quién le sirve?" (6a): a note of this PC (an agent's proposal, or the
 * user's own) stays here, goes to the project's notes with the user's name,
 * or to the whole team, where an admin reviews it first.
 */
export function WhoForDialog({
  note,
  project,
  projectName,
  team,
  initial = "mine",
  onClose,
}: {
  note: Note;
  project: string;
  projectName: string;
  team: string;
  /** The option chosen when it opens: only for the user unless they asked to share. */
  initial?: "mine" | "project" | "team";
  onClose: () => void;
}) {
  const api = useApi();
  const t = useT();
  const toast = useToast();
  const client = useQueryClient();
  const status = useTeamStatus(team).data;
  const proposed = note.status === "proposed";
  const [choice, setChoice] = useState<"mine" | "project" | "team">(initial);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const free = status ? Math.max(0, status.layerMax - status.layerTokens) : 0;
  const cost = estimateTokens(note.text) + 8;

  const run = async (action: () => Promise<unknown>, done: string) => {
    setBusy(true);
    setError(null);
    try {
      await action();
      toast.info(done);
      await Promise.all([refreshTeams(client), client.invalidateQueries({ queryKey: ["memory", project] })]);
      onClose();
    } catch (e) {
      setError(errorMessage(e, t));
    } finally {
      setBusy(false);
    }
  };
  const apply = () => {
    if (choice === "mine") {
      return proposed ? run(() => api.approveNote(note.id), t("team.who.keptMine")) : onClose();
    }
    const everyone = choice === "team";
    return run(
      async () => {
        const write = await api.teamPublishNote(team, note.id, everyone);
        if (!write.published) {
          // An admin may turn it down: until then it is the user's, approved here.
          if (proposed) await api.approveNote(note.id);
          toast.info(t("team.who.suggested"));
        }
      },
      t(everyone ? "team.who.sentTeam" : "team.who.shared", { project: projectName }),
    );
  };

  const option = (value: typeof choice, title: string, text: string) => (
    <label className={choice === value ? "team-who__option team-who__option--on" : "team-who__option"}>
      <input type="radio" name="team-who" checked={choice === value} onChange={() => setChoice(value)} />
      <span>
        <strong>{title}</strong>
        <span className="small muted">{text}</span>
      </span>
    </label>
  );

  return (
    <Dialog
      title={t("team.who.title")}
      onClose={onClose}
      dismissible={!busy}
      footer={
        <>
          {proposed && (
            <Button variant="ghost" disabled={busy} onClick={() => void run(() => api.forgetNote(note.id), t("team.who.discarded"))}>
              {t("team.who.discard")}
            </Button>
          )}
          <Button variant="primary" disabled={busy} onClick={() => void apply()}>
            {choice === "mine"
              ? t(proposed ? "team.who.approveMine" : "common.cancel")
              : choice === "project"
                ? t("team.who.shareProject", { project: projectName })
                : t("team.who.sendTeam")}
          </Button>
        </>
      }
    >
      {proposed && <p className="small muted">{t("team.who.agentProposes")}</p>}
      <blockquote className="team-who__quote">«{note.text}»</blockquote>
      <div className="team-who" role="radiogroup" aria-label={t("team.who.title")}>
        {option("mine", t("team.who.mine"), t("team.who.mineText"))}
        {option("project", t("team.who.project", { project: projectName }), t("team.who.projectText"))}
        {option(
          "team",
          t("team.who.team", { team: status?.summary.name ?? "" }),
          t("team.who.teamText", { n: cost, free }),
        )}
      </div>
      {error && (
        <p className="error-text" role="alert">
          {error}
        </p>
      )}
    </Dialog>
  );
}
