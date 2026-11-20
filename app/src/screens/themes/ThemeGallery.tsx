import type { MouseEvent } from "react";

import type { CustomTheme, Overview } from "../../api/types";
import { useT, type MessageKey } from "../../i18n";
import { copyText } from "../../code/copies";
import { Button } from "../../components/ui/Button";
import { useContextMenu } from "../../components/ui/ContextMenu";
import type { MenuItem } from "../../components/ui/Menu";
import { MenuButton } from "../../components/ui/Menu";
import { useToast } from "../../components/ui/Toast";
import { PRESETS, type ThemeDef } from "../../theme/presets";
import { repairContrast } from "../../theme/repair";
import { customThemeDef, useTheme } from "../../theme/ThemeProvider";
import { accountPalette } from "../../theme/tokens";
import { ThemeCard } from "./ThemeCard";
import { useThemeStudio } from "./ThemeStudio";
import "./themes.css";

const ORIGIN: Record<CustomTheme["origin"], MessageKey> = {
  vscode: "theme.origin.vscode",
  described: "theme.origin.described",
  edited: "theme.origin.edited",
};

/**
 * The theme gallery of Ajustes › Apariencia: Hedy's themes and well-known
 * ones, dark and light, then the user's own. A card previews its theme across
 * the whole app; the preview bar applies or cancels it.
 */
export function ThemeGallery({ overview }: { overview: Overview }) {
  const t = useT();
  const toast = useToast();
  const studio = useThemeStudio();
  const menu = useContextMenu();
  const { theme: shown, inUse, mode, system, customs, previewing, preview, cancelPreview } = useTheme();
  const base = mode === "system" ? system : mode;

  const select = (def: ThemeDef) => {
    // The theme the app shows without a preview: back to it.
    if (def.id === inUse[base].id) return cancelPreview();
    if (previewing?.theme.id === def.id) return;
    const { tokens, fixes } = repairContrast(def.tokens, def.base);
    preview({ theme: { ...def, tokens }, fixes });
  };

  const menuItems = (theme: CustomTheme): MenuItem[] => [
    { label: t("theme.rename"), onSelect: () => studio.rename(theme) },
    {
      label: t("theme.copyJson"),
      onSelect: () =>
        void copyText(JSON.stringify(theme, null, 2)).then((ok) =>
          ok ? toast.info(t("theme.copied", { name: theme.name })) : toast.error(t("theme.copyFailed")),
        ),
    },
    { label: t("theme.delete"), danger: true, separatorBefore: true, onSelect: () => studio.remove(theme) },
  ];

  const card = (def: ThemeDef, note: string, custom?: CustomTheme) => (
    <ThemeCard
      key={def.id}
      name={def.name}
      note={note}
      tokens={def.tokens}
      pressed={shown.id === def.id}
      inUse={inUse[def.base].id === def.id}
      viewing={previewing?.theme.id === def.id}
      onSelect={() => select(def)}
      onContextMenu={
        custom
          ? (e: MouseEvent) => {
              e.preventDefault();
              menu.open(e, menuItems(custom), t("theme.menu", { name: custom.name }));
            }
          : undefined
      }
      menu={custom && <MenuButton label={t("theme.menu", { name: custom.name })} items={menuItems(custom)} />}
    />
  );

  const dots = [...overview.accounts.map((a) => a.color), ...accountPalette.map((c) => c.hex)].slice(0, 3);

  return (
    <div className="theme-gallery" id="theme-gallery">
      <div className="theme-gallery__head">
        <h2 tabIndex={-1}>{t("theme.gallery")}</h2>
        <p className="hint">{t("theme.galleryHint")}</p>
      </div>
      {(["dark", "light"] as const).map((group) => (
        <section key={group} className="theme-group" aria-labelledby={`theme-group-${group}`}>
          <h3 id={`theme-group-${group}`} className="theme-group__title">
            {t(group === "dark" ? "theme.group.dark" : "theme.group.light")}
          </h3>
          <ul className="theme-grid">
            {PRESETS.filter((p) => p.base === group).map((p) => card(p, p.note ? t(p.note) : ""))}
          </ul>
        </section>
      ))}
      <section className="theme-group" aria-labelledby="theme-group-custom">
        <h3 id="theme-group-custom" className="theme-group__title">
          {t("theme.group.custom")}
        </h3>
        <ul className="theme-grid">
          {customs.map((c) =>
            card(
              customThemeDef(c),
              `${t(c.base === "dark" ? "theme.base.dark" : "theme.base.light")} · ${t(ORIGIN[c.origin])}`,
              c,
            ),
          )}
          <li className="theme-create">
            <span className="theme-create__title">{t("theme.create")}</span>
            <span className="hint">{t("theme.createHint")}</span>
            <span className="theme-create__actions">
              <Button small onClick={studio.importVscode}>
                {t("theme.import")}
              </Button>
              <Button small onClick={studio.describe}>
                {t("theme.describe")}
              </Button>
            </span>
          </li>
        </ul>
      </section>
      <p className="theme-accounts-note">
        <span className="theme-accounts-note__dots" aria-hidden>
          {dots.map((color, i) => (
            <span key={`${color}-${i}`} className="dot" style={{ background: color }} />
          ))}
        </span>
        {t("theme.accountsNote")}
      </p>
      {menu.element}
    </div>
  );
}
