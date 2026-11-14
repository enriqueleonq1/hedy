import { useState } from "react";

import { useAction } from "../../api/context";
import type { Account, NewAccountInput, Overview, ProviderId } from "../../api/types";
import { useTerminals } from "../../components/Terminal/TerminalProvider";
import { Button } from "../../components/ui/Button";
import { Dialog } from "../../components/ui/Dialog";
import { ColorPicker, Field } from "../../components/ui/controls";
import { useT, type MessageKey } from "../../i18n";
import { accountPalette, DEFAULT_ACCOUNT_COLOR } from "../../theme/tokens";

interface Kind {
  id: string;
  provider: ProviderId;
  apiKey: boolean;
  title: string;
  detail: MessageKey;
  defaultFolder?: string;
  /** The CLI keeps one login per machine: only its default folder. */
  onlyDefault?: boolean;
  warning?: MessageKey;
}

const CLAUDE: Kind = {
  id: "claude",
  provider: "claude",
  apiKey: false,
  title: "Claude Code",
  detail: "accounts.kind.claude",
  defaultFolder: "~/.claude",
};

const KINDS: Kind[] = [
  CLAUDE,
  {
    id: "codex",
    provider: "codex",
    apiKey: false,
    title: "Codex",
    detail: "accounts.kind.codex",
    defaultFolder: "~/.codex",
  },
  {
    id: "kimi-code",
    provider: "kimi",
    apiKey: false,
    title: "Kimi Code",
    detail: "accounts.kind.kimiCode",
    defaultFolder: "~/.kimi-code",
  },
  { id: "kimi", provider: "kimi", apiKey: true, title: "Kimi (API)", detail: "accounts.kind.kimiApi" },
  {
    id: "antigravity",
    provider: "antigravity",
    apiKey: false,
    title: "Antigravity",
    detail: "accounts.kind.antigravity",
    onlyDefault: true,
    warning: "accounts.kind.antigravityWarning",
  },
  { id: "gemini-api", provider: "gemini", apiKey: true, title: "Gemini", detail: "accounts.kind.geminiApi" },
  {
    id: "gemini-cli",
    provider: "gemini",
    apiKey: false,
    title: "Gemini CLI",
    detail: "accounts.kind.geminiCli",
    defaultFolder: "~/.gemini",
    warning: "accounts.kind.geminiCliWarning",
  },
];

export function AddAccountDialog({
  overview,
  onClose,
  onNeedsKey,
}: {
  overview: Overview;
  onClose: () => void;
  onNeedsKey: (account: Account) => void;
}) {
  const t = useT();
  const terminals = useTerminals();
  const [kind, setKind] = useState<Kind>(CLAUDE);
  const [name, setName] = useState("");
  const [useDefault, setUseDefault] = useState(false);
  const [color, setColor] = useState(
    accountPalette[overview.accounts.length % accountPalette.length]?.hex ?? DEFAULT_ACCOUNT_COLOR,
  );

  const defaultTaken = overview.accounts.some((a) => a.provider === kind.provider && a.defaultFolder);
  const defaultFolder = !kind.apiKey && (kind.onlyDefault || (useDefault && !defaultTaken));

  const create = useAction((api, input: NewAccountInput) => api.addAccount(input), {
    onSuccess: (id, input) => {
      onClose();
      const created: Account = {
        id,
        name: input.name,
        provider: input.provider,
        providerName: kind.title,
        kind: input.apiKey ? "apiKey" : "subscription",
        defaultFolder: input.defaultFolder,
        color: input.color ?? color,
        login: { state: "loggedOut", plan: null },
        projects: [],
        usage: { weekTokens: 0, weekCostUsd: null, limit: null },
      };
      if (input.apiKey) onNeedsKey(created);
      // An adopted default folder is usually logged in already.
      else if (!input.defaultFolder || kind.onlyDefault)
        terminals.open({ kind: "login", account: id }, `Login · ${input.name}`);
    },
  });

  return (
    <Dialog
      title={t("common.addAccount")}
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>{t("common.cancel")}</Button>
          <Button
            variant="primary"
            disabled={!name.trim() || create.isPending || (kind.onlyDefault && defaultTaken)}
            onClick={() =>
              create.mutate({ name: name.trim(), provider: kind.provider, apiKey: kind.apiKey, defaultFolder, color })
            }
          >
            {kind.apiKey
              ? t("accounts.add.createAndSaveKey")
              : defaultFolder && !kind.onlyDefault
                ? t("accounts.add.create")
                : t("accounts.add.createAndSignIn")}
          </Button>
        </>
      }
    >
      <Field label={t("common.name")}>
        {(id) => (
          <input
            id={id}
            value={name}
            placeholder="Claude personal"
            onChange={(e) => setName(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && name.trim() && e.currentTarget.blur()}
          />
        )}
      </Field>

      <fieldset className="kind-picker">
        <legend className="field__label">{t("common.type")}</legend>
        {KINDS.map((k) => (
          <label key={k.id} className={`kind${k.id === kind.id ? " kind--selected" : ""}`}>
            <input type="radio" name="kind" checked={k.id === kind.id} onChange={() => setKind(k)} />
            <span className="kind__title">{k.title}</span>
            <span className="kind__detail">{t(k.detail)}</span>
          </label>
        ))}
      </fieldset>

      {kind.warning && <p className="warning">{t(kind.warning)}</p>}

      {kind.onlyDefault && defaultTaken && (
        <p className="error-text">{t("accounts.add.alreadyHave", { name: kind.title })}</p>
      )}
      {kind.defaultFolder && (
        <label className="check">
          <input
            type="checkbox"
            checked={defaultFolder}
            disabled={defaultTaken}
            onChange={(e) => setUseDefault(e.target.checked)}
          />
          <span>
            {t("accounts.add.useExisting")} <span className="mono">{kind.defaultFolder}</span>
            {defaultTaken && t("accounts.add.taken")}
          </span>
        </label>
      )}

      <div className="field">
        <span className="field__label">{t("common.color")}</span>
        <ColorPicker label={t("accounts.add.colorLabel")} value={color} onChange={setColor} />
      </div>
    </Dialog>
  );
}
