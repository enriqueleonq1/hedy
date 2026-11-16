import type { CSSProperties, MouseEvent, ReactNode } from "react";

import { useT } from "../../i18n";
import type { ThemeTokens } from "../../theme/tokens";
import { accountPalette, DEFAULT_ACCOUNT_COLOR, tokenStyle } from "../../theme/tokens";
import "./themes.css";

/** Three account colors for the miniature's sidebar: they never change with the theme. */
const MINI_ACCOUNTS = [0, 2, 3].map((i) => accountPalette[i]?.hex ?? DEFAULT_ACCOUNT_COLOR);

/**
 * Hedy in miniature, painted in `tokens` (scoped CSS variables on this element
 * only): the sidebar with Blob and the accounts, a message, code with its syntax
 * colors and an added line, and the composer with its action button.
 */
export function ThemeMiniature({ tokens }: { tokens: ThemeTokens }) {
  return (
    <div className="theme-mini" style={tokenStyle(tokens) as CSSProperties} aria-hidden>
      <div className="theme-mini__side">
        <span className="theme-mini__blob" style={{ background: DEFAULT_ACCOUNT_COLOR }}>
          <i />
          <i />
        </span>
        <span className="theme-mini__nav theme-mini__nav--current" />
        {MINI_ACCOUNTS.map((color) => (
          <span key={color} className="theme-mini__account">
            <span className="theme-mini__dot" style={{ background: color }} />
            <span className="theme-mini__label" />
          </span>
        ))}
      </div>
      <div className="theme-mini__main">
        <span className="theme-mini__bubble">
          <span />
        </span>
        <span className="theme-mini__code">
          <span className="theme-mini__line">
            <b className="syn-keyword" />
            <b className="syn-function" />
            <b className="syn-ink" />
          </span>
          <span className="theme-mini__line theme-mini__line--indent">
            <b className="syn-type" />
            <b className="syn-string" />
          </span>
          <span className="theme-mini__line theme-mini__line--added">
            <b className="syn-added" />
            <b className="syn-number" />
          </span>
          <span className="theme-mini__line">
            <b className="syn-comment" />
          </span>
        </span>
        <span className="theme-mini__composer">
          <span className="theme-mini__placeholder" />
          <span className="theme-mini__send" />
        </span>
      </div>
    </div>
  );
}

interface ThemeCardProps {
  name: string;
  note: string;
  tokens: ThemeTokens;
  /** On screen now (previewed, or the theme in use when nothing is). */
  pressed: boolean;
  inUse: boolean;
  viewing: boolean;
  onSelect: () => void;
  onContextMenu?: (e: MouseEvent) => void;
  /** The ⋯ menu of a theme of the user's own. */
  menu?: ReactNode;
}

/** One theme of the gallery: a real button that previews it across the app. */
export function ThemeCard({ name, note, tokens, pressed, inUse, viewing, onSelect, onContextMenu, menu }: ThemeCardProps) {
  const t = useT();
  return (
    <li className={`theme-card${menu ? " theme-card--menu" : ""}`} onContextMenu={onContextMenu}>
      <button type="button" className="theme-card__select" aria-pressed={pressed} onClick={onSelect}>
        <ThemeMiniature tokens={tokens} />
        <span className="theme-card__foot">
          <span className="theme-card__text">
            <span className="theme-card__name">{name}</span>
            <span className="theme-card__note">{note}</span>
          </span>
          {(inUse || viewing) && (
            <span className="theme-card__badges">
              {viewing && <span className="theme-badge theme-badge--viewing">{t("theme.badge.viewing")}</span>}
              {inUse && <span className="theme-badge">{t("theme.badge.inUse")}</span>}
            </span>
          )}
        </span>
      </button>
      {menu && <span className="theme-card__menu">{menu}</span>}
    </li>
  );
}
