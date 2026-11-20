import { useEffect, useId, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";

import { useT, type LanguageSetting, type MessageKey } from "../i18n";
import type { SettingsSection } from "../components/Layout";

import { OVERVIEW_KEY, useAction, useApi } from "../api/context";
import { useTerminals } from "../components/Terminal/TerminalProvider";
import { Button } from "../components/ui/Button";
import type { CheckLevel, CliProviderId, Overview, SettingKey, ThemeSetting } from "../api/types";
import { Mascot, MASCOT_STATE_LABEL, type MascotState } from "../components/Mascot/Mascot";
import { ACTIVITY_LABEL, ACTIVITY_WORDS, WORKING_ACTIVITIES } from "../components/Mascot/activity";
import { ColorPicker, Field, Segmented, Switch } from "../components/ui/controls";
import { DEFAULT_ACCOUNT_COLOR } from "../theme/tokens";
import { AccountsScreen } from "./accounts/AccountsScreen";
import { SyncSection } from "./sync/SyncSection";
import { TeamsSection } from "./team/TeamsSection";
import { TemplatesSection } from "./templates/TemplatesSection";
import { ThemeGallery } from "./themes/ThemeGallery";

const CLIS: { id: CliProviderId; name: string }[] = [
  { id: "claude", name: "Claude Code" },
  { id: "codex", name: "Codex" },
  { id: "gemini", name: "Gemini CLI" },
  { id: "kimi", name: "Kimi Code" },
  { id: "antigravity", name: "Antigravity" },
];


const INDEX: {
  group: MessageKey | null;
  items: { id: SettingsSection; label: MessageKey; words: MessageKey[] }[];
}[] = [
  {
    group: null,
    items: [
      {
        id: "general",
        label: "settings.section.general",
        words: ["common.language", "settings.name", "settings.notifications", "settings.keywords.general"],
      },
      { id: "accounts", label: "settings.section.accounts", words: ["accounts.title", "common.projectsAndAccounts", "settings.keywords.accounts"] },
      {
        id: "appearance",
        label: "settings.appearance",
        words: ["theme.mode", "theme.gallery", "settings.colors", "settings.mascotStates", "settings.keywords.appearance"],
      },
      { id: "saving", label: "settings.saving", words: ["common.codeMap", "settings.keywords.saving"] },
      { id: "templates", label: "settings.section.templates", words: ["settings.keywords.templates"] },
    ],
  },
  {
    group: "settings.group.computers",
    items: [
      { id: "sync", label: "settings.section.sync", words: ["settings.keywords.sync"] },
      { id: "teams", label: "settings.section.teams", words: ["settings.keywords.teams"] },
    ],
  },
  {
    group: "settings.group.more",
    items: [
      {
        id: "advanced",
        label: "settings.section.advanced",
        words: ["settings.cliPaths", "settings.doctor", "settings.data", "settings.keywords.advanced"],
      },
    ],
  },
];

/** Lowercase and without accents, so "diagnostico" finds "Diagnóstico". */
export function foldText(text: string): string {
  return text.normalize("NFD").replace(/\p{M}/gu, "").toLowerCase();
}

/**
 * Ajustes: an index with a search on the side and one section at a time
 * (`#settings/<section>`), instead of one page of everything. Arriving from
 * another screen (the palette, Inicio, a link) puts focus on the section's
 * title; moving through the index leaves it in the index.
 */
export function Settings({
  overview,
  section = "general",
  account,
  gallery = false,
  onSection,
}: {
  overview: Overview;
  section?: SettingsSection;
  /** In Cuentas: that account's card. */
  account?: string;
  /** In Apariencia: straight to the theme gallery ("Cambiar tema…"). */
  gallery?: boolean;
  onSection: (section: SettingsSection) => void;
}) {
  const t = useT();
  const [query, setQuery] = useState("");
  const title = useRef<HTMLHeadingElement>(null);
  const needle = foldText(query.trim());
  const matches = (group: MessageKey | null, item: (typeof INDEX)[number]["items"][number]) =>
    !needle || [group, item.label, ...item.words].some((key) => key !== null && foldText(t(key)).includes(needle));
  const current = INDEX.flatMap((g) => g.items).find((i) => i.id === section);

  // Only on arrival: the index keeps focus while moving through it. Cuentas
  // draws its own title, and the gallery focuses its own.
  const arrival = useRef({ section, gallery, account });
  useEffect(() => {
    const { section: first, gallery: toGallery, account: card } = arrival.current;
    if (first === "general" || first === "accounts" || toGallery || card) return;
    title.current?.focus({ preventScroll: true });
  }, []);

  return (
    <div className="screen settings settings--indexed">
      <nav className="settings__index" aria-label={t("settings.index")}>
        <input
          type="search"
          className="settings__search"
          aria-label={t("settings.search")}
          placeholder={t("settings.search")}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
        {INDEX.map(({ group, items }) => {
          const shown = items.filter((item) => matches(group, item));
          if (shown.length === 0) return null;
          return (
            <div key={group ?? "main"} className="settings__index-group">
              {group && <span className="settings__index-title">{t(group)}</span>}
              {shown.map((item) => (
                <button
                  key={item.id}
                  type="button"
                  className="settings__index-item"
                  aria-current={item.id === section ? "page" : undefined}
                  onClick={() => onSection(item.id)}
                >
                  {t(item.label)}
                </button>
              ))}
            </div>
          );
        })}
        {needle && !INDEX.some((g) => g.items.some((item) => matches(g.group, item))) && (
          <p className="small muted">{t("settings.noMatch")}</p>
        )}
      </nav>
      <div className="settings__content">
        {section !== "accounts" && current && (
          <h1 ref={title} tabIndex={-1}>
            {t(current.label)}
          </h1>
        )}
        {section === "general" && <GeneralSection overview={overview} />}
        {section === "accounts" && <AccountsScreen overview={overview} focus={account} />}
        {section === "appearance" && <AppearanceSection overview={overview} gallery={gallery} />}
        {section === "saving" && (
          <>
            <ContextSaving overview={overview} />
            <CodeMap overview={overview} />
          </>
        )}
        {section === "templates" && <TemplatesSection overview={overview} />}
        {section === "sync" && <SyncSection />}
        {section === "teams" && <TeamsSection />}
        {section === "advanced" && <AdvancedSection overview={overview} onAccounts={() => onSection("accounts")} />}
      </div>
    </div>
  );
}

function useSetSetting() {
  return useAction((api, vars: { key: SettingKey; value: string | null }) => api.setSetting(vars.key, vars.value));
}

/** Language, the name Hedy calls the user, notifications and the guide. */
function GeneralSection({ overview }: { overview: Overview }) {
  const t = useT();
  const setSetting = useSetSetting();
  return (
    <section className="card settings__section">
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
              e.target.value.trim() !== overview.userName && setSetting.mutate({ key: "ui.name", value: e.target.value })
            }
          />
        )}
      </Field>
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
      <div>
        <Button onClick={() => setSetting.mutate({ key: "ui.onboarded", value: null })}>{t("onboarding.showAgain")}</Button>
      </div>
    </section>
  );
}

