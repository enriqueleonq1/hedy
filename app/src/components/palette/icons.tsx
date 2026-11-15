// Small line icons of the palette's rows. Decorative: the row carries the name.
// File icons tell kinds apart by shape (code, text, data, image, locked), not by
// color, so they stay quiet next to the names.

const common = {
  width: 16,
  height: 16,
  viewBox: "0 0 24 24",
  fill: "none",
  stroke: "currentColor",
  strokeWidth: 1.8,
  strokeLinecap: "round" as const,
  strokeLinejoin: "round" as const,
  "aria-hidden": true,
  focusable: false,
};

export type FileKind = "code" | "text" | "data" | "image" | "binary" | "locked";

const CODE = /\.(rs|py|ts|tsx|js|jsx|mjs|cjs|go|rb|java|kt|swift|c|h|cc|cpp|hpp|cs|php|sh|bash|zsh|fish|lua|sql|vue|svelte|css|scss|html)$/i;
const TEXT = /(\.(md|mdx|txt|rst|adoc)|^readme|^license|^changelog)$/i;
const DATA = /(\.(json|jsonc|toml|ya?ml|ini|cfg|conf|xml|csv|tsv|lock|env\.example)|^\.?[\w.-]*rc|^dockerfile|^makefile|^\.gitignore)$/i;
const IMAGE = /\.(png|jpe?g|gif|webp|svg|ico|bmp|avif)$/i;
const BINARY = /\.(woff2?|ttf|otf|zip|gz|tar|pdf|wasm|exe|dll|so|dylib|bin)$/i;

export function fileKind(path: string, locked = false): FileKind {
  if (locked) return "locked";
  const name = path.slice(path.lastIndexOf("/") + 1);
  if (IMAGE.test(name)) return "image";
  if (CODE.test(name)) return "code";
  if (TEXT.test(name)) return "text";
  if (DATA.test(name)) return "data";
  if (BINARY.test(name)) return "binary";
  return "text";
}

export function FileIcon({ path, locked }: { path: string; locked?: boolean }) {
  const kind = fileKind(path, locked);
  return (
    <svg {...common} className={`icon palette__file-icon palette__file-icon--${kind}`}>
      {kind === "locked" ? (
        <>
          <rect x="5" y="10.5" width="14" height="10" rx="2" />
          <path d="M8.5 10.5V7.5a3.5 3.5 0 0 1 7 0v3" />
        </>
      ) : kind === "image" ? (
        <>
          <rect x="4" y="4.5" width="16" height="15" rx="2.5" />
          <circle cx="9.5" cy="9.5" r="1.6" />
          <path d="m5 18 4.5-4.5 3 3 2.5-2.5L19.5 18" />
        </>
      ) : (
        <>
          <path d="M6 3.5h8l4.5 4.5v12.5H6z" />
          <path d="M13.5 3.5V8.5h5" />
          {kind === "code" && <path d="m10.5 12.5-2 2 2 2M13.5 12.5l2 2-2 2" />}
          {kind === "text" && <path d="M9 13h6M9 16.5h4" />}
          {kind === "data" && <path d="M9 12.5h2M13 12.5h2M9 16h2M13 16h2" />}
          {kind === "binary" && <path d="M9.5 12.5v4M14.5 12.5v4" />}
        </>
      )}
    </svg>
  );
}

/** ›_: a command. */
export function CommandIcon() {
  return (
    <svg {...common} className="icon">
      <path d="m6 8 4 4-4 4M12 16h6" />
    </svg>
  );
}

/** A symbol of the outline, by kind: ƒ for functions, a box for types, # for headings. */
export function SymbolIcon({ kind }: { kind: string }) {
  if (kind === "heading") {
    return (
      <svg {...common} className="icon">
        <path d="M9 4.5 7.5 19.5M16.5 4.5 15 19.5M5 9h14M4.5 15h14" />
      </svg>
    );
  }
  if (kind === "function" || kind === "method") {
    return (
      <svg {...common} className="icon">
        <path d="M15.5 4.5c-2.5-.5-4 .5-4.5 3L9 19.5c-.4 2-2 2.5-3.5 2M7.5 11h7" />
      </svg>
    );
  }
  if (kind === "constant") {
    return (
      <svg {...common} className="icon">
        <rect x="5" y="8" width="14" height="8" rx="2" />
        <path d="M9 12h6" />
      </svg>
    );
  }
  return (
    <svg {...common} className="icon">
      <path d="M12 3.5 19.5 7.7v8.6L12 20.5l-7.5-4.2V7.7z" />
      <path d="M4.5 7.7 12 12l7.5-4.3M12 12v8.5" />
    </svg>
  );
}

/** A magnifier with a line: a text match. */
export function LineIcon() {
  return (
    <svg {...common} className="icon">
      <path d="M5 7h14M5 12h9M5 17h11" />
    </svg>
  );
}
