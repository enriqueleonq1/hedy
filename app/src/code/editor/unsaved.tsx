// Unsaved edits, app-wide: the file viewer registers its buffer while it has
// changes, and anything that would drop it (opening another file, switching the
// copy or the task, closing the panel, closing the window) asks first with
// `confirmLeave()`: Guardar / Descartar / Cancelar.

import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";

import { useApi } from "../../api/context";
import { Button } from "../../components/ui/Button";
import { Dialog } from "../../components/ui/Dialog";
import { useT } from "../../i18n";

export interface UnsavedEdit {
  /** The file's name, for the question. */
  name: string;
  /** Saves; resolves to whether it worked (a conflict or an error keeps the buffer). */
  save(): Promise<boolean>;
  /** Drops the changes. */
  discard(): void;
}

export interface UnsavedChanges {
  /** Registers (or, with `null`, removes) the edit of one owner. */
  set(owner: string, edit: UnsavedEdit | null): void;
  /** Whether going on is fine: nothing unsaved, or saved or discarded after asking. */
  confirmLeave(): Promise<boolean>;
  /** Some edit is unsaved now. */
  dirty: boolean;
}

const INERT: UnsavedChanges = { set: () => {}, confirmLeave: async () => true, dirty: false };

const UnsavedContext = createContext<UnsavedChanges>(INERT);

type Choice = "save" | "discard" | "cancel";

/** Mounted once in `App`: holds the unsaved edits and asks before they are lost. */
export function UnsavedChangesProvider({ children }: { children: ReactNode }) {
  const t = useT();
  const api = useApi();
  const edits = useRef(new Map<string, UnsavedEdit>());
  const [dirty, setDirty] = useState(false);
  const [asking, setAsking] = useState<{ edit: UnsavedEdit; answer: (choice: Choice) => void } | null>(null);
  const [saving, setSaving] = useState(false);
  const pending = useRef<Promise<boolean> | null>(null);

  const set = useCallback((owner: string, edit: UnsavedEdit | null) => {
    if (edit) edits.current.set(owner, edit);
    else edits.current.delete(owner);
    setDirty(edits.current.size > 0);
  }, []);

  const confirmLeave = useCallback((): Promise<boolean> => {
    if (edits.current.size === 0) return Promise.resolve(true);
    // A second request while asking waits for the same answer.
    if (pending.current) return pending.current;
    const run = async () => {
      for (const edit of [...edits.current.values()]) {
        const choice = await new Promise<Choice>((answer) => setAsking({ edit, answer }));
        if (choice === "cancel") return false;
        if (choice === "discard") {
          edit.discard();
          continue;
        }
        setSaving(true);
        const ok = await edit.save().finally(() => setSaving(false));
        if (!ok) return false;
      }
      return true;
    };
    pending.current = run().finally(() => {
      pending.current = null;
      setAsking(null);
    });
    return pending.current;
  }, []);

  // Closing the window asks too (the desktop app; nothing in a browser tab).
  useEffect(() => api.onCloseRequested(confirmLeave), [api, confirmLeave]);

  const value = useMemo<UnsavedChanges>(() => ({ set, confirmLeave, dirty }), [set, confirmLeave, dirty]);
  return (
    <UnsavedContext.Provider value={value}>
      {children}
      {asking && (
        <Dialog
          title={t("edit.unsaved.title", { name: asking.edit.name })}
          onClose={() => asking.answer("cancel")}
          footer={
            <>
              <Button variant="primary" disabled={saving} onClick={() => asking.answer("save")}>
                {t("edit.save")}
              </Button>
              <Button variant="ghost" disabled={saving} onClick={() => asking.answer("discard")}>
                {t("edit.discard")}
              </Button>
              <Button variant="ghost" disabled={saving} onClick={() => asking.answer("cancel")}>
                {t("common.cancel")}
              </Button>
            </>
          }
        >
          <p className="muted">{t("edit.unsaved.text")}</p>
        </Dialog>
      )}
    </UnsavedContext.Provider>
  );
}

export function useUnsavedChanges(): UnsavedChanges {
  return useContext(UnsavedContext);
}
