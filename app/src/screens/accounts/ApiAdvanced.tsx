import { useId, useState } from "react";

import { useAction } from "../../api/context";
import type { Account, Overview, SettingKey } from "../../api/types";
import { Field } from "../../components/ui/controls";
import { useT } from "../../i18n";

/** The API providers whose address and price can be set (`api_url.*`, `api_price.*`). */
const APIS = {
  kimi: { name: "Kimi", placeholder: "https://api.moonshot.ai/v1" },
  gemini: { name: "Gemini", placeholder: "https://generativelanguage.googleapis.com/v1beta/openai" },
} as const;

type ApiProvider = keyof typeof APIS;

const isApiProvider = (provider: string): provider is ApiProvider => provider in APIS;

/**
 * "Avanzado" of an API-key account: its provider's address and price, which
 * few people touch, closed by default. They are the same settings as in
 * Ajustes → Avanzado, so they apply to every key of that provider.
 */
export function ApiAdvanced({ account, overview }: { account: Account; overview: Overview }) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const detailId = useId();
  const setSetting = useAction((api, vars: { key: SettingKey; value: string | null }) =>
    api.setSetting(vars.key, vars.value),
  );
  const provider = account.provider;
  if (account.kind !== "apiKey" || !isApiProvider(provider)) return null;
  const { name, placeholder } = APIS[provider];
  const url = overview.apiUrls[provider] ?? "";
  const price = overview.apiPrices[provider] ?? "";

  return (
    <div className="account-advanced">
      <button
        type="button"
        className="disclosure disclosure--quiet"
        aria-expanded={open}
        aria-controls={detailId}
        onClick={() => setOpen((o) => !o)}
      >
        <span className="disclosure__mark" aria-hidden>
          ›
        </span>
        {t("accounts.card.advanced")}
      </button>
      <div id={detailId} className="account-advanced__body" hidden={!open}>
        <p className="usage__note">{t("accounts.card.advancedHint", { provider: name })}</p>
        <Field label={t("settings.apiUrl", { name })} hint={t("settings.apiUrlHint")}>
          {(id) => (
            <input
              id={id}
              className="mono"
              key={url}
              defaultValue={url}
              placeholder={placeholder}
              spellCheck={false}
              onBlur={(e) =>
                e.target.value.trim() !== url && setSetting.mutate({ key: `api_url.${provider}`, value: e.target.value })
              }
            />
          )}
        </Field>
        <Field label={t("settings.price", { name })} hint={t("settings.priceHint")}>
          {(id) => (
            <input
              id={id}
              className="mono"
              key={price}
              defaultValue={price}
              placeholder={t("settings.pricePlaceholder")}
              spellCheck={false}
              onBlur={(e) =>
                e.target.value.trim() !== price &&
                setSetting.mutate({ key: `api_price.${provider}`, value: e.target.value.trim() || null })
              }
            />
          )}
        </Field>
      </div>
    </div>
  );
}
