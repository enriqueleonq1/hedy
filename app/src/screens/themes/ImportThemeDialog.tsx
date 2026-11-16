import { useId, useRef, useState } from "react";

import { useT } from "../../i18n";
import { Button } from "../../components/ui/Button";
import { Dialog } from "../../components/ui/Dialog";
import { Field } from "../../components/ui/controls";
import { buildTheme } from "../../theme/derive";
import type { DraftSource, Preview } from "../../theme/ThemeProvider";
import { importVscodeTheme, ThemeImportError, type ImportErrorCode } from "../../theme/vscode";

type VscodeSource = Extract<DraftSource, { kind: "vscode" }>;

const ERROR_KEY = {
  invalidJson: "theme.importError.invalidJson",
  notTheme: "theme.importError.notTheme",
  include: "theme.importError.include",
  missingColors: "theme.importError.missingColors",
} as const satisfies Record<ImportErrorCode, string>;

/** The theme's own name, if the text already reads as a theme. */
function nameIn(text: string): string | null {
  try {
    return importVscodeTheme(text).name;
  } catch {
    return null;
  }
}

/**
 * "Importar de VS Code": a theme file chosen or pasted, read into a seed and
 * shown across the whole app (nothing saved until "Guardar y aplicar").
 */
export function ImportThemeDialog({
  initial,
  onClose,
  onPreview,
}: {
  initial?: VscodeSource;
  onClose: () => void;
  onPreview: (preview: Preview) => void;
}) {
  const t = useT();
  const fileInput = useRef<HTMLInputElement>(null);
  const fileLabel = useId();
  const [text, setText] = useState(initial?.text ?? "");
  const [fileName, setFileName] = useState<string | null>(initial?.fileName ?? null);
  const [name, setName] = useState(initial?.name ?? "");
  const [named, setNamed] = useState(Boolean(initial?.name));
  const [error, setError] = useState<string | null>(null);

  const changeText = (next: string, file: string | null = fileName) => {
    setText(next);
    setError(null);
    if (!named) setName(nameIn(next) ?? (file ? file.replace(/\.jsonc?$/i, "") : ""));
  };

  const pickFile = async (file: File | undefined) => {
    if (!file) return;
    try {
      const content = await file.text();
      setFileName(file.name);
      changeText(content, file.name);
    } catch {
      setError(t("theme.importError.read"));
    }
  };

  const preview = () => {
    if (!text.trim()) return setError(t("theme.importEmpty"));
    try {
      const imported = importVscodeTheme(text);
      const { tokens, fixes } = buildTheme(imported.seed, imported.base);
      const themeName = name.trim() || imported.name || "VS Code";
      onPreview({
        theme: { id: "draft", name: themeName, base: imported.base, tokens },
        fixes,
        draft: { origin: "vscode", source: { kind: "vscode", text, fileName, name: name.trim() } },
      });
      onClose();
    } catch (e) {
      setError(e instanceof ThemeImportError ? t(ERROR_KEY[e.code]) : t("theme.importError.invalidJson"));
    }
  };

  return (
    <Dialog
      title={t("theme.importTitle")}
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>{t("common.cancel")}</Button>
          <Button variant="primary" onClick={preview}>
            {t("theme.importPreview")}
          </Button>
        </>
      }
    >
      <p className="hint">{t("theme.importIntro")}</p>
      <div className="theme-dialog__file">
        <Button aria-describedby={fileName ? fileLabel : undefined} onClick={() => fileInput.current?.click()}>
          {t("theme.importFile")}
        </Button>
        {fileName && (
          <span id={fileLabel} className="mono theme-dialog__file-name">
            {fileName}
          </span>
        )}
      </div>
      <Field label={t("theme.importPaste")}>
        {(id) => (
          <textarea
            id={id}
            className="theme-dialog__paste"
            value={text}
            spellCheck={false}
            placeholder={'{\n  "name": "…",\n  "type": "dark",\n  "colors": { … }\n}'}
            onChange={(e) => changeText(e.target.value)}
          />
        )}
      </Field>
      <Field label={t("common.name")}>
        {(id) => (
          <input
            id={id}
            value={name}
            onChange={(e) => {
              setName(e.target.value);
              setNamed(e.target.value.trim() !== "");
            }}
          />
        )}
      </Field>
      {error && (
        <p className="error-text" role="alert">
          {error}
        </p>
      )}
      {/* Last, so the dialog's first focus lands on "Elegir archivo…". */}
      <input
        ref={fileInput}
        type="file"
        accept=".json,.jsonc,application/json"
        className="sr-only"
        aria-label={t("theme.importFileLabel")}
        tabIndex={-1}
        onChange={(e) => void pickFile(e.target.files?.[0])}
      />
    </Dialog>
  );
}
