// A keyboard-first tree view over a virtual list (the WAI-ARIA tree pattern):
// the tree itself holds the focus and points at the active row with
// `aria-activedescendant`, so rows can come and go as the list scrolls.
// Arrows move and open/close, Home/End jump, Enter opens, typing finds a row by
// its name, Shift+F10 (or the menu key) opens the row's context menu.

import {
  forwardRef,
  useEffect,
  useId,
  useImperativeHandle,
  useRef,
  useState,
  type KeyboardEvent,
  type MouseEvent,
  type ReactNode,
} from "react";

import { ChevronIcon } from "./icons";
import type { TreeRow } from "./treeModel";
import { VirtualList, type VirtualListHandle } from "./VirtualList";

export const TREE_ROW_HEIGHT = 26;
/** Indentation per level, in CSS pixels. */
const INDENT = 12;
/** Typed characters within this window form one search ("imp" finds "impuestos.py"). */
const TYPEAHEAD_MS = 600;

export interface VirtualTreeHandle {
  /** Makes `path` the active row and scrolls to it, once it is among the rows. */
  reveal(path: string, options?: { focus?: boolean; center?: boolean }): void;
  focus(): void;
}

export interface RowState {
  active: boolean;
  selected: boolean;
  expanded: boolean;
}

export interface VirtualTreeProps<T> {
  /** Accessible name of the tree. */
  label: string;
  rows: TreeRow<T>[];
  expanded: ReadonlySet<string>;
  onToggle: (path: string, open: boolean) => void;
  /** The open item (a file shown in the viewer); its row is marked as selected. */
  selectedPath?: string | null;
  /** Enter or a click on a leaf. */
  onActivate: (row: TreeRow<T>) => void;
  /** Right click, Shift+F10 or the menu key. */
  onContextMenu?: (row: TreeRow<T>, at: { x: number; y: number }) => void;
  /** Any other key on the active row (F2, Delete…); `true` when it was handled. */
  onRowKey?: (row: TreeRow<T>, key: string) => boolean;
  /** A click or Enter on a row (folders too): e.g. where a new file goes. */
  onRowFocus?: (row: TreeRow<T>) => void;
  /** The row's content after its chevron: icon, name, badges, hover actions. */
  renderRow: (row: TreeRow<T>, state: RowState) => ReactNode;
  /** Shown instead of the rows when there are none. */
  empty?: ReactNode;
  className?: string;
}

/** Finds the next row (after `from`, wrapping) whose name starts with `prefix`. */
export function typeaheadMatch<T>(rows: TreeRow<T>[], from: number, prefix: string): number {
  const wanted = prefix.toLowerCase();
  for (let step = 1; step <= rows.length; step++) {
    const i = (from + step) % rows.length;
    if (rows[i]?.node.name.toLowerCase().startsWith(wanted)) return i;
  }
  return -1;
}

