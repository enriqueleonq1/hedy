import { useEffect, useId, useState } from "react";
import { useQuery } from "@tanstack/react-query";

import { errorMessage, OVERVIEW_KEY, useAction, useApi } from "../../api/context";
import type { NewProjectInput, Overview } from "../../api/types";
import { accountOption } from "../../components/accounts";
import { shortPath } from "../../components/projects";
import { Button } from "../../components/ui/Button";
import { Field, Switch } from "../../components/ui/controls";
import { Dialog } from "../../components/ui/Dialog";
import { Select } from "../../components/ui/Select";
import { useToast } from "../../components/ui/Toast";
import { useT } from "../../i18n";

/** `value`, once it stopped changing for `ms`. */
function useSettled<T>(value: T, ms: number): T {
  const [settled, setSettled] = useState(value);
  useEffect(() => {
    const timer = setTimeout(() => setSettled(value), ms);
    return () => clearTimeout(timer);
  }, [value, ms]);
  return settled;
}

/**
 * "Nuevo proyecto": only a name and where (with a sensible default) to start;
 * the account (preselected), protection and client wait under "Más opciones".
 * The final folder is shown before anything is created.
 */
export function NewProjectDialog({
  overview,
  initialName,
  onClose,
  onCreated,
}: {
  overview: Overview;
  /** What the name starts with (a name suggested from a conversation). */
  initialName?: string;
  onClose: () => void;
  /** After the project exists (not on cancel). */
  onCreated: (path: string, name: string) => void;
}) {
  const api = useApi();
  const t = useT();
  const toast = useToast();
  const moreId = useId();
  const previewId = useId();
  const [name, setName] = useState(initialName ?? "");
  const [picked, setPicked] = useState<string | null>(null);
  const [more, setMore] = useState(false);
  const [chosen, setChosen] = useState("");
  const [protect, setProtect] = useState(false);
  const [client, setClient] = useState("");

  const defaults = useQuery({ queryKey: ["defaultProjectParent"], queryFn: () => api.defaultProjectParent() });
  const parent = picked ?? defaults.data ?? "";
  const typed = useSettled(name.trim(), 150);
  const preview = useQuery({
    queryKey: ["newProjectPath", typed, parent],
    queryFn: () => api.newProjectPath(typed, parent),
    enabled: typed !== "" && parent !== "",
    retry: false,
  });
  // A new folder is outside every project: the accounts free of protected projects.
  const offered = useQuery({ queryKey: ["conversationAccounts"], queryFn: () => api.conversationAccounts() });
  const accounts = overview.accounts.filter((a) => offered.data?.includes(a.id));
  const account =
    accounts.find((a) => a.id === chosen)?.id ??
    accounts.find((a) => a.id === overview.defaultAccount)?.id ??
    accounts[0]?.id ??
    "";

  const create = useAction((a, input: NewProjectInput) => a.newProject(input), {
    invalidate: [OVERVIEW_KEY, ["defaultProjectParent"]],
    onSuccess: (path) => onCreated(path, name.trim()),
  });

  const change = async () => {
    try {
      const folder = await api.pickFolder();
      if (folder) setPicked(folder);
    } catch (e) {
      toast.error(errorMessage(e, t));
    }
  };

  const current = typed === name.trim();
  const ready = current && preview.isSuccess && !create.isPending;
  const submit = () => {
    if (!ready) return;
    create.mutate({
      name: name.trim(),
      parent,
      account: account || null,
      protect,
      client: client.trim() || null,
    });
  };

  return (
    <Dialog
      title={t("newProject.title")}
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>{t("common.cancel")}</Button>
          <Button variant="primary" disabled={!ready} onClick={submit}>
            {create.isPending ? t("newProject.creating") : t("newProject.create")}
          </Button>
        </>
      }
    >
      <form
        className="new-project"
        onSubmit={(e) => {
          e.preventDefault();
          submit();
        }}
      >
        <Field label={t("newProject.name")}>
          {(id) => (
            <input
              id={id}
              value={name}
              autoComplete="off"
              spellCheck={false}
              placeholder={t("newProject.namePlaceholder")}
              aria-describedby={previewId}
              aria-invalid={preview.isError && current ? true : undefined}
              onChange={(e) => setName(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  e.preventDefault();
                  submit();
                }
              }}
            />
          )}
        </Field>
        <div className="field">
          <span className="field__label">{t("newProject.where")}</span>
          <div className="new-project__where">
            <span className="mono new-project__parent" title={parent}>
              {parent ? shortPath(parent) : "…"}
            </span>
            <Button small variant="ghost" aria-label={t("newProject.changeLabel")} onClick={() => void change()}>
              {t("newProject.change")}
            </Button>
          </div>
        </div>
        <div id={previewId} className="new-project__preview" aria-live="polite">
          {preview.isError && current ? (
            <p className="error-text">{errorMessage(preview.error, t)}</p>
          ) : preview.data && current ? (
            <p>
              {t("newProject.preview")} <span className="mono">{shortPath(preview.data)}</span>
            </p>
          ) : null}
          <p className="hint">{t("newProject.previewHint")}</p>
        </div>

        <button
          type="button"
          className="disclosure"
          aria-expanded={more}
          aria-controls={moreId}
          onClick={() => setMore((m) => !m)}
        >
          <span className="disclosure__mark" aria-hidden>
            ›
          </span>
          {t("newProject.more")}
          {!more && account && (
            <span className="muted small"> · {overview.accounts.find((a) => a.id === account)?.name}</span>
          )}
        </button>
        <div id={moreId} className="new-project__more" hidden={!more}>
          <Field label={t("common.account")} hint={t("newProject.accountHint")}>
            {(id) => (
              <Select
                id={id}
                title={t("common.account")}
                value={account}
                onChange={setChosen}
                disabled={offered.isPending}
                options={accounts.map((a) => accountOption(a))}
              />
            )}
          </Field>
          {offered.isSuccess && accounts.length === 0 && <p className="warning">{t("accounts.project.noAccounts")}</p>}
          <div className="row row--between">
            <div>
              <span className="field__label">{t("accounts.project.protect")}</span>
              <p className="hint">{t("accounts.project.protectHint")}</p>
            </div>
            <Switch checked={protect} onChange={setProtect} label={t("accounts.project.protectLabel")} />
          </div>
          <Field label={t("common.client")}>
            {(id) => (
              <input
                id={id}
                value={client}
                placeholder={t("newProject.clientPlaceholder")}
                onChange={(e) => setClient(e.target.value)}
              />
            )}
          </Field>
        </div>
      </form>
    </Dialog>
  );
}
