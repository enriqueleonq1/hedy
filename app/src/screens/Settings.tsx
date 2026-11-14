import { useState } from "react";

import { useT, type LanguageSetting } from "../i18n";

import { OVERVIEW_KEY, useAction } from "../api/context";
import { useTerminals } from "../components/Terminal/TerminalProvider";
import { Button } from "../components/ui/Button";
import type { CliProviderId, Overview, SettingKey, ThemeSetting } from "../api/types";
import { Mascot, MASCOT_STATE_LABEL, type MascotState } from "../components/Mascot/Mascot";
import { ACTIVITY_LABEL, ACTIVITY_WORDS, WORKING_ACTIVITIES } from "../components/Mascot/activity";
import { ColorPicker, Field, Segmented, Switch } from "../components/ui/controls";
import { DEFAULT_ACCOUNT_COLOR } from "../theme/tokens";
import { TemplatesSection } from "./templates/TemplatesSection";

const CLIS: { id: CliProviderId; name: string }[] = [
  { id: "claude", name: "Claude Code" },
  { id: "codex", name: "Codex" },
  { id: "gemini", name: "Gemini CLI" },
  { id: "kimi", name: "Kimi Code" },
  { id: "antigravity", name: "Antigravity" },
];

export function Settings({ overview }: { overview: Overview }) {
  const setSetting = useAction((api, vars: { key: SettingKey; value: string | null }) =>
    api.setSetting(vars.key, vars.value),
  );
  const t = useT();
  const setColor = useAction((api, vars: { id: string; color: string }) => api.setAccountColor(vars.id, vars.color));
  const [preview, setPreview] = useState(overview.accounts[0]?.color ?? DEFAULT_ACCOUNT_COLOR);

  return (
    <div className="screen settings">
      <h1>{t("common.settings")}</h1>

      <section className="card settings__section">
        <h2>{t("settings.appearance")}</h2>
        <div className="field">
          <span className="field__label">{t("settings.theme")}</span>
          <Segmented<ThemeSetting>
            label={t("settings.theme")}
            value={overview.theme}
            onChange={(value) => setSetting.mutate({ key: "ui.theme", value })}
            options={[
              { value: "system", label: t("settings.theme.system") },
              { value: "light", label: t("settings.theme.light") },
              { value: "dark", label: t("settings.theme.dark") },
            ]}
          />
        </div>
        <div className="field">
          <span className="field__label">{t("common.language")}</span>
          <Segmented<LanguageSetting>
            label={t("common.language")}
            value={overview.language}
            onChange={(value) => setSetting.mutate({ key: "ui.language", value })}
            options={[
              { value: "system", label: t("common.languageSystem") },
              { value: "es", label: t("language.es") },
              { value: "en", label: t("language.en") },
            ]}
          />
        </div>
        <Field label={t("settings.name")} hint={t("settings.nameHint")}>
          {(id) => (
            <input
              id={id}
              key={overview.userName}
              defaultValue={overview.userName}
              onBlur={(e) =>
                e.target.value.trim() !== overview.userName &&
                setSetting.mutate({ key: "ui.name", value: e.target.value })
              }
            />
          )}
        </Field>
      </section>

      <section className="card settings__section">
        <h2>{t("settings.colors")}</h2>
        {overview.accounts.length === 0 && <p className="hint">{t("settings.noAccounts")}</p>}
        {overview.accounts.map((a) => (
          <div key={a.id} className="row row--between settings__color-row">
            <span>{a.name}</span>
            <ColorPicker
              label={t("common.colorOf", { name: a.name })}
              value={a.color}
              onChange={(color) => setColor.mutate({ id: a.id, color })}
            />
          </div>
        ))}
        <div className="mascot-gallery" aria-label={t("settings.mascotStates")}>
          {(Object.keys(MASCOT_STATE_LABEL) as MascotState[]).map((state) => (
            <figure key={state}>
              <Mascot color={preview} state={state} size={110} />
              <figcaption>{t(MASCOT_STATE_LABEL[state])}</figcaption>
            </figure>
          ))}
        </div>
        <h3 className="mascot-gallery__title">{t("settings.mascotWorking")}</h3>
        <div className="mascot-gallery" aria-label={t("settings.mascotWorking")}>
          {WORKING_ACTIVITIES.map((activity) => (
            <figure key={activity}>
              <Mascot color={preview} state="working" activity={activity} size={110} />
              <figcaption>{t(ACTIVITY_LABEL[activity])}</figcaption>
              <span className="mascot-gallery__words">{ACTIVITY_WORDS[activity].map((key) => t(key)).join(" · ")}</span>
            </figure>
          ))}
        </div>
        <div className="field">
          <span className="field__label">{t("settings.tryColor")}</span>
          <ColorPicker label={t("settings.tryColorLabel")} value={preview} onChange={setPreview} />
        </div>
      </section>

      <TemplatesSection overview={overview} />

      <section className="card settings__section">
        <h2>{t("settings.cliPaths")}</h2>
        <p className="hint">{t("settings.cliPathsHint")}</p>
        {CLIS.map((cli) => (
          <Field key={cli.id} label={cli.name}>
            {(id) => (
              <input
                id={id}
                className="mono"
                key={overview.binaries[cli.id] ?? ""}
                defaultValue={overview.binaries[cli.id] ?? ""}
                placeholder={cli.id}
                spellCheck={false}
                onBlur={(e) =>
                  e.target.value.trim() !== (overview.binaries[cli.id] ?? "") &&
                  setSetting.mutate({ key: `binary.${cli.id}`, value: e.target.value })
                }
              />
            )}
          </Field>
        ))}
      </section>

      <section className="card settings__section">
        <h2>APIs</h2>
        <p className="hint">{t("settings.apiUrlHint")}</p>
        {(
          [
            { id: "kimi", name: "Kimi", placeholder: "https://api.moonshot.ai/v1" },
            { id: "gemini", name: "Gemini", placeholder: "https://generativelanguage.googleapis.com/v1beta/openai" },
          ] as const
        ).map((api) => (
          <Field key={api.id} label={t("settings.apiUrl", { name: api.name })}>
            {(id) => (
              <input
                id={id}
                className="mono"
                key={overview.apiUrls[api.id] ?? ""}
                defaultValue={overview.apiUrls[api.id] ?? ""}
                placeholder={api.placeholder}
                spellCheck={false}
                onBlur={(e) =>
                  e.target.value.trim() !== (overview.apiUrls[api.id] ?? "") &&
                  setSetting.mutate({ key: `api_url.${api.id}`, value: e.target.value })
                }
              />
            )}
          </Field>
        ))}
        <p className="hint">{t("settings.priceHint")}</p>
        {(["kimi", "gemini"] as const).map((provider) => (
          <Field key={provider} label={t("settings.price", { name: provider === "kimi" ? "Kimi" : "Gemini" })}>
            {(id) => (
              <input
                id={id}
                className="mono"
                key={overview.apiPrices[provider] ?? ""}
                defaultValue={overview.apiPrices[provider] ?? ""}
                placeholder={t("settings.pricePlaceholder")}
                spellCheck={false}
                onBlur={(e) =>
                  e.target.value.trim() !== (overview.apiPrices[provider] ?? "") &&
                  setSetting.mutate({ key: `api_price.${provider}`, value: e.target.value.trim() || null })
                }
              />
            )}
          </Field>
        ))}
      </section>

      <section className="card settings__section">
        <h2>{t("settings.notifications")}</h2>
        <div className="row row--between">
          <span>{t("settings.notifications.toggle")}</span>
          <Switch
            label={t("settings.notifications.toggle")}
            checked={overview.notifications}
            disabled={setSetting.isPending}
            onChange={(on) => setSetting.mutate({ key: "ui.notifications", value: on ? "on" : "off" })}
          />
        </div>
        <p className="small muted">{t("settings.notifications.hint")}</p>
      </section>

      <CodeMap overview={overview} />

      <section className="card settings__section">
        <h2>{t("settings.data")}</h2>
        <p>
          {t("settings.dataDir")} <span className="mono">{overview.dataDir}</span>
        </p>
        <p className="hint">
          {t("settings.dataHintBefore")} <span className="mono">HEDY_DATA_DIR</span>
          {t("settings.dataHintAfter")}
        </p>
        <div>
          <Button onClick={() => setSetting.mutate({ key: "ui.onboarded", value: null })}>
            {t("onboarding.showAgain")}
          </Button>
        </div>
      </section>
    </div>
  );
}

