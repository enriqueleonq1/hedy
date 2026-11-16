import { useEffect, useRef, useState } from "react";
import { useMutation, useQuery } from "@tanstack/react-query";

import { errorMessage, useApi } from "../../api/context";
import type { Overview, ThemeBase } from "../../api/types";
import { useT, type MessageKey } from "../../i18n";
import { accountOption } from "../../components/accounts";
import { Button } from "../../components/ui/Button";
import { Dialog } from "../../components/ui/Dialog";
import { Field, Segmented } from "../../components/ui/controls";
import { Select } from "../../components/ui/Select";
import { asSeed, buildTheme } from "../../theme/derive";
import type { DraftSource, Preview } from "../../theme/ThemeProvider";

type DescribedSource = Extract<DraftSource, { kind: "described" }>;

const IDEAS: MessageKey[] = [
  "theme.describeIdea.oneDark",
  "theme.describeIdea.cafe",
  "theme.describeIdea.contrast",
  "theme.describeIdea.notebook",
];

/** The backend's errors are already in the user's language; a limit is said plainly. */
const LIMIT = /l[íi]mit|rate.?limit|quota|usage limit/i;

/**
 * "Describir con IA": the user says how Hedy should look and one of their own
 * accounts answers with a seed (a short question, a few tokens). The result is
 * previewed across the app; nothing is saved until "Guardar y aplicar". The
 * account is always the one chosen here: never another one on a limit.
 */
export function DescribeThemeDialog({
  overview,
  initial,
  onClose,
  onPreview,
}: {
  overview: Overview;
  initial?: DescribedSource;
  onClose: () => void;
  onPreview: (preview: Preview) => void;
}) {
  const t = useT();
  const api = useApi();
  const offered = useQuery({ queryKey: ["conversationAccounts"], queryFn: () => api.conversationAccounts() });
  const accounts = overview.accounts.filter((a) => offered.data?.includes(a.id) && a.login.state !== "loggedOut");
  const preferred = initial?.account ?? overview.defaultAccount ?? "";
  const [chosen, setChosen] = useState(preferred);
  const account = accounts.some((a) => a.id === chosen) ? chosen : (accounts[0]?.id ?? "");
  const [description, setDescription] = useState(initial?.description ?? "");
  const [base, setBase] = useState<ThemeBase | "auto">(initial?.base ?? "auto");
  const [error, setError] = useState<string | null>(null);
  const open = useRef(true);
  useEffect(() => {
    open.current = true;
    return () => {
      open.current = false;
    };
  }, []);

  const generate = useMutation({
    mutationFn: () => api.describeTheme(account, description.trim(), base === "auto" ? null : base),
    onMutate: () => setError(null),
    onSuccess: (draft) => {
      // Closed while it was generating: nothing to show.
      if (!open.current) return;
      const seed = asSeed(draft.colors);
      if (!seed) return setError(t("theme.describeInvalid"));
      const { tokens, fixes } = buildTheme(seed, draft.base);
      onPreview({
        theme: { id: "draft", name: draft.name.trim() || description.trim(), base: draft.base, tokens },
        fixes,
        draft: {
          origin: "described",
          source: { kind: "described", description, account, base: base === "auto" ? null : base },
        },
      });
      onClose();
    },
    onError: (e) => {
      const message = errorMessage(e, t);
      const name = overview.accounts.find((a) => a.id === account)?.name ?? account;
      setError(LIMIT.test(message) ? t("theme.describeLimit", { account: name }) : message);
    },
  });

  const busy = generate.isPending;
  const canGenerate = Boolean(account) && description.trim() !== "" && !busy;

  return (
    <Dialog
      title={t("theme.describeTitle")}
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>{t("common.cancel")}</Button>
          <Button variant="primary" disabled={!canGenerate} aria-busy={busy || undefined} onClick={() => generate.mutate()}>
            {busy ? t("theme.describeGenerating") : t("theme.describeGenerate")}
          </Button>
        </>
      }
    >
      <Field label={t("theme.describePrompt")}>
        {(id) => (
          <textarea
            id={id}
            className="theme-dialog__describe"
            value={description}
            placeholder={t("theme.describePlaceholder")}
            disabled={busy}
            onChange={(e) => setDescription(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && (e.metaKey || e.ctrlKey) && canGenerate) generate.mutate();
            }}
          />
        )}
      </Field>
      <div className="theme-dialog__ideas" role="group" aria-label={t("theme.describeIdeas")}>
        {IDEAS.map((key) => (
          <button key={key} type="button" className="chip" disabled={busy} onClick={() => setDescription(t(key))}>
            {t(key)}
          </button>
        ))}
      </div>
      <div className="field">
        <span className="field__label">{t("theme.describeBase")}</span>
        <Segmented<ThemeBase | "auto">
          label={t("theme.describeBase")}
          value={base}
          onChange={setBase}
          options={[
            { value: "auto", label: t("theme.describeBase.auto") },
            { value: "dark", label: t("theme.base.dark") },
            { value: "light", label: t("theme.base.light") },
          ]}
        />
      </div>
      <Field label={t("common.account")} hint={t("theme.describeAccountNote")}>
        {(id) => (
          <Select
            id={id}
            title={t("common.account")}
            value={account}
            onChange={setChosen}
            disabled={offered.isPending || busy}
            options={accounts.map((a) => accountOption(a))}
          />
        )}
      </Field>
      {offered.isSuccess && accounts.length === 0 && <p className="warning">{t("theme.describeNoAccounts")}</p>}
      {error && (
        <p className="error-text" role="alert">
          {error}
        </p>
      )}
    </Dialog>
  );
}