/** Light or dark and the themes in one place, each account's color, and the mascot one click away. */
function AppearanceSection({ overview, gallery }: { overview: Overview; gallery: boolean }) {
  const t = useT();
  const setSetting = useSetSetting();
  const setColor = useAction((api, vars: { id: string; color: string }) => api.setAccountColor(vars.id, vars.color));
  const [preview, setPreview] = useState(overview.accounts[0]?.color ?? DEFAULT_ACCOUNT_COLOR);
  const [mascot, setMascot] = useState(false);

  // "Cambiar tema…" in the palette: straight to the gallery.
  useEffect(() => {
    if (!gallery) return;
    const list = document.getElementById("theme-gallery");
    list?.scrollIntoView?.({ block: "start" });
    list?.querySelector<HTMLElement>("h2")?.focus({ preventScroll: true });
  }, [gallery]);

  return (
    <>
      <section className="card settings__section">
        <div className="field">
          <span className="field__label">{t("theme.mode")}</span>
          <Segmented<ThemeSetting>
            label={t("theme.mode")}
            value={overview.theme}
            onChange={(value) => setSetting.mutate({ key: "ui.theme", value })}
            options={[
              { value: "system", label: t("settings.theme.system") },
              { value: "light", label: t("settings.theme.light") },
              { value: "dark", label: t("settings.theme.dark") },
            ]}
          />
        </div>
        <ThemeGallery overview={overview} />
      </section>

      <section className="card settings__section">
        <h2>{t("settings.colors")}</h2>
        {overview.accounts.length === 0 && <p className="hint">{t("settings.noAccounts")}</p>}
        {overview.accounts.map((a) => (
          <div key={a.id} className="row row--between settings__color-row">
            <span>{a.name}</span>
            <ColorPicker label={t("common.colorOf", { name: a.name })} value={a.color} onChange={(color) => setColor.mutate({ id: a.id, color })} />
          </div>
        ))}
        <div>
          <Button variant="ghost" aria-expanded={mascot} onClick={() => setMascot((m) => !m)}>
            {mascot ? t("settings.mascotHide") : t("settings.mascotShow")}
          </Button>
        </div>
        {mascot && (
          <>
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
          </>
        )}
      </section>
    </>
  );
}

