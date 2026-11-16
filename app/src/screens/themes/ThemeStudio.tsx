import { createContext, useContext, useMemo, useState, type ReactNode } from "react";
import { useQueryClient } from "@tanstack/react-query";

import { errorMessage, OVERVIEW_KEY, useApi } from "../../api/context";
import type { CustomTheme, Overview } from "../../api/types";
import { useT } from "../../i18n";
import { Button } from "../../components/ui/Button";
import { Dialog } from "../../components/ui/Dialog";
import { Field } from "../../components/ui/controls";
import { useToast } from "../../components/ui/Toast";
import { THEMES_KEY, useTheme, type DraftSource } from "../../theme/ThemeProvider";
import { DescribeThemeDialog } from "./DescribeThemeDialog";
import { ImportThemeDialog } from "./ImportThemeDialog";
import { PreviewBar } from "./PreviewBar";

type Open =
  | { kind: "vscode"; source?: Extract<DraftSource, { kind: "vscode" }> }
  | { kind: "described"; source?: Extract<DraftSource, { kind: "described" }> }
  | { kind: "rename"; theme: CustomTheme }
  | { kind: "delete"; theme: CustomTheme };

interface Studio {
  importVscode(): void;
  describe(): void;
  rename(theme: CustomTheme): void;
  remove(theme: CustomTheme): void;
}

const StudioContext = createContext<Studio | null>(null);

export function useThemeStudio(): Studio {
  const studio = useContext(StudioContext);
  if (!studio) throw new Error("useThemeStudio must be used inside <ThemeStudio>");
  return studio;
}

/**
 * Where themes are made and previewed, around the whole layout: the preview bar
 * stays on every screen while the user looks around, and "Ajustar" reopens the
 * dialog a draft came from wherever they are.
 */
export function ThemeStudio({ overview, children }: { overview: Overview; children: ReactNode }) {
  const { preview } = useTheme();
  const [open, setOpen] = useState<Open | null>(null);
  const close = () => setOpen(null);

  const studio = useMemo<Studio>(
    () => ({
      importVscode: () => setOpen({ kind: "vscode" }),
      describe: () => setOpen({ kind: "described" }),
      rename: (theme) => setOpen({ kind: "rename", theme }),
      remove: (theme) => setOpen({ kind: "delete", theme }),
    }),
    [],
  );

  return (
    <StudioContext.Provider value={studio}>
      {children}
      <PreviewBar
        onAdjust={(source) =>
          setOpen(source.kind === "vscode" ? { kind: "vscode", source } : { kind: "described", source })
        }
      />
      {open?.kind === "vscode" && <ImportThemeDialog initial={open.source} onClose={close} onPreview={preview} />}
      {open?.kind === "described" && (
        <DescribeThemeDialog overview={overview} initial={open.source} onClose={close} onPreview={preview} />
      )}
      {open?.kind === "rename" && <RenameThemeDialog theme={open.theme} onClose={close} />}
      {open?.kind === "delete" && <DeleteThemeDialog theme={open.theme} onClose={close} />}
    </StudioContext.Provider>
  );
}

function RenameThemeDialog({ theme, onClose }: { theme: CustomTheme; onClose: () => void }) {
  const t = useT();
  const api = useApi();
  const client = useQueryClient();
  const [name, setName] = useState(theme.name);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const save = async () => {
    if (!name.trim() || name.trim() === theme.name) return onClose();
    setBusy(true);
    try {
      await api.saveTheme({ ...theme, name: name.trim() });
      await client.invalidateQueries({ queryKey: THEMES_KEY });
      onClose();
    } catch (e) {
      setError(errorMessage(e, t));
      setBusy(false);
    }
  };

  return (
    <Dialog
      title={t("theme.renameTitle")}
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>{t("common.cancel")}</Button>
          <Button variant="primary" disabled={busy || !name.trim()} onClick={() => void save()}>
            {t("common.save")}
          </Button>
        </>
      }
    >
      <Field label={t("common.name")}>
        {(id) => (
          <input
            id={id}
            value={name}
            onChange={(e) => setName(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && void save()}
          />
        )}
      </Field>
      {error && (
        <p className="error-text" role="alert">
          {error}
        </p>
      )}
    </Dialog>
  );
}

function DeleteThemeDialog({ theme, onClose }: { theme: CustomTheme; onClose: () => void }) {
  const t = useT();
  const api = useApi();
  const client = useQueryClient();
  const toast = useToast();
  const { inUse, previewing, cancelPreview } = useTheme();
  const [busy, setBusy] = useState(false);
  const id = `custom:${theme.id}`;
  const used = inUse[theme.base].id === id;
  const fallbackName = theme.base === "dark" ? "Tokyo Night" : "Burbuja";

  const remove = async () => {
    setBusy(true);
    try {
      if (previewing?.theme.id === id) cancelPreview();
      await api.deleteTheme(theme.id);
      // In use: back to Hedy's own theme for that mode.
      if (used) await api.setSetting(theme.base === "dark" ? "ui.theme.dark" : "ui.theme.light", null);
      await Promise.all([
        client.invalidateQueries({ queryKey: THEMES_KEY }),
        client.invalidateQueries({ queryKey: OVERVIEW_KEY }),
      ]);
      toast.info(t("theme.deleted", { name: theme.name }));
      onClose();
    } catch (e) {
      toast.error(errorMessage(e, t));
      setBusy(false);
    }
  };

  return (
    <Dialog
      title={t("theme.deleteTitle", { name: theme.name })}
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>{t("common.cancel")}</Button>
          <Button variant="danger" disabled={busy} onClick={() => void remove()}>
            {t("theme.delete")}
          </Button>
        </>
      }
    >
      <p>{t("theme.deleteBody")}</p>
      {used && <p className="hint">{t("theme.deleteInUse", { fallback: fallbackName })}</p>}
    </Dialog>
  );
}
