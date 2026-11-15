// The viewer's designed states: what shows instead of code (binary, too large,
// protected, missing, an error), image previews and the loading skeleton.

import { useState, type ReactNode } from "react";

import { useT } from "../../i18n";

/** "12 KB", "3.4 MB". */
export function formatBytes(size: number): string {
  if (size < 1024) return `${size} B`;
  if (size < 1024 * 1024) return `${Math.round(size / 1024)} KB`;
  return `${(size / (1024 * 1024)).toFixed(1)} MB`;
}

/** A centered state: an icon, a title, a line of text and its actions. */
export function CodeState({
  icon,
  title,
  text,
  actions,
  tone = "neutral",
  role,
}: {
  icon?: ReactNode;
  title: string;
  text?: ReactNode;
  actions?: ReactNode;
  tone?: "neutral" | "locked" | "error";
  role?: "alert" | "status";
}) {
  return (
    <div className={`code-state code-state--${tone}`} role={role}>
      {icon && (
        <span className="code-state__icon" aria-hidden>
          {icon}
        </span>
      )}
      <p className="code-state__title">{title}</p>
      {text && <p className="code-state__text">{text}</p>}
      {actions && <div className="code-state__actions">{actions}</div>}
    </div>
  );
}

/** Lines of different lengths that shimmer while a file loads. */
export function CodeSkeleton() {
  const t = useT();
  return (
    <div className="code-skeleton" role="status" aria-label={t("code.viewer.loading")}>
      {Array.from({ length: 14 }, (_, i) => (
        <span key={i} className="code-skeleton__line" style={{ width: `${18 + ((i * 29) % 62)}%`, marginLeft: `${(i % 4) * 2}ch` }} />
      ))}
    </div>
  );
}

/** An image, centered over a checkerboard (transparency shows), with its size. */
export function ImagePreview({ src, size, name }: { src: string; size: number; name: string }) {
  const t = useT();
  const [dims, setDims] = useState<{ w: number; h: number } | null>(null);
  return (
    <figure className="image-preview">
      <div className="image-preview__stage">
        <img
          src={src}
          alt={name}
          onLoad={(e) => setDims({ w: e.currentTarget.naturalWidth, h: e.currentTarget.naturalHeight })}
        />
      </div>
      <figcaption className="image-preview__meta">
        {dims && <span>{t("code.viewer.dimensions", { w: dims.w, h: dims.h })}</span>}
        <span>{formatBytes(size)}</span>
      </figcaption>
    </figure>
  );
}