/**
 * The code map (codebase-memory-mcp): a local index of the code that every AI
 * can query. Hedy installs it with its official script and registers it per
 * run; the index stays on this PC.
 */
function CodeMap({ overview }: { overview: Overview }) {
  const t = useT();
  const terminals = useTerminals();
  // Every project status says whether the code map is on.
  const setSetting = useAction((api, value: string | null) => api.setSetting("code_map.binary", value), {
    invalidate: [OVERVIEW_KEY, ["status"]],
  });
  const { enabled, found } = overview.codeMap;
  return (
    <section className="card settings__section">
      <h2>{t("common.codeMap")}</h2>
      <p className="hint">
        {t("settings.codeMap.hintBefore")} <span className="mono">codebase-memory-mcp</span>
        {t("settings.codeMap.hintAfter")}
      </p>
      {found ? (
        <div className="row row--between">
          <span>
            {t("settings.codeMap.installedAt")} <span className="mono">{found}</span>
          </span>
          <Switch
            label={t("settings.codeMap.register")}
            checked={enabled !== null}
            disabled={setSetting.isPending}
            onChange={(on) => setSetting.mutate(on ? found : null)}
          />
        </div>
      ) : (
        <div className="row row--between">
          <span className="muted">
            {enabled ? t("settings.codeMap.missing", { path: enabled }) : t("settings.codeMap.notInstalled")}
          </span>
          <Button onClick={() => terminals.open({ kind: "installCodeMap" }, t("settings.codeMap.installTitle"))}>
            {t("settings.codeMap.install")}
          </Button>
        </div>
      )}
      <p className="small muted">{t("settings.codeMap.note")}</p>
    </section>
  );
}
