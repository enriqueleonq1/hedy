// A compact file-type icon set: a handful of shapes by kind of file, muted by
// default and tinted only for a few kinds (code, data/config, styles, images)
// with semantic tokens, so a tree reads at a glance without turning into a rainbow.

import type { ReactNode } from "react";

export type FileKind = "code" | "data" | "style" | "image" | "doc" | "shell" | "lock" | "text" | "file";

const BY_EXT: Record<string, FileKind> = {};
const put = (kind: FileKind, exts: string) => exts.split(" ").forEach((ext) => (BY_EXT[ext] = kind));
put("code", "ts tsx js jsx mjs cjs py rs go rb php java kt kts swift c h cc cpp hpp cs scala dart lua ex exs erl hs ml clj vue svelte astro zig nim r jl");
put("data", "json jsonc json5 toml yaml yml xml ini cfg conf env lock csv tsv sql graphql gql proto plist");
put("style", "css scss sass less styl");
put("image", "png jpg jpeg gif webp svg ico bmp avif");
put("doc", "md mdx markdown rst adoc txt");
put("shell", "sh bash zsh fish ps1 bat cmd");
put("text", "log");

const BY_NAME: Record<string, FileKind> = {
  dockerfile: "shell",
  makefile: "shell",
  justfile: "shell",
  license: "doc",
  ".gitignore": "data",
  ".editorconfig": "data",
  ".prettierrc": "data",
};

export function fileKind(path: string): FileKind {
  const name = (path.split("/").pop() ?? path).toLowerCase();
  const known = BY_NAME[name];
  if (known) return known;
  const ext = name.includes(".") ? name.slice(name.lastIndexOf(".") + 1) : "";
  return BY_EXT[ext] ?? "file";
}

const page = "M4.5 1.75h4.75l3 3v8.5a1 1 0 0 1-1 1h-6.75a1 1 0 0 1-1-1V2.75a1 1 0 0 1 1-1Z";
const fold = "M9.25 1.75v3h3";

const GLYPH: Record<FileKind, ReactNode> = {
  code: (
    <>
      <path d={page} />
      <path d={fold} />
      <path d="m6.75 8-1.25 1.5L6.75 11M9.25 8l1.25 1.5L9.25 11" />
    </>
  ),
  data: (
    <>
      <path d="M6 2.5c-1.25 0-1.75.6-1.75 1.75v2c0 .9-.45 1.5-1.25 1.75.8.25 1.25.85 1.25 1.75v2c0 1.15.5 1.75 1.75 1.75" />
      <path d="M10 2.5c1.25 0 1.75.6 1.75 1.75v2c0 .9.45 1.5 1.25 1.75-.8.25-1.25.85-1.25 1.75v2c0 1.15-.5 1.75-1.75 1.75" />
    </>
  ),
  style: (
    <>
      <path d={page} />
      <path d={fold} />
      <path d="M6.5 7.5 6 12M9 7.5 8.5 12M5.5 9h4.25M5.25 10.75H9.5" />
    </>
  ),
  image: (
    <>
      <rect x="2.25" y="2.75" width="11.5" height="10.5" rx="1.5" />
      <circle cx="5.75" cy="6" r="1.1" />
      <path d="m2.5 11.5 3.25-3 2.5 2.25 1.75-1.5 3.5 3" />
    </>
  ),
  doc: (
    <>
      <path d={page} />
      <path d={fold} />
      <path d="M5.75 8h4.5M5.75 10h4.5M5.75 12h2.75" />
    </>
  ),
  shell: (
    <>
      <rect x="2" y="2.75" width="12" height="10.5" rx="1.5" />
      <path d="m4.75 6.5 1.75 1.5-1.75 1.5M8.25 10.25h3" />
    </>
  ),
  lock: (
    <>
      <rect x="3.25" y="7" width="9.5" height="6.75" rx="1.5" />
      <path d="M5.25 7V5a2.75 2.75 0 0 1 5.5 0v2" />
    </>
  ),
  text: (
    <>
      <path d={page} />
      <path d={fold} />
      <path d="M5.75 8.5h4.5M5.75 11h4.5" />
    </>
  ),
  file: (
    <>
      <path d={page} />
      <path d={fold} />
    </>
  ),
};

/** The icon of a file (or a lock for a protected one). */
export function FileIcon({ path, locked = false }: { path: string; locked?: boolean }) {
  const kind: FileKind = locked ? "lock" : fileKind(path);
  return (
    <svg
      width={16}
      height={16}
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.35}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
      focusable={false}
      className={`file-icon file-icon--${kind}`}
    >
      {GLYPH[kind]}
    </svg>
  );
}

export function FolderIcon({ open }: { open: boolean }) {
  return (
    <svg
      width={16}
      height={16}
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.35}
      strokeLinejoin="round"
      aria-hidden
      focusable={false}
      className="file-icon file-icon--folder"
    >
      {open ? (
        <>
          <path d="M2 12.5V4a1 1 0 0 1 1-1h3l1.5 1.5H12a1 1 0 0 1 1 1V7" />
          <path d="M2 12.5 3.6 7.7a1 1 0 0 1 .95-.7h9.2a.6.6 0 0 1 .57.79l-1.4 4.2a.75.75 0 0 1-.7.51H2Z" />
        </>
      ) : (
        <path d="M2 4a1 1 0 0 1 1-1h3l1.5 1.5H13a1 1 0 0 1 1 1V12a1 1 0 0 1-1 1H3a1 1 0 0 1-1-1V4Z" />
      )}
    </svg>
  );
}
