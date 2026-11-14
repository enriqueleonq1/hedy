import { useState } from "react";

import { useAction } from "../../api/context";
import type { Account, NewAccountInput, Overview, ProviderId } from "../../api/types";
import { useTerminals } from "../../components/Terminal/TerminalProvider";
import { Button } from "../../components/ui/Button";
import { Dialog } from "../../components/ui/Dialog";
import { ColorPicker, Field } from "../../components/ui/controls";
import { accountPalette, DEFAULT_ACCOUNT_COLOR } from "../../theme/tokens";

interface Kind {
  id: string;
  provider: ProviderId;
  apiKey: boolean;
  title: string;
  detail: string;
  defaultFolder?: string;
  /** The CLI keeps one login per machine: only its default folder. */
  onlyDefault?: boolean;
  warning?: string;
}

const CLAUDE: Kind = {
  id: "claude",
  provider: "claude",
  apiKey: false,
  title: "Claude Code",
  detail: "Suscripción Pro, Max o Team",
  defaultFolder: "~/.claude",
};

const KINDS: Kind[] = [
  CLAUDE,
  {
    id: "codex",
    provider: "codex",
    apiKey: false,
    title: "Codex",
    detail: "Suscripción de ChatGPT",
    defaultFolder: "~/.codex",
  },
  {
    id: "kimi-code",
    provider: "kimi",
    apiKey: false,
    title: "Kimi Code",
    detail: "Suscripción de Kimi Code",
    defaultFolder: "~/.kimi-code",
  },
  { id: "kimi", provider: "kimi", apiKey: true, title: "Kimi (API)", detail: "API key de pago por uso" },
  {
    id: "antigravity",
    provider: "antigravity",
    apiKey: false,
    title: "Antigravity",
    detail: "Tu cuenta personal de Google",
    onlyDefault: true,
    warning:
      "Antigravity guarda la sesión en el llavero del sistema y no deja elegir carpeta: solo admite una cuenta por equipo.",
  },
  { id: "gemini-api", provider: "gemini", apiKey: true, title: "Gemini", detail: "API key de Google AI Studio" },
  {
    id: "gemini-cli",
    provider: "gemini",
    apiKey: false,
    title: "Gemini CLI",
    detail: "Code Assist Standard o Enterprise",
    defaultFolder: "~/.gemini",
    warning:
      "Gemini CLI ya no acepta cuentas personales de Google. Para una cuenta personal usa Antigravity o Gemini con API key.",
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
      title="Agregar cuenta"
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>Cancelar</Button>
          <Button
            variant="primary"
            disabled={!name.trim() || create.isPending || (kind.onlyDefault && defaultTaken)}
            onClick={() =>
              create.mutate({ name: name.trim(), provider: kind.provider, apiKey: kind.apiKey, defaultFolder, color })
            }
          >
            {kind.apiKey ? "Crear y guardar key" : defaultFolder && !kind.onlyDefault ? "Crear" : "Crear e iniciar sesión"}
          </Button>
        </>
      }
    >
      <Field label="Nombre">
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
        <legend className="field__label">Tipo</legend>
        {KINDS.map((k) => (
          <label key={k.id} className={`kind${k.id === kind.id ? " kind--selected" : ""}`}>
            <input type="radio" name="kind" checked={k.id === kind.id} onChange={() => setKind(k)} />
            <span className="kind__title">{k.title}</span>
            <span className="kind__detail">{k.detail}</span>
          </label>
        ))}
      </fieldset>

      {kind.warning && <p className="warning">{kind.warning}</p>}

      {kind.onlyDefault && defaultTaken && <p className="error-text">Ya tienes una cuenta de {kind.title} en este equipo.</p>}
      {kind.defaultFolder && (
        <label className="check">
          <input
            type="checkbox"
            checked={defaultFolder}
            disabled={defaultTaken}
            onChange={(e) => setUseDefault(e.target.checked)}
          />
          <span>
            Usar la sesión que ya existe en <span className="mono">{kind.defaultFolder}</span>
            {defaultTaken && " (ya la usa otra cuenta)"}
          </span>
        </label>
      )}

      <div className="field">
        <span className="field__label">Color</span>
        <ColorPicker label="Color de la cuenta" value={color} onChange={setColor} />
      </div>
    </Dialog>
  );
}
