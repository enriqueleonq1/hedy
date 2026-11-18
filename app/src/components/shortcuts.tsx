// Keyboard shortcuts of the shell: Ctrl/⌘ 1…9 jumps to a sidebar project, and
// "?" (outside a text field) opens the list of every shortcut.

import { createContext, Fragment, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from "react";

import { useT, type MessageKey } from "../i18n";
import { altKey, modKey, shiftKey } from "./palette/keys";
import { Dialog } from "./ui/Dialog";

/** Typing there means text, not a shortcut (a terminal included). */
export function isTypingTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  if (target.closest(".xterm")) return true;
  if (target.isContentEditable || target instanceof HTMLTextAreaElement || target instanceof HTMLSelectElement) return true;
  if (target instanceof HTMLInputElement) {
    return !["button", "checkbox", "radio", "range", "color", "file", "submit", "reset"].includes(target.type);
  }
  return false;
}

/**
 * 1…9 for Alt/⌥ plus a digit, else null. By physical key: ⌥ digit types a symbol on
 * a Mac, and AltGr (which types "@", "#"… on many layouts) is left alone.
 * Ctrl/⌘ plus a digit already jumps between a project's tasks in the workspace.
 */
export function projectShortcut(e: KeyboardEvent): number | null {
  if (!e.altKey || e.ctrlKey || e.metaKey || e.shiftKey || e.getModifierState?.("AltGraph")) return null;
  const digit = /^Digit([1-9])$/.exec(e.code)?.[1] ?? /^[1-9]$/.exec(e.key)?.[0];
  return digit ? Number(digit) : null;
}

/** Alt/⌥ N opens the Nth project of the sidebar (anywhere but a terminal). */
export function useProjectShortcuts(projects: { path: string }[], open: (path: string) => void) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.target instanceof Element && e.target.closest(".xterm")) return;
      // Not from behind a dialog: it may hold something that must finish.
      if (document.querySelector('[aria-modal="true"]')) return;
      const n = projectShortcut(e);
      const project = n ? projects[n - 1] : undefined;
      if (!project) return;
      e.preventDefault();
      open(project.path);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [projects, open]);
}

interface Shortcuts {
  /** Opens the list of shortcuts. */
  show: () => void;
}

const ShortcutsContext = createContext<Shortcuts>({ show: () => {} });

export function ShortcutsProvider({ children }: { children: ReactNode }) {
  const [open, setOpen] = useState(false);
  const show = useCallback(() => setOpen(true), []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "?" || e.ctrlKey || e.metaKey || e.altKey || e.defaultPrevented) return;
      if (isTypingTarget(e.target)) return;
      // Not over another dialog (the palette included).
      if (document.querySelector('[aria-modal="true"]')) return;
      e.preventDefault();
      setOpen(true);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const value = useMemo(() => ({ show }), [show]);
  return (
    <ShortcutsContext.Provider value={value}>
      {children}
      {open && <ShortcutsDialog onClose={() => setOpen(false)} />}
    </ShortcutsContext.Provider>
  );
}

export function useShortcuts(): Shortcuts {
  return useContext(ShortcutsContext);
}

interface Row {
  label: MessageKey;
  /** Each combination is a list of keys pressed together. */
  combos: string[][];
  /** Between combinations: a range ("1 – 9") or alternatives ("j / k"). */
  joiner?: "range" | "or";
}

