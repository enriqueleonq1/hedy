// Syntax highlighting by file name, each language pack loaded only when a file
// of that kind is opened (a separate chunk per pack).

import type { Extension } from "@codemirror/state";
import type { StreamParser } from "@codemirror/language";

type Loader = () => Promise<Extension>;

async function legacy(load: () => Promise<StreamParser<unknown>>): Promise<Extension> {
  const [{ StreamLanguage }, parser] = await Promise.all([import("@codemirror/language"), load()]);
  return StreamLanguage.define(parser);
}

const javascript = (options: { typescript?: boolean; jsx?: boolean }): Loader => async () =>
  (await import("@codemirror/lang-javascript")).javascript(options);

const LOADERS: Record<string, Loader> = {
  js: javascript({}),
  mjs: javascript({}),
  cjs: javascript({}),
  jsx: javascript({ jsx: true }),
  ts: javascript({ typescript: true }),
  mts: javascript({ typescript: true }),
  cts: javascript({ typescript: true }),
  tsx: javascript({ typescript: true, jsx: true }),
  py: async () => (await import("@codemirror/lang-python")).python(),
  rs: async () => (await import("@codemirror/lang-rust")).rust(),
  json: async () => (await import("@codemirror/lang-json")).json(),
  jsonc: async () => (await import("@codemirror/lang-json")).json(),
  css: async () => (await import("@codemirror/lang-css")).css(),
  scss: async () => (await import("@codemirror/lang-css")).css(),
  less: async () => (await import("@codemirror/lang-css")).css(),
  html: async () => (await import("@codemirror/lang-html")).html(),
  htm: async () => (await import("@codemirror/lang-html")).html(),
  vue: async () => (await import("@codemirror/lang-html")).html(),
  svelte: async () => (await import("@codemirror/lang-html")).html(),
  md: async () => (await import("@codemirror/lang-markdown")).markdown(),
  mdx: async () => (await import("@codemirror/lang-markdown")).markdown(),
  go: async () => (await import("@codemirror/lang-go")).go(),
  java: async () => (await import("@codemirror/lang-java")).java(),
  c: async () => (await import("@codemirror/lang-cpp")).cpp(),
  h: async () => (await import("@codemirror/lang-cpp")).cpp(),
  cc: async () => (await import("@codemirror/lang-cpp")).cpp(),
  cpp: async () => (await import("@codemirror/lang-cpp")).cpp(),
  hpp: async () => (await import("@codemirror/lang-cpp")).cpp(),
  sql: async () => (await import("@codemirror/lang-sql")).sql(),
  yaml: async () => (await import("@codemirror/lang-yaml")).yaml(),
  yml: async () => (await import("@codemirror/lang-yaml")).yaml(),
  php: async () => (await import("@codemirror/lang-php")).php(),
  xml: async () => (await import("@codemirror/lang-xml")).xml(),
  svg: async () => (await import("@codemirror/lang-xml")).xml(),
  toml: () => legacy(async () => (await import("@codemirror/legacy-modes/mode/toml")).toml),
  sh: () => legacy(async () => (await import("@codemirror/legacy-modes/mode/shell")).shell),
  bash: () => legacy(async () => (await import("@codemirror/legacy-modes/mode/shell")).shell),
  zsh: () => legacy(async () => (await import("@codemirror/legacy-modes/mode/shell")).shell),
  fish: () => legacy(async () => (await import("@codemirror/legacy-modes/mode/shell")).shell),
  rb: () => legacy(async () => (await import("@codemirror/legacy-modes/mode/ruby")).ruby),
  swift: () => legacy(async () => (await import("@codemirror/legacy-modes/mode/swift")).swift),
  lua: () => legacy(async () => (await import("@codemirror/legacy-modes/mode/lua")).lua),
  kt: () => legacy(async () => (await import("@codemirror/legacy-modes/mode/clike")).kotlin),
  kts: () => legacy(async () => (await import("@codemirror/legacy-modes/mode/clike")).kotlin),
  cs: () => legacy(async () => (await import("@codemirror/legacy-modes/mode/clike")).csharp),
  scala: () => legacy(async () => (await import("@codemirror/legacy-modes/mode/clike")).scala),
  dockerfile: () => legacy(async () => (await import("@codemirror/legacy-modes/mode/dockerfile")).dockerFile),
  ini: () => legacy(async () => (await import("@codemirror/legacy-modes/mode/properties")).properties),
  properties: () => legacy(async () => (await import("@codemirror/legacy-modes/mode/properties")).properties),
  diff: () => legacy(async () => (await import("@codemirror/legacy-modes/mode/diff")).diff),
};

/** The language key of a file (`dockerfile` by name, else its extension), or `null`. */
export function languageKey(path: string): string | null {
  const name = (path.split("/").pop() ?? path).toLowerCase();
  if (name === "dockerfile" || name.startsWith("dockerfile.")) return "dockerfile";
  if (name === ".env" || name.startsWith(".env.")) return "ini";
  const ext = name.includes(".") ? name.slice(name.lastIndexOf(".") + 1) : "";
  return ext in LOADERS ? ext : null;
}

const cache = new Map<string, Promise<Extension>>();

/** The highlighting for a file, or `null` when its language is unknown (plain text). */
export function loadLanguage(path: string): Promise<Extension> | null {
  const key = languageKey(path);
  const loader = key ? LOADERS[key] : undefined;
  if (!key || !loader) return null;
  let pending = cache.get(key);
  if (!pending) {
    pending = loader();
    cache.set(key, pending);
    // A failed chunk load is retried next time.
    pending.catch(() => cache.delete(key));
  }
  return pending;
}
