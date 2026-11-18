// Creating a team step by step, joining with an invite, removing someone,
// and reviewing suggestions one at a time (docs/decisions.md §35; views
// 2a, 4b, 9a and 7b of the canvas).

import { useEffect, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { errorMessage, useApi } from "../../api/context";
import type { TeamPerson, TeamStatus, TeamSuggestion } from "../../api/types";
import { Button } from "../../components/ui/Button";
import { Dialog } from "../../components/ui/Dialog";
import { useToast } from "../../components/ui/Toast";
import { useT } from "../../i18n";
import { estimateTokens, refreshTeams, teamSuggestionsKey } from "../../team/hooks";
import { foldWord } from "../sync/SyncDialogs";

/** The word typed back to show the team's key was kept (n.º 7). */
const CHECK_WORD = 7;

function Steps({ step, total }: { step: number; total: number }) {
  const t = useT();
  return (
    <div className="sync-steps">
      <div className="sync-steps__bars" aria-hidden>
        {Array.from({ length: total }, (_, i) => (
          <span key={i} className={i < step ? "sync-steps__bar sync-steps__bar--done" : "sync-steps__bar"} />
        ))}
      </div>
      <span className="small muted">{t("sync.step", { n: step, total })}</span>
    </div>
  );
}

function TextField({
  id,
  label,
  value,
  onChange,
  mono,
  placeholder,
  disabled,
}: {
  id: string;
  label: string;
  value: string;
  onChange: (value: string) => void;
  mono?: boolean;
  placeholder?: string;
  disabled?: boolean;
}) {
  return (
    <div className="field">
      <label htmlFor={id}>{label}</label>
      <input
        id={id}
        className={mono ? "mono" : undefined}
        value={value}
        placeholder={placeholder}
        spellCheck={false}
        autoComplete="off"
        maxLength={mono ? undefined : 60}
        disabled={disabled}
        onChange={(e) => onChange(e.target.value)}
      />
    </div>
  );
}

function usePcName(): [string, (name: string) => void] {
  const api = useApi();
  const fallback = useQuery({ queryKey: ["sync", "defaultName"], queryFn: () => api.syncDefaultName() }).data ?? "";
  const [name, setName] = useState<string | null>(null);
  return [name ?? fallback, setName];
}

/** "Crear un equipo": the repo, the names, then the team's 12 words, kept once (2a). */
export function CreateTeamDialog({ onClose }: { onClose: () => void }) {
  const api = useApi();
  const t = useT();
  const toast = useToast();
  const client = useQueryClient();
  const [step, setStep] = useState(1);
  const [url, setUrl] = useState("");
  const [name, setName] = useState("");
  const [me, setMe] = useState("");
  const [pc, setPc] = usePcName();
  const [words, setWords] = useState("");
  const [typed, setTyped] = useState("");

  const create = useMutation({
    mutationFn: () => api.teamCreate(url, name.trim(), me.trim(), pc.trim()),
    onSuccess: (created) => {
      setWords(created.words);
      setStep(3);
      void refreshTeams(client);
    },
  });

  // Closing the window now would lose the words.
  const locked = step === 3 || create.isPending;
  useEffect(() => {
    if (!locked) return;
    return api.onCloseRequested(async () => {
      toast.info(t("sync.words.closeFirst"));
      return false;
    });
  }, [api, locked, toast, t]);

  const list = words.split(/\s+/).filter(Boolean);
  const confirmed = list.length >= CHECK_WORD && foldWord(typed) === foldWord(list[CHECK_WORD - 1] ?? "");
  const finish = () => {
    toast.info(t("team.create.done", { name: name.trim() }));
    onClose();
  };

  const footer =
    step === 1 ? (
      <>
        <Button variant="ghost" onClick={onClose}>
          {t("common.cancel")}
        </Button>
        <Button variant="primary" disabled={!url.trim()} onClick={() => setStep(2)}>
          {t("common.continue")}
        </Button>
      </>
    ) : step === 2 ? (
      <>
        <Button variant="ghost" disabled={create.isPending} onClick={() => setStep(1)}>
          {t("common.back")}
        </Button>
        <Button
          variant="primary"
          disabled={!name.trim() || !me.trim() || !pc.trim() || create.isPending}
          onClick={() => create.mutate()}
        >
          {create.isPending ? t("team.create.creating") : t("team.create.create")}
        </Button>
      </>
    ) : (
      <Button variant="primary" disabled={!confirmed} onClick={finish}>
        {t("sync.words.finish")}
      </Button>
    );

  return (
    <Dialog title={t("team.create.title")} onClose={onClose} dismissible={step < 3 && !create.isPending} footer={footer}>
      <Steps step={step} total={3} />
      {step === 1 && (
        <>
          <div className="sync-lead">
            <h3>{t("team.create.whereTitle")}</h3>
            <p className="muted">{t("team.create.whereText")}</p>
          </div>
          <TextField id="team-url" label={t("sync.url.label")} value={url} mono placeholder={t("team.url.placeholder")} onChange={setUrl} />
          <ul className="team-checks small muted">
            <li>{t("team.create.checkPrivate")}</li>
            <li>{t("team.create.checkEmpty")}</li>
            <li>{t("team.create.checkWrite")}</li>
          </ul>
          <p className="small muted">{t("team.create.protect")}</p>
        </>
      )}
      {step === 2 && (
        <>
          <div className="sync-lead">
            <h3>{t("team.create.namesTitle")}</h3>
            <p className="muted">{t("team.create.namesText")}</p>
          </div>
          <TextField id="team-name" label={t("team.create.name")} value={name} onChange={setName} />
          <TextField id="team-me" label={t("team.me")} value={me} onChange={setMe} />
          <TextField id="team-pc" label={t("sync.name.label")} value={pc} onChange={setPc} />
        </>
      )}
      {step === 3 && (
        <>
          <div className="sync-lead">
            <h3>{t("team.words.title")}</h3>
            <p className="muted">{t("team.words.text")}</p>
          </div>
          <div className="sync-words">
            <p className="sync-words__list mono" aria-label={t("sync.words.label")}>
              {words}
            </p>
          </div>
          <div className="field">
            <label htmlFor="team-check">{t("sync.words.confirm", { n: CHECK_WORD })}</label>
            <input
              id="team-check"
              className="mono"
              value={typed}
              autoComplete="off"
              spellCheck={false}
              onChange={(e) => setTyped(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && confirmed && finish()}
            />
            {typed.trim() && !confirmed && <p className="hint">{t("sync.words.mismatch")}</p>}
          </div>
        </>
      )}
      {create.isError && step < 3 && (
        <p className="error-text" role="alert">
          {errorMessage(create.error, t)}
        </p>
      )}
    </Dialog>
  );
}

/** "Unirme con una invitación": the address and the code, then the wait (4b). */
export function JoinTeamDialog({ onClose }: { onClose: () => void }) {
  const api = useApi();
  const t = useT();
  const client = useQueryClient();
  const [url, setUrl] = useState("");
  const [code, setCode] = useState("");
  const [me, setMe] = useState("");
  const [pc, setPc] = usePcName();
  const join = useMutation({
    mutationFn: () => api.teamJoin(url, code.trim(), me.trim(), pc.trim()),
    onSuccess: () => void refreshTeams(client),
  });
  const waiting = join.data && !join.data.joined;

  return (
    <Dialog
      title={t("team.join.title")}
      onClose={onClose}
      dismissible={!join.isPending}
      footer={
        join.isSuccess ? (
          <Button variant="primary" onClick={onClose}>
            {t("common.close")}
          </Button>
        ) : (
          <>
            <Button variant="ghost" disabled={join.isPending} onClick={onClose}>
              {t("common.cancel")}
            </Button>
            <Button
              variant="primary"
              disabled={!url.trim() || !code.trim() || !me.trim() || !pc.trim() || join.isPending}
              onClick={() => join.mutate()}
            >
              {join.isPending ? t("team.join.joining") : t("team.join.join")}
            </Button>
          </>
        )
      }
    >
      {join.isSuccess ? (
        <div className="team-wait" role="status">
          <span className="team-wait__mark" aria-hidden>
            {waiting ? "…" : "✓"}
          </span>
          <div>
            <strong>{t(waiting ? "team.join.waiting" : "team.join.in")}</strong>
            {waiting && <p className="muted small">{t("team.join.waitingText")}</p>}
          </div>
        </div>
      ) : (
        <>
          <p className="muted">{t("team.join.text")}</p>
          <TextField id="team-join-url" label={t("team.join.url")} value={url} mono placeholder={t("team.url.placeholder")} onChange={setUrl} />
          <TextField id="team-join-code" label={t("team.join.code")} value={code} mono placeholder="7-tigre-lima-nube-faro" onChange={setCode} />
          <TextField id="team-join-me" label={t("team.me")} value={me} onChange={setMe} />
          <TextField id="team-join-pc" label={t("sync.name.label")} value={pc} onChange={setPc} />
          {join.isError && (
            <p className="error-text" role="alert">
              {errorMessage(join.error, t)}
            </p>
          )}
        </>
      )}
    </Dialog>
  );
}

/** "¿Quitar a Luis del equipo?", with the reminder about git access (9a). */
export function RemovePersonDialog({
  team,
  person,
  onClose,
}: {
  team: TeamStatus;
  person: TeamPerson;
  onClose: () => void;
}) {
  const api = useApi();
  const t = useT();
  const toast = useToast();
  const client = useQueryClient();
  const name = person.name;
  const remove = useMutation({
    mutationFn: () => api.teamRemovePerson(team.summary.id, person.person),
    onSuccess: () => {
      toast.info(t("team.remove.done", { name }));
      void refreshTeams(client);
      onClose();
    },
  });
  return (
    <Dialog
      title={t("team.remove.title", { name })}
      onClose={onClose}
      dismissible={!remove.isPending}
      footer={
        <>
          <Button variant="ghost" disabled={remove.isPending} onClick={onClose}>
            {t("common.cancel")}
          </Button>
          <Button variant="danger" disabled={remove.isPending} onClick={() => remove.mutate()}>
            {remove.isPending ? t("team.remove.removing") : t("team.remove.confirm", { name })}
          </Button>
        </>
      }
    >
      <p className="muted">{t("team.remove.text")}</p>
      <p className="small warning">{t("team.remove.git")}</p>
      {remove.isError && (
        <p className="error-text" role="alert">
          {errorMessage(remove.error, t)}
        </p>
      )}
    </Dialog>
  );
}

/** "Sugerencias por revisar": one at a time, now and with the change (7b). */
export function ReviewDialog({ team, onClose }: { team: TeamStatus; onClose: () => void }) {
  const api = useApi();
  const t = useT();
  const client = useQueryClient();
  const id = team.summary.id;
  const list = useQuery({ queryKey: teamSuggestionsKey(id), queryFn: () => api.teamSuggestions(id) });
  const mine = (list.data ?? []).filter((s) => s.canReview);
  const [index, setIndex] = useState(0);
  const current: TeamSuggestion | undefined = mine[Math.min(index, mine.length - 1)];
  const decide = useMutation({
    mutationFn: (accept: boolean) =>
      accept ? api.teamAcceptSuggestion(id, current?.uid ?? "") : api.teamRejectSuggestion(id, current?.uid ?? ""),
    onSuccess: () => void refreshTeams(client),
  });

  if (list.isSuccess && !current) {
    return (
      <Dialog title={t("team.review.title")} onClose={onClose} footer={<Button variant="primary" onClick={onClose}>{t("common.close")}</Button>}>
        <p className="muted">{t("team.review.none")}</p>
      </Dialog>
    );
  }

  const delta = current ? estimateTokens(current.after ?? "") - estimateTokens(current.before ?? "") : 0;
  const free = team.layerMax - team.layerTokens - (current?.teamLayer ? delta : 0);
  return (
    <Dialog
      title={t("team.review.title")}
      onClose={onClose}
      dismissible={!decide.isPending}
      footer={
        current && (
          <>
            <Button variant="ghost" disabled={decide.isPending} onClick={() => decide.mutate(false)}>
              {t("common.reject")}
            </Button>
            <Button variant="primary" disabled={decide.isPending || current.hidden} onClick={() => decide.mutate(true)}>
              {t("team.review.accept")}
            </Button>
          </>
        )
      }
    >
      {current && (
        <div className="team-review">
          <p className="small muted">
            {t("team.review.counter", { n: Math.min(index, mine.length - 1) + 1, total: mine.length, team: team.summary.name })}
          </p>
          <h3>{t(current.note ? "team.review.change" : "team.review.new", { name: current.author })}</h3>
          {current.before !== null && (
            <div className="team-review__side">
              <span className="small muted">{t("team.review.now")}</span>
              <p className="team-review__text">{current.before}</p>
            </div>
          )}
          <div className="team-review__side team-review__side--after">
            <span className="small muted">{t(current.before !== null ? "team.review.with" : "team.review.proposed")}</span>
            <p className="team-review__text">{current.shown ?? t("team.review.removeNote")}</p>
          </div>
          {current.hidden && (
            <p className="error-text" role="alert">
              {t("team.review.hidden")}
            </p>
          )}
          {current.teamLayer && (
            <p className="small muted">
              {t(delta >= 0 ? "team.review.tokensMore" : "team.review.tokensLess", { n: Math.abs(delta), free, max: team.layerMax })}
            </p>
          )}
          {mine.length > 1 && (
            <div className="row">
              <button type="button" className="link-button" disabled={index === 0} onClick={() => setIndex(index - 1)}>
                {t("team.review.previous")}
              </button>
              <button type="button" className="link-button" disabled={index >= mine.length - 1} onClick={() => setIndex(index + 1)}>
                {t("team.review.next")}
              </button>
            </div>
          )}
          {decide.isError && (
            <p className="error-text" role="alert">
              {errorMessage(decide.error, t)}
            </p>
          )}
        </div>
      )}
    </Dialog>
  );
}
