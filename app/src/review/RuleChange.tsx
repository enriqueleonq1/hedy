import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";

import { errorMessage, useApi } from "../api/context";
import type { MemorySource } from "../api/types";
import { Button } from "../components/ui/Button";
import { Dialog } from "../components/ui/Dialog";
import { useToast } from "../components/ui/Toast";
import { useT } from "../i18n";
import { MEMORY_GRAPH_KEY } from "../screens/ask/AskScreen";
import { refreshTeams } from "../team/hooks";

type Choice = "change" | "remove" | "exception";

/**
 * A note's text without its hidden characters: the check shows them written
 * out (`⟦U+200B⟧`), the memory as they are.
 */
function comparable(text: string): string {
  return text
    .replace(/⟦U\+[0-9A-F]{4,6}⟧/g, "")
    .replace(/[\p{Cc}\p{Cf}\p{Co}\p{Zl}\p{Zp}\u{FE00}-\u{FE0F}]/gu, "")
    .trim();
}

/**
 * "La regla ya no aplica" (docs/decisions.md §37): instead of ignoring a
 * warning, the rule itself is fixed. The user's own note changes now; a
 * team's goes to whoever looks after it as a suggestion (rule 6: nobody's
 * note changes behind them); or the project gets an exception, a note of its
 * own beside the rule.
 */
export function RuleChangeDialog({
  rule,
  project,
  onClose,
  onDone,
}: {
  rule: MemorySource;
  project: string;
  onClose: () => void;
  /** It changed (or got an exception) here: its warnings wait for a new check. A mere suggestion settles nothing. */
  onDone: (rule: MemorySource) => void;
}) {
  const t = useT();
  const api = useApi();
  const client = useQueryClient();
  const toast = useToast();
  const [choice, setChoice] = useState<Choice>("change");
  const [text, setText] = useState(rule.text);
  const [exception, setException] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const team = rule.note.kind === "team" ? rule.note : null;
  const local = rule.note.kind === "local" ? rule.note : null;
  // The note as it is now, not as the check saw it: it may have changed or
  // gone since (and a new note may have taken its number). A team's also
  // says whether this person may change it for everyone.
  const now = useQuery({
    queryKey: ["ruleNow", rule.note],
    queryFn: async () => {
      if (team) {
        const note = (await api.teamNotes(team.team)).find((n) => n.uid === team.uid);
        return note ? { text: note.text, canEdit: note.canEdit } : null;
      }
      const note = (await api.memory(project)).notes.find((n) => n.id === local?.id);
      return note ? { text: note.text, canEdit: true } : null;
    },
    staleTime: 0,
    gcTime: 0,
  });
  const changedSince = now.data === null || (!!now.data && comparable(now.data.text) !== comparable(rule.text));
  const forEveryone = !!team && !!now.data?.canEdit;

  const written = choice === "change" ? text.trim() : choice === "exception" ? exception.trim() : "x";
  const unchanged = choice === "change" && text.trim() === rule.text.trim();
  // Only an exception (a note of its own) is safe on a rule that changed.
  const blocked = choice !== "exception" && (now.isPending || now.isError || changedSince);

  const submit = async () => {
    setBusy(true);
    setError(null);
    try {
      let settled = true;
      if (choice === "exception") {
        await api.addNote(project, t("rules.change.exceptionNote", { rule: rule.text, text: exception.trim() }), "convention");
        toast.info(t("rules.change.excepted"));
      } else if (team) {
        const write = await api.teamEditNote(team.team, team.uid, choice === "remove" ? null : text.trim());
        // A suggestion changes nothing yet: the rule still holds.
        settled = write.published;
        toast.info(
          t(!write.published ? "rules.change.suggested" : choice === "remove" ? "rules.change.removedTeam" : "rules.change.savedTeam"),
        );
      } else if (local) {
        if (choice === "remove") await api.forgetNote(local.id);
        else await api.editNote(local.id, text.trim(), rule.category);
        toast.info(t(choice === "remove" ? "rules.change.removed" : "rules.change.saved"));
      }
      await Promise.all([
        refreshTeams(client),
        client.invalidateQueries({ queryKey: MEMORY_GRAPH_KEY }),
        client.invalidateQueries({ queryKey: ["status", project] }),
      ]);
      if (settled) onDone(rule);
      onClose();
    } catch (e) {
      setError(errorMessage(e, t));
    } finally {
      setBusy(false);
    }
  };

  const options: { value: Choice; label: string }[] = [
    { value: "change", label: t("rules.change.change") },
    { value: "remove", label: t("rules.change.remove") },
    { value: "exception", label: t("rules.change.exception") },
  ];

  return (
    <Dialog
      title={t("rules.change.title")}
      onClose={onClose}
      // Mid-save, closing would not undo it.
      dismissible={!busy}
      footer={
        <>
          <Button disabled={busy} onClick={onClose}>
            {t("common.cancel")}
          </Button>
          <Button
            variant={choice === "remove" ? "danger" : "primary"}
            disabled={busy || blocked || !written || unchanged}
            onClick={() => void submit()}
          >
            {choice === "exception"
              ? t("rules.change.addException")
              : !team
                ? choice === "remove"
                  ? t("rules.change.removeNow")
                  : t("rules.change.apply")
                : !forEveryone
                  ? t("rules.change.propose")
                  : choice === "remove"
                    ? t("rules.change.removeAll")
                    : t("rules.change.applyAll")}
          </Button>
        </>
      }
    >
      <div className="rl-change">
        <p className="muted">
          {!team
            ? t("rules.change.ownHint")
            : forEveryone
              ? t("rules.change.teamEditHint", { team: rule.team ?? t("rules.teamNoName") })
              : t("rules.change.teamHint", { team: rule.team ?? t("rules.teamNoName") })}
        </p>
        {!team && !rule.project && <p className="muted">{t("rules.change.everyProject")}</p>}
        {changedSince && (
          <p className="error-text" role="alert">
            {t("rules.change.changedSince")}
          </p>
        )}
        <div className="rl-change__now">
          <span className="rl-change__label">{t("rules.change.now")}</span>
          <p>{rule.text}</p>
        </div>
        <fieldset className="rl-change__choices">
          <legend className="sr-only">{t("rules.change.what")}</legend>
          {options.map((o) => (
            <label key={o.value} className="rl-change__choice">
              <input
                type="radio"
                name="rule-change"
                value={o.value}
                checked={choice === o.value}
                onChange={() => {
                  setChoice(o.value);
                  setError(null);
                }}
              />
              {o.label}
            </label>
          ))}
        </fieldset>
        {choice === "change" && (
          <label className="rl-change__field">
            <span className="rl-change__label">{t("rules.change.new")}</span>
            <textarea rows={3} value={text} onChange={(e) => setText(e.target.value)} />
          </label>
        )}
        {choice === "change" && (unchanged || !text.trim()) && <p className="muted small">{t("rules.change.writeNew")}</p>}
        {choice === "exception" && (
          <label className="rl-change__field">
            <span className="rl-change__label">{t("rules.change.exceptionLabel")}</span>
            <textarea
              rows={3}
              value={exception}
              placeholder={t("rules.change.exceptionPlaceholder")}
              onChange={(e) => setException(e.target.value)}
            />
          </label>
        )}
        {choice === "remove" && (
          <p className="muted">
            {!team ? t("rules.change.removeOwn") : forEveryone ? t("rules.change.removeTeamNow") : t("rules.change.removeTeam")}
          </p>
        )}
        {error && (
          <p className="error-text" role="alert">
            {error}
          </p>
        )}
      </div>
    </Dialog>
  );
}
