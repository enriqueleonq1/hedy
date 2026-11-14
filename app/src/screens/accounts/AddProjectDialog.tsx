import { useState } from "react";
import { useQuery } from "@tanstack/react-query";

import { errorMessage, useAction, useApi } from "../../api/context";
import type { Overview } from "../../api/types";
import { shortPath } from "../../components/projects";
import { Button } from "../../components/ui/Button";
import { Dialog } from "../../components/ui/Dialog";
import { Field, Switch } from "../../components/ui/controls";
import { useToast } from "../../components/ui/Toast";

/** Picks a folder first; the dialog opens only once there is one. */
export function useAddProject() {
  const api = useApi();
  const toast = useToast();
  const [path, setPath] = useState<string | null>(null);

  const start = async () => {
    try {
      const folder = await api.pickFolder();
      if (folder) setPath(folder);
    } catch (e) {
      toast.error(errorMessage(e));
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
      title="Agregar proyecto"
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>Cancelar</Button>
          <Button
            variant="primary"
            disabled={!account || save.isPending}
            onClick={() => save.mutate({ account, protect })}
          >
            Agregar
          </Button>
        </>
      }
    >
      <div className="field">
        <span className="field__label">Carpeta</span>
        <span className="mono">{shortPath(path)}</span>
        <p className="hint">Si está dentro de un repositorio git, se usa la raíz del repositorio.</p>
      </div>
      <Field label="Cuenta">
        {(id) => (
          <select id={id} value={account} onChange={(e) => setChosen(e.target.value)} disabled={offered.isPending}>
            {accounts.map((a) => (
              <option key={a.id} value={a.id}>
                {a.name} · {a.providerName}
              </option>
            ))}
          </select>
        )}
      </Field>
      {offered.isSuccess && accounts.length === 0 && (
        <p className="warning">No hay cuentas disponibles aquí: todas están reservadas por proyectos protegidos.</p>
      )}
      <div className="row row--between">
        <div>
          <span className="field__label">Proteger</span>
          <p className="hint">Solo esta cuenta en este proyecto, y no se ofrece en los demás.</p>
        </div>
        <Switch checked={protect} onChange={setProtect} label="Proteger proyecto" />
      </div>
    </Dialog>
  );
}
