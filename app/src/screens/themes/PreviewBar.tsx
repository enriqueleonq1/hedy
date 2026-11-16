import { useEffect, useState } from "react";

import { useT } from "../../i18n";
import { Button } from "../../components/ui/Button";
import { EyeIcon } from "../../components/ui/icons";
import { useTheme, type DraftSource } from "../../theme/ThemeProvider";
import "./themes.css";

/** Something else answers Escape first: a dialog, a menu, an open list, the palette. */
function escapeTakenElsewhere(): boolean {
  return document.querySelector('[aria-modal="true"], [role="menu"], [role="listbox"]') !== null;
}

/**
 * The bar shown on every screen while a theme is previewed: what it is, how its
 * contrast was checked, and Cancelar (Esc) / Aplicar tema. For a theme not
 * saved yet: Ajustar (back to its dialog), Descartar, Guardar y aplicar.
 */
export function PreviewBar({ onAdjust }: { onAdjust: (source: DraftSource) => void }) {
  const t = useT();
  const { previewing, cancelPreview, applyPreview, applying } = useTheme();
  // The theme whose list of fixes is open: another theme starts with it closed.
  const [fixesOf, setFixesOf] = useState<object | null>(null);
  const showFixes = previewing !== null && fixesOf === previewing.theme;

  const active = previewing !== null;
  useEffect(() => {
    if (!active) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape" || e.defaultPrevented || escapeTakenElsewhere()) return;
      cancelPreview();
    };
    window.addEventListener("keydown", onKey);
    // Room under every screen so the bar never hides the composer or the last row.
    document.documentElement.dataset.themePreview = "";
    return () => {
      window.removeEventListener("keydown", onKey);
      delete document.documentElement.dataset.themePreview;
    };
  }, [active, cancelPreview]);

  if (!previewing) return null;
  const { theme, fixes, draft } = previewing;
  const hint =
    fixes.length === 0
      ? t("theme.preview.hint")
      : fixes.length === 1
        ? t("theme.preview.fixedOne")
        : t("theme.preview.fixedMany", { n: fixes.length });

  return (
    <section className="theme-preview" aria-label={t("theme.preview.region")}>
      <span className="theme-preview__icon">
        <EyeIcon />
      </span>
      <div className="theme-preview__text">
        <strong className="theme-preview__title">{t("theme.preview.title", { name: theme.name })}</strong>
        <span className="theme-preview__hint">
          {hint}
          {fixes.length > 0 && (
            <>
              {" "}
              <button
                type="button"
                className="theme-preview__link"
                aria-expanded={showFixes}
                onClick={() => setFixesOf(showFixes ? null : theme)}
              >
                {showFixes ? t("theme.preview.hideFixes") : t("theme.preview.seeFixes")}
              </button>
            </>
          )}
        </span>
      </div>
      <div className="theme-preview__actions">
        {draft && (
          <Button small variant="ghost" disabled={applying} onClick={() => onAdjust(draft.source)}>
            {t("theme.preview.adjust")}
          </Button>
        )}
        <Button small disabled={applying} onClick={cancelPreview} aria-keyshortcuts="Escape">
          {draft ? t("theme.preview.discard") : t("theme.preview.cancel")}
          <kbd className="kbd" aria-hidden>
            Esc
          </kbd>
        </Button>
        <Button small variant="primary" disabled={applying} onClick={() => void applyPreview()}>
          {draft ? t("theme.preview.saveApply") : t("theme.preview.apply")}
        </Button>
      </div>
      {showFixes && (
        <div className="theme-preview__fixes">
          <h3>{t("theme.preview.fixesTitle")}</h3>
          <ul className="theme-preview__fix-list">
            {fixes.map((fix) => (
              <li key={fix.token} className="theme-fix">
                <span className="theme-fix__what">
                  {t("theme.preview.fixOn", { token: fix.token, against: fix.against })}
                </span>
                <span className="theme-fix__colors">
                  <span className="theme-fix__swatch" style={{ background: fix.from }} aria-hidden />
                  {fix.from}
                  <span aria-hidden>→</span>
                  <span className="theme-fix__swatch" style={{ background: fix.to }} aria-hidden />
                  {fix.to}
                </span>
                <span className="theme-fix__ratio">
                  {fix.before.toFixed(1)}:1 → {fix.after.toFixed(1)}:1
                </span>
              </li>
            ))}
          </ul>
        </div>
      )}
    </section>
  );
}
