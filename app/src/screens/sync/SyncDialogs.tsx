// Setting up the sync on a first PC, adding this PC to it, and how to add
// another one: short step-by-step dialogs (docs/decisions.md §34).

import { useEffect, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { errorMessage, useApi } from "../../api/context";
import { Button } from "../../components/ui/Button";
import { Dialog } from "../../components/ui/Dialog";
import { useToast } from "../../components/ui/Toast";
import { useT } from "../../i18n";
import type { SyncDevice } from "../../api/types";
import { deviceName, refreshAfterSync } from "../../sync/hooks";

/** The word the user types back to show the key was kept (n.º 7). */
const CHECK_WORD = 7;

const CREATE_REPO = [
  { name: "GitHub", url: "https://github.com/new" },
  { name: "GitLab", url: "https://gitlab.com/projects/new" },
  { name: "Bitbucket", url: "https://bitbucket.org/repo/create" },
];

/**
 * A recovery word as compared: any case, Spanish accents optional (as the core reads them; `ñ`
 * stays). The words come with their accents as separate marks (`o` + `´`), as typed ones may not.
 */
export function foldWord(word: string): string {
  return word
    .trim()
    .toLowerCase()
    .normalize("NFD")
    .replace(/n\u0303/g, "ñ")
    .replace(/[\u0300-\u036f]/g, "");
}

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

function Lead({ title, text }: { title: string; text: string }) {
  return (
    <div className="sync-lead">
      <h3>{title}</h3>
      <p className="muted">{text}</p>
    </div>
  );
}

function UrlField({
  value,
  onChange,
  disabled,
}: {
  value: string;
  onChange: (url: string) => void;
  disabled?: boolean;
}) {
  const t = useT();
  return (
    <div className="field">
      <label htmlFor="sync-url">{t("sync.url.label")}</label>
      <input
        id="sync-url"
        className="mono"
        value={value}
        placeholder={t("sync.url.placeholder")}
        spellCheck={false}
        autoComplete="off"
        disabled={disabled}
        onChange={(e) => onChange(e.target.value)}
      />
    </div>
  );
}

function NameField({ value, onChange }: { value: string; onChange: (name: string) => void }) {
  const t = useT();
  return (
    <div className="field">
      <label htmlFor="sync-name">{t("sync.name.label")}</label>
      <input id="sync-name" value={value} maxLength={60} onChange={(e) => onChange(e.target.value)} />
    </div>
  );
}

function useDefaultName(): [string, (name: string) => void] {
  const api = useApi();
  const fallback = useQuery({ queryKey: ["sync", "defaultName"], queryFn: () => api.syncDefaultName() }).data ?? "";
  const [name, setName] = useState<string | null>(null);
  return [name ?? fallback, setName];
}

/** "Configurar": the repo's address, this PC's name, then the 12 words, kept once. */
export function SetupDialog({ onClose }: { onClose: () => void }) {
  const api = useApi();
  const t = useT();
  const toast = useToast();
  const client = useQueryClient();
  const [step, setStep] = useState(1);
  const [url, setUrl] = useState("");
  const [name, setName] = useDefaultName();
  const [publicRepo, setPublicRepo] = useState(false);
  const [words, setWords] = useState("");
  const [typed, setTyped] = useState("");
  const [copied, setCopied] = useState(false);

  const check = useMutation({
    mutationFn: () => api.syncRepoIsPublic(url),
    onSuccess: (isPublic) => (isPublic ? setPublicRepo(true) : setStep(2)),
  });
  const setup = useMutation({
    mutationFn: () => api.syncSetup(url, name.trim()),
    onSuccess: (got) => {
      setWords(got);
      setStep(3);
      void refreshAfterSync(client);
    },
  });

  // Closing the window now would lose the words: it waits for "Terminar".
  const locked = step === 3 || setup.isPending;
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
    // The key goes to a password manager, not stays in the clipboard.
    if (copied) void navigator.clipboard?.writeText("").catch(() => {});
    toast.info(t("sync.setup.done"));
    onClose();
  };
  const error = check.error ?? setup.error;

  const footer =
    step === 1 ? (
      <>
        <Button variant="ghost" onClick={onClose}>
          {t("common.cancel")}
        </Button>
        <Button
          variant="primary"
          disabled={!url.trim() || check.isPending}
          onClick={() => (publicRepo ? setStep(2) : check.mutate())}
        >
          {check.isPending ? t("sync.url.checking") : publicRepo ? t("sync.url.anyway") : t("common.continue")}
        </Button>
      </>
    ) : step === 2 ? (
      <>
        <Button
          variant="ghost"
          disabled={setup.isPending}
          onClick={() => {
            setup.reset();
            setStep(1);
          }}
        >
          {t("common.back")}
        </Button>
        <Button variant="primary" disabled={!name.trim() || setup.isPending} onClick={() => setup.mutate()}>
          {setup.isPending ? t("sync.setup.starting") : t("sync.setup.start")}
        </Button>
      </>
    ) : (
      <Button variant="primary" disabled={!confirmed} onClick={finish}>
        {t("sync.words.finish")}
      </Button>
    );

  // The words are shown once: the dialog only closes with "Terminar" (and
  // not while the repo takes the first epoch, whose words would be lost).
  return (
    <Dialog
      title={t("sync.setup.title")}
      onClose={onClose}
      dismissible={step < 3 && !setup.isPending}
      footer={footer}
    >
      <Steps step={step} total={3} />
      {step === 1 && (
        <>
          <Lead title={t("sync.url.title")} text={t("sync.url.text")} />
          <UrlField
            value={url}
            disabled={check.isPending}
            onChange={(next) => {
              setUrl(next);
              setPublicRepo(false);
              check.reset();
              setup.reset();
            }}
          />
          {publicRepo && (
            <p className="small warning" role="status">
              {t("sync.url.public")}
            </p>
          )}
          <p className="small muted">
            {t("sync.url.create")}{" "}
            {CREATE_REPO.map((host, i) => (
              <span key={host.name}>
                {i > 0 && " · "}
                <button type="button" className="link-button" onClick={() => void api.openLink(host.url)}>
                  {host.name}
                </button>
              </span>
            ))}
          </p>
        </>
      )}
      {step === 2 && (
        <>
          <Lead title={t("sync.name.title")} text={t("sync.name.text")} />
          <NameField value={name} onChange={setName} />
        </>
      )}
      {step === 3 && (
        <>
          <Lead title={t("sync.words.title")} text={t("sync.words.text")} />
          <div className="sync-words">
            <p className="sync-words__list mono" aria-label={t("sync.words.label")}>
              {words}
            </p>
            <button
              type="button"
              className="sync-words__copy"
              onClick={() =>
                void navigator.clipboard?.writeText(words).then(
                  () => setCopied(true),
                  (e: unknown) => toast.error(errorMessage(e, t)),
                )
              }
            >
              {copied ? t("sync.words.copied") : t("sync.words.copy")}
            </button>
          </div>
          <div className="field">
            <label htmlFor="sync-check">{t("sync.words.confirm", { n: CHECK_WORD })}</label>
            <input
              id="sync-check"
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
      {error !== null && step < 3 && (
        <p className="error-text" role="alert">
          {errorMessage(error, t)}
        </p>
      )}
    </Dialog>
  );
}

/** "Ya la uso en otra PC": the same address, this PC's name and the 12 words. */
export function JoinDialog({ onClose }: { onClose: () => void }) {
  const api = useApi();
  const t = useT();
  const toast = useToast();
  const client = useQueryClient();
  const [step, setStep] = useState(1);
  const [url, setUrl] = useState("");
  const [name, setName] = useDefaultName();
  const [words, setWords] = useState("");
  const join = useMutation({
    mutationFn: () => api.syncJoin(url, words, name.trim()),
    onSuccess: (report) => {
      toast.info(t("sync.join.done", { received: report.received }));
      void refreshAfterSync(client);
      onClose();
    },
  });
  const ready = [url.trim(), name.trim(), words.trim()][step - 1] !== "";

  const footer = (
    <>
      <Button
        variant="ghost"
        disabled={join.isPending}
        onClick={() => {
          join.reset();
          if (step === 1) onClose();
          else setStep(step - 1);
        }}
      >
        {step === 1 ? t("common.cancel") : t("common.back")}
      </Button>
      <Button
        variant="primary"
        disabled={!ready || join.isPending}
        onClick={() => (step < 3 ? setStep(step + 1) : join.mutate())}
      >
        {step < 3 ? t("common.continue") : join.isPending ? t("sync.join.joining") : t("sync.join.start")}
      </Button>
    </>
  );

  return (
    <Dialog title={t("sync.join.title")} onClose={onClose} dismissible={!join.isPending} footer={footer}>
      <Steps step={step} total={3} />
      {step === 1 && (
        <>
          <Lead title={t("sync.url.title")} text={t("sync.url.joinText")} />
          <UrlField value={url} onChange={setUrl} />
        </>
      )}
      {step === 2 && (
        <>
          <Lead title={t("sync.name.title")} text={t("sync.name.text")} />
          <NameField value={name} onChange={setName} />
        </>
      )}
      {step === 3 && (
        <>
          <Lead title={t("sync.join.wordsTitle")} text={t("sync.join.wordsText")} />
          <div className="field">
            <label htmlFor="sync-words">{t("sync.words.label")}</label>
            <textarea
              id="sync-words"
              className="mono sync-words__input"
              rows={3}
              value={words}
              autoComplete="off"
              spellCheck={false}
              onChange={(e) => setWords(e.target.value)}
            />
          </div>
        </>
      )}
      {join.isError && (
        <p className="error-text" role="alert">
          {errorMessage(join.error, t)}
        </p>
      )}
    </Dialog>
  );
}

/** "Agregar otra computadora tuya": what to do on the other one, with the address to paste. */
export function AddPcDialog({ url, onClose }: { url: string; onClose: () => void }) {
  const t = useT();
  const toast = useToast();
  return (
    <Dialog
      title={t("sync.addPc")}
      onClose={onClose}
      footer={
        <Button variant="primary" onClick={onClose}>
          {t("common.close")}
        </Button>
      }
    >
      <p className="muted">{t("sync.addPc.text")}</p>
      <ol className="sync-howto">
        <li>{t("sync.addPc.step1")}</li>
        <li>{t("sync.addPc.step2")}</li>
        <li>{t("sync.addPc.step3")}</li>
      </ol>
      <div className="sync-address">
        <span className="mono">{url}</span>
        <Button
          small
          onClick={() =>
            void navigator.clipboard?.writeText(url).then(
              () => toast.info(t("sync.addPc.copied")),
              (e: unknown) => toast.error(errorMessage(e, t)),
            )
          }
        >
          {t("sync.addPc.copy")}
        </Button>
      </div>
    </Dialog>
  );
}

/** "¿Quitar Oficina?": a new key leaves it out; what it has stays there. */
export function RemovePcDialog({ device, onClose }: { device: SyncDevice; onClose: () => void }) {
  const api = useApi();
  const t = useT();
  const toast = useToast();
  const client = useQueryClient();
  const pc = deviceName(device, t);
  const remove = useMutation({
    mutationFn: () => api.syncRemoveDevice(device.id),
    onSuccess: () => {
      toast.info(t("sync.remove.done", { pc }));
      void refreshAfterSync(client);
      onClose();
    },
  });
  return (
    <Dialog
      title={t("sync.remove.title", { pc })}
      onClose={onClose}
      dismissible={!remove.isPending}
      footer={
        <>
          <Button variant="ghost" disabled={remove.isPending} onClick={onClose}>
            {t("common.cancel")}
          </Button>
          <Button variant="danger" disabled={remove.isPending} onClick={() => remove.mutate()}>
            {remove.isPending ? t("sync.remove.removing") : t("sync.remove.confirm")}
          </Button>
        </>
      }
    >
      <p className="muted">{t("sync.remove.text", { pc })}</p>
      {remove.isError && (
        <p className="error-text" role="alert">
          {errorMessage(remove.error, t)}
        </p>
      )}
    </Dialog>
  );
}
