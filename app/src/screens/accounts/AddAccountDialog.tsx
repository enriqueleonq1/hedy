import { useId, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";

import { useAction, useApi } from "../../api/context";
import type { Account, DetectedCli, NewAccountInput, Overview, ProviderId } from "../../api/types";
import { useTerminals } from "../../components/Terminal/TerminalProvider";
import { Button } from "../../components/ui/Button";
import { Dialog } from "../../components/ui/Dialog";
import { ColorPicker, Field } from "../../components/ui/controls";
import { useT, type MessageKey } from "../../i18n";
import { accountPalette, DEFAULT_ACCOUNT_COLOR } from "../../theme/tokens";
import "../../styles/accounts.css";

interface Kind {
  id: string;
  /** Who makes it, to group the choices: Claude, ChatGPT, Google, Kimi. */
  group: Group;
  provider: ProviderId;
  apiKey: boolean;
  title: string;
  detail: MessageKey;
  defaultFolder?: string;
  /** The CLI keeps one login per machine: only its default folder. */
  onlyDefault?: boolean;
  warning?: MessageKey;
}

type Group = "Claude" | "ChatGPT" | "Google" | "Kimi";
const GROUPS: Group[] = ["Claude", "ChatGPT", "Google", "Kimi"];

const CLAUDE: Kind = {
  id: "claude",
  group: "Claude",
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
    group: "ChatGPT",
    provider: "codex",
    apiKey: false,
    title: "Codex",
    detail: "accounts.kind.codex",
    defaultFolder: "~/.codex",
  },
  {
    id: "kimi-code",
    group: "Kimi",
    provider: "kimi",
    apiKey: false,
    title: "Kimi Code",
    detail: "accounts.kind.kimiCode",
    defaultFolder: "~/.kimi-code",
  },
  { id: "kimi", group: "Kimi", provider: "kimi", apiKey: true, title: "Kimi (API)", detail: "accounts.kind.kimiApi" },
  {
    id: "antigravity",
    group: "Google",
    provider: "antigravity",
    apiKey: false,
    title: "Antigravity",
    detail: "accounts.kind.antigravity",
    onlyDefault: true,
    warning: "accounts.kind.antigravityWarning",
  },
  { id: "gemini-api", group: "Google", provider: "gemini", apiKey: true, title: "Gemini", detail: "accounts.kind.geminiApi" },
  {
    id: "gemini-cli",
    group: "Google",
    provider: "gemini",
    apiKey: false,
    title: "Gemini CLI",
    detail: "accounts.kind.geminiCli",
    defaultFolder: "~/.gemini",
    warning: "accounts.kind.geminiCliWarning",
  },
];

/** The subscription kind whose default folder a detected CLI signs in to. */
const DETECTED_KIND: Record<ProviderId, string> = {
  claude: "claude",
  codex: "codex",
  gemini: "gemini-cli",
  kimi: "kimi-code",
  antigravity: "antigravity",
};

/** How the AI is paid: the first question of the dialog. */
type Pay = "subscription" | "apiKey";

/** The kinds paid that way, in the order of their groups. */
function kindsFor(pay: Pay): Kind[] {
  return GROUPS.flatMap((g) => KINDS.filter((k) => k.group === g && k.apiKey === (pay === "apiKey")));
}

/**
 * Sessions already signed in on this computer that no account uses yet: the
 * CLI's default folder, offered before anything else.
 */
function foundSessions(detected: DetectedCli[] | undefined, overview: Overview): Kind[] {
  return (detected ?? [])
    .filter(
      (d) =>
        d.installed &&
        d.login.state === "loggedIn" &&
        !d.account &&
        !overview.accounts.some((a) => a.provider === d.provider && a.kind === "subscription" && a.defaultFolder),
    )
    .map((d) => KINDS.find((k) => k.id === DETECTED_KIND[d.provider]))
    .filter((k): k is Kind => !!k);
}

