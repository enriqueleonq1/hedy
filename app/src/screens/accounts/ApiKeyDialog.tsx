import { useState } from "react";

import { useAction } from "../../api/context";
import type { Account } from "../../api/types";
import { Button } from "../../components/ui/Button";
import { Dialog } from "../../components/ui/Dialog";
import { Field } from "../../components/ui/controls";
import { useT } from "../../i18n";

/** The key goes straight to the OS keyring; it is never shown again. */
export function ApiKeyDialog({ account, onClose }: { account: Account; onClose: () => void }) {
  const t = useT();
  const [key, setKey] = useState("");
  const save = useAction((api, value: string) => api.setApiKey(account.id, value), { onSuccess: onClose });

  return (
    <Dialog
      title={t("accounts.key.title", { name: account.name })}
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>{t("common.cancel")}</Button>
          <Button variant="primary" disabled={!key.trim() || save.isPending} onClick={() => save.mutate(key)}>
            {t("accounts.key.save")}
          </Button>
        </>
      }
    >
      <Field label="API key" hint={t("accounts.key.hint")}>
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