function groups(mod: string, alt: string, shift: string, keys: { click: string; del: string }): { title: MessageKey; rows: Row[] }[] {
  return [
    {
      title: "shell.shortcuts.general",
      rows: [
        { label: "shell.shortcuts.palette", combos: [[mod, "K"]] },
        { label: "shell.shortcuts.quickOpen", combos: [[mod, "P"]] },
        { label: "shell.shortcuts.commands", combos: [["F1"], [mod, shift, "P"]], joiner: "or" },
        { label: "shell.shortcuts.projects", combos: [[alt, "1"], [alt, "9"]], joiner: "range" },
        { label: "shell.shortcuts.tasks", combos: [[mod, "1"], [mod, "9"]], joiner: "range" },
        { label: "shell.shortcuts.focus", combos: [[mod, "\\"]] },
        { label: "shell.shortcuts.help", combos: [["?"]] },
      ],
    },
    {
      title: "shell.shortcuts.composer",
      rows: [
        { label: "shell.shortcuts.send", combos: [["Enter"]] },
        { label: "shell.shortcuts.newLine", combos: [["Shift", "Enter"]] },
        { label: "shell.shortcuts.stop", combos: [["Esc"]] },
      ],
    },
    {
      title: "code.shortcuts.title",
      rows: [
        { label: "code.shortcuts.explorer", combos: [[mod, "Shift", "E"]] },
        { label: "code.shortcuts.search", combos: [[mod, "Shift", "F"]] },
        { label: "code.shortcuts.review", combos: [[mod, "Shift", "G"]] },
        { label: "code.shortcuts.find", combos: [[mod, "F"]] },
        { label: "code.shortcuts.gotoLine", combos: [[mod, "G"]] },
        { label: "code.shortcuts.menu", combos: [["Shift", "F10"]] },
        { label: "code.shortcuts.typeahead", combos: [["a"], ["z"]], joiner: "range" },
      ],
    },
    {
      title: "edit.shortcuts.title",
      rows: [
        { label: "edit.shortcuts.toggle", combos: [[mod, "E"]] },
        { label: "edit.shortcuts.save", combos: [[mod, "S"]] },
        { label: "edit.shortcuts.undo", combos: [[mod, "Z"], [mod, shift, "Z"]], joiner: "or" },
        { label: "edit.shortcuts.replace", combos: [[mod, "H"]] },
        { label: "edit.shortcuts.cursor", combos: [[alt, keys.click]] },
        { label: "edit.shortcuts.next", combos: [[mod, "D"]] },
        { label: "edit.shortcuts.rename", combos: [["F2"]] },
        { label: "edit.shortcuts.delete", combos: [[keys.del]] },
      ],
    },
    {
      title: "shell.shortcuts.activity",
      rows: [
        { label: "shell.shortcuts.move", combos: [["j"], ["k"]], joiner: "or" },
        { label: "shell.shortcuts.approve", combos: [["a"]] },
        { label: "shell.shortcuts.reject", combos: [["r"]] },
        { label: "shell.shortcuts.open", combos: [["Enter"]] },
      ],
    },
    {
      title: "reviewer.shortcuts.title",
      rows: [
        { label: "reviewer.shortcuts.files", combos: [["j"], ["k"]], joiner: "or" },
        { label: "reviewer.shortcuts.lines", combos: [["↑"], ["↓"]], joiner: "or" },
        { label: "reviewer.shortcuts.select", combos: [["Shift", "↑"], ["Shift", "↓"]], joiner: "or" },
        { label: "reviewer.shortcuts.comment", combos: [["C"], ["Enter"]], joiner: "or" },
        { label: "reviewer.shortcuts.add", combos: [[mod, "Enter"]] },
        { label: "reviewer.shortcuts.menu", combos: [["Shift", "F10"]] },
      ],
    },
    {
      title: "ship.shortcuts.title",
      rows: [{ label: "ship.shortcuts.confirm", combos: [[mod, "Enter"]] }],
    },
  ];
}

function ShortcutsDialog({ onClose }: { onClose: () => void }) {
  const t = useT();
  return (
    <Dialog title={t("shell.shortcuts.title")} onClose={onClose}>
      <div className="shortcuts">
        {groups(modKey(), altKey(), shiftKey(), { click: t("edit.shortcuts.click"), del: t("edit.deleteKey") }).map((group) => (
          <section key={group.title} className="shortcuts__group">
            <h3 className="shortcuts__title">{t(group.title)}</h3>
            <dl className="shortcuts__list">
              {group.rows.map((row) => (
                <div key={row.label} className="shortcuts__row">
                  <dt>{t(row.label)}</dt>
                  <dd className="shortcuts__keys">
                    {row.combos.map((combo, i) => (
                      <Fragment key={combo.join("+")}>
                        {i > 0 && (
                          <span className="shortcuts__joiner">
                            {row.joiner === "range" ? "–" : "/"}
                          </span>
                        )}
                        <kbd className="shortcuts__combo">
                          {combo.map((key) => (
                            <kbd key={key}>{key}</kbd>
                          ))}
                        </kbd>
                      </Fragment>
                    ))}
                  </dd>
                </div>
              ))}
            </dl>
          </section>
        ))}
      </div>
    </Dialog>
  );
}
