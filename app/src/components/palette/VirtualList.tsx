// A listbox of titled sections that only renders the rows in view (plus the
// active one), so 50,000 files or 2,000 matches scroll smoothly. Rows have a
// fixed height; sections keep their `role="group"` for screen readers.

import { useLayoutEffect, useRef, useState, type MouseEvent, type ReactNode } from "react";

export const ROW_HEIGHT = 38;
export const HEADER_HEIGHT = 30;
/** Before the list is measured (and in tests, which have no layout). */
const FALLBACK_VIEWPORT = 1200;
const OVERSCAN = 6 * ROW_HEIGHT;

export interface ListSection<T> {
  key: string;
  /** Read by screen readers as the group's name; no header row when absent. */
  title?: string;
  /** What the header shows instead of the plain title (an icon, a path). */
  header?: ReactNode;
  items: T[];
}

interface Placed<T> {
  section: ListSection<T>;
  /** Offset of the section in the list. */
  top: number;
  /** Height of its title row (0 without one). */
  head: number;
  /** Index of its first item across every section. */
  start: number;
}

function layout<T>(sections: ListSection<T>[]): { placed: Placed<T>[]; total: number } {
  const placed: Placed<T>[] = [];
  let top = 0;
  let start = 0;
  for (const section of sections) {
    const head = section.title ? HEADER_HEIGHT : 0;
    placed.push({ section, top, head, start });
    top += head + section.items.length * ROW_HEIGHT;
    start += section.items.length;
  }
  return { placed, total: top };
}

export function VirtualList<T>({
  id,
  label,
  sections,
  active,
  optionId,
  itemKey,
  onHover,
  onPick,
  children,
  footer,
}: {
  id: string;
  label: string;
  sections: ListSection<T>[];
  /** Index of the active item across every section. */
  active: number;
  optionId: (index: number) => string;
  itemKey: (item: T) => string;
  onHover: (index: number) => void;
  onPick: (index: number, e: MouseEvent) => void;
  /** The option's contents. */
  children: (item: T, selected: boolean) => ReactNode;
  /** Below the rows, inside the scrolling area (an empty state). */
  footer?: ReactNode;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [scroll, setScroll] = useState(0);
  const [viewport, setViewport] = useState(0);
  const { placed, total } = layout(sections);

  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const measure = () => setViewport(el.clientHeight);
    measure();
    if (typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  // Keep the active row in view (with its section's title when it is the first).
  const holder = placed.findLast((p) => p.start <= active);
  const activeTop = holder ? holder.top + holder.head + (active - holder.start) * ROW_HEIGHT : 0;
  const activeFrom = holder && active === holder.start ? activeTop - holder.head : activeTop;
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el || !el.clientHeight) return;
    if (activeFrom < el.scrollTop) el.scrollTop = activeFrom;
    else if (activeTop + ROW_HEIGHT > el.scrollTop + el.clientHeight) el.scrollTop = activeTop + ROW_HEIGHT - el.clientHeight;
  }, [activeTop, activeFrom]);

  // Back to the top when the results change.
  const first = sections[0]?.items[0];
  const signature = `${sections.length}:${sections[0]?.key ?? ""}:${first === undefined ? "" : itemKey(first)}`;
  const [shownSignature, setShownSignature] = useState(signature);
  if (shownSignature !== signature) {
    setShownSignature(signature);
    setScroll(0);
  }
  useLayoutEffect(() => {
    if (ref.current) ref.current.scrollTop = 0;
  }, [signature]);

  const height = viewport || FALLBACK_VIEWPORT;
  const lo = scroll - OVERSCAN;
  const hi = scroll + height + OVERSCAN;

  return (
    <div
      ref={ref}
      id={id}
      className="palette__list"
      role="listbox"
      aria-label={label}
      onScroll={(e) => setScroll(e.currentTarget.scrollTop)}
    >
      <div className="palette__rows" style={{ height: total }}>
        {placed.map(({ section, top, head, start }, s) => {
          const bottom = top + head + section.items.length * ROW_HEIGHT;
          const holdsActive = holder?.section === section;
          if ((bottom < lo || top > hi) && !holdsActive) return null;
          const firstRow = Math.max(0, Math.floor((lo - top - head) / ROW_HEIGHT));
          const lastRow = Math.min(section.items.length - 1, Math.floor((hi - top - head) / ROW_HEIGHT));
          const rows: number[] = [];
          for (let r = firstRow; r <= lastRow; r++) rows.push(r);
          const activeRow = active - start;
          if (holdsActive && (activeRow < firstRow || activeRow > lastRow)) rows.push(activeRow);
          const headerId = `${id}-group-${s}`;
          return (
            <div
              key={section.key}
              role="group"
              aria-labelledby={section.title ? headerId : undefined}
              className="palette__section"
              style={{ top, height: bottom - top }}
            >
              {section.title && (
                <div id={headerId} className="palette__group" role="presentation">
                  {section.header ?? section.title}
                </div>
              )}
              {rows.map((r) => {
                const item = section.items[r];
                if (item === undefined) return null;
                const index = start + r;
                const selected = index === active;
                return (
                  <div
                    key={itemKey(item)}
                    id={optionId(index)}
                    role="option"
                    aria-selected={selected}
                    className="palette__option"
                    style={{ top: head + r * ROW_HEIGHT }}
                    onMouseMove={() => !selected && onHover(index)}
                    onMouseDown={(e) => e.preventDefault()}
                    onClick={(e) => onPick(index, e)}
                  >
                    {children(item, selected)}
                  </div>
                );
              })}
            </div>
          );
        })}
      </div>
      {footer}
    </div>
  );
}
