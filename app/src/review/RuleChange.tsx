import { useState } from "react";
import { useQueryClient } from "@tanstack/react-query";

import { errorMessage, useApi } from "../api/context";
import type { MemorySource } from "../api/types";
import { Button } from "../components/ui/Button";
import { Dialog } from "../components/ui/Dialog";
import { useToast } from "../components/ui/Toast";
import { useT } from "../i18n";
import { refreshTeams } from "../team/hooks";

type Choice = "change" | "remove" | "exception";

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
  /** It was changed, proposed or excepted: its warnings are settled. */
  onDone: () => void;
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

  const written = choice === "change" ? text.trim() : choice === "exception" ? exception.trim() : "x";
  const unchanged = choice === "change" && text.trim() === rule.text.trim();

  const submit = async () => {
    setBusy(true);
    setError(null);
    try {
      if (choice === "exception") {
        await api.addNote(project, t("rules.change.exceptionNote", { rule: rule.text, text: exception.trim() }), "convention");
        toast.info(t("rules.change.excepted"));
      } else if (team) {
        const write = await api.teamEditNote(team.team, team.uid, choice === "remove" ? null : text.trim());
        toast.info(t(write.published ? "rules.change.saved" : "rules.change.suggested"));
      } else if (local) {
        if (choice === "remove") await api.forgetNote(local.id);
        else await api.editNote(local.id, text.trim(), rule.category);
        toast.info(t(choice === "remove" ? "rules.change.removed" : "rules.change.saved"));
      }
      await refreshTeams(client);
      onDone();
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
      footer={
        <>
          <Button onClick={onClose}>{t("common.cancel")}</Button>
          <Button variant="primary" disabled={busy || !written || unchanged} onClick={() => void submit()}>
            {choice === "exception" ? t("rules.change.addException") : team ? t("rules.change.propose") : t("rules.change.apply")}
          </Button>
        </>
      }
    >
      <div className="rl-change">
        <p className="muted">
          {team
            ? t("rules.change.teamHint", { team: rule.team ?? t("rules.teamNoName") })
            : t("rules.change.ownHint")}
        </p>
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
                onChange={() => setChoice(o.value)}
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
          <p className="muted">{team ? t("rules.change.removeTeam") : t("rules.change.removeOwn")}</p>
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