/**
 * What few people touch: the CLIs' paths, the diagnosis, where the APIs'
 * addresses and prices live (each API account's "Avanzado"), and the data folder.
 */
function AdvancedSection({ overview, onAccounts }: { overview: Overview; onAccounts: () => void }) {
  const t = useT();
  const setSetting = useSetSetting();
  return (
    <>
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

      <Diagnosis />

      <section className="card settings__section">
        <h2>APIs</h2>
        <p className="hint">
          {t("settings.apiPointer")}{" "}
          <button type="button" className="link-button" onClick={onAccounts}>
            {t("settings.apiPointerLink")}
          </button>
        </p>
      </section>

      <section className="card settings__section">
        <h2>{t("settings.data")}</h2>
        <p>
          {t("settings.dataDir")} <span className="mono">{overview.dataDir}</span>
        </p>
        <p className="hint">
          {t("settings.dataHintBefore")} <span className="mono">HEDY_DATA_DIR</span>
          {t("settings.dataHintAfter")}
        </p>
      </section>
    </>
  );
}

const LEVEL_LABEL: Record<CheckLevel, MessageKey> = {
  ok: "settings.doctor.level.ok",
  info: "settings.doctor.level.info",
  warning: "settings.doctor.level.warning",
  error: "settings.doctor.level.error",
};

/**
 * The CLIs `hedy doctor` found and their versions (a version that changed
 * since Hedy was checked with it shows here, not on the home screen), plus
 * any other warning or problem it found.
 */
function Diagnosis() {
  const api = useApi();
  const t = useT();
  const doctor = useQuery({ queryKey: ["doctor"], queryFn: () => api.doctor(), staleTime: 10 * 60_000 }).data;
  const checks = doctor?.checks.filter((c) => c.area === "cli" || c.level === "warning" || c.level === "error");
  return (
    <section className="card settings__section">
      <h2>{t("settings.doctor")}</h2>
      <p className="hint">
        {t("settings.doctor.hintBefore")} <span className="mono">hedy doctor</span>
        {t("settings.doctor.hintAfter")}
      </p>
      {checks ? (
        <ul className="diagnosis" aria-label={t("settings.doctor.list")}>
          {checks.map((c) => (
            <li key={`${c.area}-${c.subject}`} className="diagnosis__row">
              <span className={`diagnosis__dot diagnosis__dot--${c.level}`} role="img" aria-label={t(LEVEL_LABEL[c.level])} />
              <span className="diagnosis__subject">{c.subject}</span>
              <span className={`diagnosis__message${c.level === "ok" || c.level === "info" ? " muted" : ""}`}>
                {c.message}
                {c.fix && <span className="mono diagnosis__fix">{c.fix}</span>}
              </span>
            </li>
          ))}
        </ul>
      ) : (
        <p className="small muted">{t("settings.doctor.checking")}</p>
      )}
    </section>
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

/** The context-saving package (docs/decisions.md §33): one switch, one
 * sentence, and the technical detail one click away. */
function ContextSaving({ overview }: { overview: Overview }) {
  const t = useT();
  const setSetting = useAction((api, vars: { key: SettingKey; value: string | null }) =>
    api.setSetting(vars.key, vars.value),
  );
  const [open, setOpen] = useState(false);
  const detailId = useId();
  const points: MessageKey[] = [
    "settings.saving.how.briefing",
    "settings.saving.how.trim",
    "settings.saving.how.search",
    "settings.saving.how.claude",
    "settings.saving.how.others",
  ];
  return (
    <section className="card settings__section">
      <div className="row row--between">
        <span>{t("settings.saving.toggle")}</span>
        <Switch
          label={t("settings.saving.toggle")}
          checked={overview.contextSaving}
          disabled={setSetting.isPending || overview.contextSavingByEnv}
          onChange={(on) => setSetting.mutate({ key: "context_saving", value: on ? "on" : "off" })}
        />
      </div>
      <p className="small muted">{t("settings.saving.hint")}</p>
      {overview.contextSavingByEnv && <p className="small warning">{t("settings.saving.byEnv")}</p>}
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
        {t("settings.saving.how")}
      </button>
      <ul id={detailId} className="settings__how small muted" hidden={!open}>
        {points.map((key) => (
          <li key={key}>{t(key)}</li>
        ))}
      </ul>
    </section>
  );
}
