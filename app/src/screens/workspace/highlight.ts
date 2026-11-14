// Syntax highlighting for diffs with shiki, loaded lazily: themes and
// languages are separate chunks fetched only when a diff needs them.

import type { HighlighterCore, ThemedToken } from "shiki/core";

import type { ThemeName } from "../../theme/tokens";

/** Tokyo Night as the spec asks; a warm light theme for "Burbuja". */
const CODE_THEME: Record<ThemeName, string> = {
  dark: "tokyo-night",
  light: "catppuccin-latte",
};

const LANG_BY_EXT: Record<string, string> = {
  ts: "typescript",
  tsx: "tsx",
  js: "javascript",
  jsx: "jsx",
  mjs: "javascript",
  cjs: "javascript",
  json: "json",
  py: "python",
  rs: "rust",
  go: "go",
  rb: "ruby",
  php: "php",
  java: "java",
  kt: "kotlin",
  swift: "swift",
  c: "c",
  h: "c",
  cpp: "cpp",
  cs: "csharp",
  css: "css",
  scss: "scss",
  html: "html",
  vue: "vue",
  svelte: "svelte",
  md: "markdown",
  sql: "sql",
  sh: "bash",
  bash: "bash",
  fish: "fish",
  toml: "toml",
  yaml: "yaml",
  yml: "yaml",
  xml: "xml",
  dockerfile: "docker",
};

function languageFor(path: string): string | null {
  const name = path.split("/").pop()?.toLowerCase() ?? "";
  if (name === "dockerfile") return "docker";
  const ext = name.includes(".") ? name.split(".").pop() : undefined;
  return (ext && LANG_BY_EXT[ext]) || null;
}

let highlighter: Promise<HighlighterCore> | null = null;

async function getHighlighter(): Promise<HighlighterCore> {
  highlighter ??= (async () => {
    const [{ createHighlighterCore }, { createJavaScriptRegexEngine }] = await Promise.all([
      import("shiki/core"),
      import("shiki/engine/javascript"),
    ]);
    return createHighlighterCore({ engine: createJavaScriptRegexEngine(), themes: [], langs: [] });
  })();
  return highlighter;
}

async function ensureLoaded(h: HighlighterCore, lang: string, theme: string): Promise<void> {
  const [{ bundledLanguages }, { bundledThemes }] = await Promise.all([
    import("shiki/langs"),
    import("shiki/themes"),
  ]);
  if (!h.getLoadedThemes().includes(theme)) {
    const loader = bundledThemes[theme as keyof typeof bundledThemes];
    if (loader) await h.loadTheme(loader);
  }
  if (!h.getLoadedLanguages().includes(lang)) {
    const loader = bundledLanguages[lang as keyof typeof bundledLanguages];
    if (loader) await h.loadLanguage(loader);
  }
}

/** Tokens per line, or null when the language is unknown. */
export async function highlightLines(
  lines: string[],
  path: string,
  theme: ThemeName,
): Promise<ThemedToken[][] | null> {
  const lang = languageFor(path);
  if (!lang) return null;
  const h = await getHighlighter();
  await ensureLoaded(h, lang, CODE_THEME[theme]);
  if (!h.getLoadedLanguages().includes(lang)) return null;
  // One line at a time: diff hunks are not complete programs.
  return lines.map((line) => h.codeToTokensBase(line, { lang, theme: CODE_THEME[theme] })[0] ?? []);
}
