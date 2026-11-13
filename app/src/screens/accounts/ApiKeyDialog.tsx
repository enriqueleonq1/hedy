import { useState } from "react";

import { useAction } from "../../api/context";
import type { Account } from "../../api/types";
import { Button } from "../../components/ui/Button";
import { Dialog } from "../../components/ui/Dialog";
import { Field } from "../../components/ui/controls";

/** The key goes straight to the OS keyring; it is never shown again. */
export function ApiKeyDialog({ account, onClose }: { account: Account; onClose: () => void }) {
  const [key, setKey] = useState("");
  const save = useAction((api, value: string) => api.setApiKey(account.id, value), { onSuccess: onClose });

  return (
    <Dialog
      title={`API key de ${account.name}`}
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>Cancelar</Button>
          <Button variant="primary" disabled={!key.trim() || save.isPending} onClick={() => save.mutate(key)}>
            Guardar en el llavero
          </Button>
        </>
      }
    >
      <Field label="API key" hint="Se guarda en el llavero del sistema, nunca en archivos.">
        {(id) => (
          <input
            id={id}
            type="password"
            autoComplete="off"
            value={key}
            onChange={(e) => setKey(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && key.trim() && save.mutate(key)}
          />
        )}
      </Field>
    </Dialog>
  );
}
