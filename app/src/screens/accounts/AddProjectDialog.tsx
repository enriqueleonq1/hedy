import { useState } from "react";
import { useQuery } from "@tanstack/react-query";

import { errorMessage, useAction, useApi } from "../../api/context";
import type { Overview } from "../../api/types";
import { accountOption } from "../../components/accounts";
import { shortPath } from "../../components/projects";
import { Button } from "../../components/ui/Button";
import { Dialog } from "../../components/ui/Dialog";
import { Field, Switch } from "../../components/ui/controls";
import { Select } from "../../components/ui/Select";
import { useToast } from "../../components/ui/Toast";
import { useT } from "../../i18n";

/** Picks a folder first; the dialog opens only once there is one. */
export function useAddProject() {
  const api = useApi();
  const toast = useToast();
  const t = useT();
  const [path, setPath] = useState<string | null>(null);

  const start = async () => {
    try {
      const folder = await api.pickFolder();
      if (folder) setPath(folder);
    } catch (e) {
      toast.error(errorMessage(e, t));
    }
  };
  return { path, start, close: () => setPath(null) };
}

/** Assigns an account (only the ones the rules allow there) to a new project. */
export function AddProjectDialog({
  overview,
  path,
  onClose,
  onAdded,
}: {
  overview: Overview;
  path: string;
  onClose: () => void;
  /** After the project was registered (not on cancel). */
  onAdded?: (path: string) => void;
}) {
  const api = useApi();
  const t = useT();
  const [chosen, setChosen] = useState("");
  const [protect, setProtect] = useState(false);
  const offered = useQuery({
    queryKey: ["offerable", path],
    queryFn: () => api.offerableAccounts(path),
  });
  const save = useAction(
    (a, vars: { account: string; protect: boolean }) => a.useAccount(path, vars.account, vars.protect),
    {
      onSuccess: () => {
        onAdded?.(path);
        onClose();
      },
    },
  );

  const accounts = overview.accounts.filter((a) => offered.data?.includes(a.id));
  const account = accounts.some((a) => a.id === chosen) ? chosen : (accounts[0]?.id ?? "");

  return (
    <Dialog
      title={t("common.addProject")}
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>{t("common.cancel")}</Button>
          <Button
            variant="primary"
            disabled={!account || save.isPending}
            onClick={() => save.mutate({ account, protect })}
          >
            {t("common.add")}
          </Button>
        </>
      }
    >
      <div className="field">
        <span className="field__label">{t("accounts.project.folder")}</span>
        <span className="mono">{shortPath(path)}</span>
        <p className="hint">{t("accounts.project.folderHint")}</p>
      </div>
      <Field label={t("common.account")}>
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
      {offered.isSuccess && accounts.length === 0 && (
        <p className="warning">{t("accounts.project.noAccounts")}</p>
      )}
      <div className="row row--between">
        <div>
          <span className="field__label">{t("accounts.project.protect")}</span>
          <p className="hint">{t("accounts.project.protectHint")}</p>
        </div>
        <Switch checked={protect} onChange={setProtect} label={t("accounts.project.protectLabel")} />
      </div>
    </Dialog>
  );
}