/**
 * "Agregar cuenta" in two steps: how the AI is paid (a subscription or an
 * API key), then the provider of that kind, grouped by who makes it, with
 * the name, the CLI's default folder and the color. A session already on
 * this computer is offered first. Subscriptions sign in through the CLI's
 * official login; keys go to the keyring (`ApiKeyDialog`).
 */
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
  const api = useApi();
  const client = useQueryClient();
  const terminals = useTerminals();
  const foundId = useId();
  const payId = useId();
  const detected = useQuery({ queryKey: ["detectClis"], queryFn: () => api.detectClis() });
  const found = foundSessions(detected.data, overview);
  const [pay, setPay] = useState<Pay | null>(null);
  const [kind, setKind] = useState<Kind>(CLAUDE);
  const [name, setName] = useState("");
  const [useDefault, setUseDefault] = useState(false);
  const [color, setColor] = useState(
    accountPalette[overview.accounts.length % accountPalette.length]?.hex ?? DEFAULT_ACCOUNT_COLOR,
  );

  const defaultTaken = overview.accounts.some((a) => a.provider === kind.provider && a.defaultFolder);
  const defaultFolder = !kind.apiKey && (kind.onlyDefault || (useDefault && !defaultTaken));

  const choosePay = (next: Pay) => {
    setPay(next);
    const first = kindsFor(next)[0];
    if (first && first.apiKey !== kind.apiKey) setKind(first);
  };
  const adoptFound = (k: Kind) => {
    setPay("subscription");
    setKind(k);
    setUseDefault(true);
    if (!name.trim()) setName(k.title);
  };

  const create = useAction((api, input: NewAccountInput) => api.addAccount(input), {
    onSuccess: (id, input) => {
      void client.invalidateQueries({ queryKey: ["detectClis"] });
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
        usage: {
          weekTokens: 0,
          weekCostUsd: null,
          limit: null,
          plan: { usage: null, attemptedAt: null, problem: null, supported: false },
        },
      };
      if (input.apiKey) onNeedsKey(created);
      // An adopted default folder is usually logged in already.
      else if (!input.defaultFolder || kind.onlyDefault)
        terminals.open({ kind: "login", account: id }, `Login · ${input.name}`);
    },
  });

  if (pay === null) {
    return (
      <Dialog title={t("common.addAccount")} onClose={onClose} footer={<Button onClick={onClose}>{t("common.cancel")}</Button>}>
        {found.length > 0 && (
          <section className="add-account__section" aria-labelledby={foundId}>
            <h3 id={foundId} className="field__label">
              {t("accounts.add.found")}
            </h3>
            {found.map((k) => (
              <button key={k.id} type="button" className="pay-choice pay-choice--found" onClick={() => adoptFound(k)}>
                <span className="pay-choice__title">{k.title}</span>
                <span className="pay-choice__detail">
                  {k.defaultFolder
                    ? t("accounts.add.foundIn", { folder: k.defaultFolder })
                    : t("accounts.add.foundHere")}
                </span>
              </button>
            ))}
          </section>
        )}
        <section className="add-account__section" aria-labelledby={payId}>
          <h3 id={payId} className="field__label">
            {t("accounts.add.payTitle")}
          </h3>
          <div className="pay-choices">
            <button type="button" className="pay-choice" onClick={() => choosePay("subscription")}>
              <span className="pay-choice__title">{t("accounts.add.paySubscription")}</span>
              <span className="pay-choice__detail">{t("accounts.add.paySubscriptionDetail")}</span>
            </button>
            <button type="button" className="pay-choice" onClick={() => choosePay("apiKey")}>
              <span className="pay-choice__title">{t("accounts.add.payApiKey")}</span>
              <span className="pay-choice__detail">{t("accounts.add.payApiKeyDetail")}</span>
            </button>
          </div>
        </section>
      </Dialog>
    );
  }

  const kinds = kindsFor(pay);
  return (
    <Dialog
      title={t("common.addAccount")}
      onClose={onClose}
      footer={
        <>
          <Button onClick={() => setPay(null)}>{t("common.back")}</Button>
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
      <fieldset className="add-account__groups">
        <legend className="field__label">{t("accounts.add.provider")}</legend>
        {GROUPS.map((group) => {
          const options = kinds.filter((k) => k.group === group);
          if (options.length === 0) return null;
          return (
            <div key={group} className="add-account__group" role="group" aria-label={group}>
              <span className="add-account__group-name" aria-hidden>
                {group}
              </span>
              <div className="kind-picker">
                {options.map((k) => (
                  <label key={k.id} className={`kind${k.id === kind.id ? " kind--selected" : ""}`}>
                    <input type="radio" name="kind" checked={k.id === kind.id} onChange={() => setKind(k)} />
                    <span className="kind__title">{k.title}</span>
                    <span className="kind__detail">{t(k.detail)}</span>
                  </label>
                ))}
              </div>
            </div>
          );
        })}
      </fieldset>

      {kind.warning && <p className="warning">{t(kind.warning)}</p>}

      {kind.onlyDefault && defaultTaken && (
        <p className="error-text">{t("accounts.add.alreadyHave", { name: kind.title })}</p>
      )}

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