function TreeInner<T>(
  { label, rows, expanded, onToggle, selectedPath, onActivate, onContextMenu, onRowKey, onRowFocus, renderRow, empty, className }: VirtualTreeProps<T>,
  ref: React.ForwardedRef<VirtualTreeHandle>,
) {
  const baseId = useId();
  const list = useRef<VirtualListHandle>(null);
  const [activePath, setActivePath] = useState<string | null>(null);
  const typed = useRef({ text: "", at: 0 });
  // A reveal asked for a row that is not listed yet (its folder opens on the next render).
  const pending = useRef<{ path: string; focus: boolean; center: boolean } | null>(null);

  const indexOf = (path: string | null) => (path === null ? -1 : rows.findIndex((r) => r.node.path === path));
  const activeIndex = Math.max(0, indexOf(activePath));
  const active = rows.length ? rows[Math.min(activeIndex, rows.length - 1)] : undefined;

  const tryReveal = () => {
    const want = pending.current;
    if (!want) return;
    const index = rows.findIndex((r) => r.node.path === want.path);
    if (index < 0) return;
    pending.current = null;
    setActivePath(want.path);
    list.current?.scrollToIndex(index, want.center ? "center" : "nearest");
    if (want.focus) list.current?.element?.focus({ preventScroll: true });
  };

  useImperativeHandle(ref, () => ({
    reveal(path, options) {
      pending.current = { path, focus: options?.focus ?? false, center: options?.center ?? true };
      tryReveal();
    },
    focus() {
      list.current?.element?.focus();
    },
  }));

  // Rows changed (a folder opened): finish a reveal that waited for them.
  useEffect(tryReveal);

  const moveTo = (index: number) => {
    const row = rows[Math.max(0, Math.min(rows.length - 1, index))];
    if (!row) return;
    setActivePath(row.node.path);
    onRowFocus?.(row);
    list.current?.scrollToIndex(rows.indexOf(row));
  };

  const openMenu = (row: TreeRow<T>, index: number) => {
    if (!onContextMenu) return;
    const el = document.getElementById(`${baseId}-r${index}`);
    const box = el?.getBoundingClientRect();
    onContextMenu(row, { x: (box?.left ?? 0) + 24, y: box?.bottom ?? 0 });
  };

  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    if (!active || e.target !== e.currentTarget) return;
    const index = rows.indexOf(active);
    const node = active.node;
    const page = Math.max(1, Math.floor((list.current?.element?.clientHeight ?? 400) / TREE_ROW_HEIGHT) - 1);
    const handled = () => e.preventDefault();
    if (e.altKey || e.ctrlKey || e.metaKey) return;
    switch (e.key) {
      case "ArrowDown":
        handled();
        return moveTo(index + 1);
      case "ArrowUp":
        handled();
        return moveTo(index - 1);
      case "PageDown":
        handled();
        return moveTo(index + page);
      case "PageUp":
        handled();
        return moveTo(index - page);
      case "Home":
        handled();
        return moveTo(0);
      case "End":
        handled();
        return moveTo(rows.length - 1);
      case "ArrowRight":
        handled();
        if (node.kind !== "folder") return;
        if (!expanded.has(node.path)) return onToggle(node.path, true);
        if (rows[index + 1]?.parent === node.path) moveTo(index + 1);
        return;
      case "ArrowLeft": {
        handled();
        if (node.kind === "folder" && expanded.has(node.path)) return onToggle(node.path, false);
        const parent = indexOf(active.parent);
        if (parent >= 0) moveTo(parent);
        return;
      }
      case "Enter":
      case " ":
        handled();
        if (node.kind === "folder") onToggle(node.path, !expanded.has(node.path));
        else onActivate(active);
        return;
      case "ContextMenu":
        handled();
        return openMenu(active, index);
      case "F10":
        if (e.shiftKey) {
          handled();
          openMenu(active, index);
        }
        return;
      default:
        if (onRowKey?.(active, e.key)) {
          handled();
          return;
        }
        if (e.key.length === 1 && e.key !== " ") {
          const now = Date.now();
          const text = now - typed.current.at < TYPEAHEAD_MS ? typed.current.text + e.key : e.key;
          typed.current = { text, at: now };
          // Typing more of the same name stays on the row; a new search starts after it.
          const found = typeaheadMatch(rows, text.length > 1 ? index - 1 : index, text);
          if (found >= 0) {
            handled();
            moveTo(found);
          }
        }
    }
  };

  const renderOne = (index: number) => {
    const row = rows[index];
    if (!row) return null;
    const { node, depth } = row;
    const isFolder = node.kind === "folder";
    const open = isFolder && expanded.has(node.path);
    const state: RowState = { active: row === active, selected: node.path === selectedPath, expanded: open };
    return (
      <div
        key={node.path}
        id={`${baseId}-r${index}`}
        role="treeitem"
        aria-level={depth + 1}
        aria-expanded={isFolder ? open : undefined}
        aria-selected={state.selected}
        className={[
          "vtree__row",
          state.active && "vtree__row--active",
          state.selected && "vtree__row--selected",
          isFolder && "vtree__row--folder",
        ]
          .filter(Boolean)
          .join(" ")}
        style={{ height: TREE_ROW_HEIGHT, paddingLeft: 6 + depth * INDENT }}
        onMouseDown={(e: MouseEvent) => {
          // The tree keeps the focus; a click only moves the active row.
          if (e.button === 0) e.preventDefault();
          list.current?.element?.focus({ preventScroll: true });
        }}
        onClick={(e) => {
          if ((e.target as HTMLElement).closest("button, input")) return;
          setActivePath(node.path);
          onRowFocus?.(row);
          if (isFolder) onToggle(node.path, !open);
          else onActivate(row);
        }}
        onContextMenu={(e) => {
          if (!onContextMenu) return;
          e.preventDefault();
          setActivePath(node.path);
          onContextMenu(row, { x: e.clientX, y: e.clientY });
        }}
      >
        {Array.from({ length: depth }, (_, level) => (
          <span key={level} className="vtree__guide" style={{ left: 6 + level * INDENT + 6 }} aria-hidden />
        ))}
        <span className={`vtree__chevron${open ? " vtree__chevron--open" : ""}`} aria-hidden>
          {isFolder && <ChevronIcon />}
        </span>
        {renderRow(row, state)}
      </div>
    );
  };

  if (rows.length === 0) return <>{empty}</>;

  return (
    <VirtualList
      ref={list}
      role="tree"
      aria-label={label}
      tabIndex={0}
      aria-activedescendant={active ? `${baseId}-r${rows.indexOf(active)}` : undefined}
      className={["vtree", className].filter(Boolean).join(" ")}
      count={rows.length}
      rowHeight={TREE_ROW_HEIGHT}
      renderRow={renderOne}
      onKeyDown={onKeyDown}
      onFocus={() => {
        if (!activePath && rows[0]) setActivePath(selectedPath ?? rows[0].node.path);
      }}
    />
  );
}

/** A virtualized tree; see the module comment. */
export const VirtualTree = forwardRef(TreeInner) as <T>(
  props: VirtualTreeProps<T> & { ref?: React.Ref<VirtualTreeHandle> },
) => ReturnType<typeof TreeInner>;
