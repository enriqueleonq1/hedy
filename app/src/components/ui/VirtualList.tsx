// A windowed list: only the rows in sight (plus a few around them) are in the
// DOM, so a tree of 100 000 files or a long list of search results scrolls as
// smoothly as ten. Rows have one fixed height, which keeps the math trivial and
// the scroll position exact; that is all a file tree or a result list needs.

import {
  forwardRef,
  useCallback,
  useImperativeHandle,
  useLayoutEffect,
  useRef,
  useState,
  type HTMLAttributes,
  type ReactNode,
} from "react";

/** Rows drawn beyond each edge of the viewport, so a fast scroll never shows a gap. */
const OVERSCAN = 8;
/** Before the viewport is measured (and in jsdom, which has no layout): assume a tall one. */
const FALLBACK_HEIGHT = 640;

/** Which rows to draw for a scroll position (pure, exported for tests). */
export function visibleRange(
  count: number,
  rowHeight: number,
  scrollTop: number,
  viewport: number,
  overscan = OVERSCAN,
): { start: number; end: number } {
  if (count === 0) return { start: 0, end: 0 };
  const height = viewport > 0 ? viewport : FALLBACK_HEIGHT;
  const first = Math.floor(scrollTop / rowHeight);
  const start = Math.max(0, first - overscan);
  const end = Math.min(count, first + Math.ceil(height / rowHeight) + overscan);
  return { start, end };
}

/** The scroll position that brings row `index` into view, or `null` when it already is. */
export function scrollFor(
  index: number,
  rowHeight: number,
  scrollTop: number,
  viewport: number,
  align: "nearest" | "center" = "nearest",
): number | null {
  const top = index * rowHeight;
  const bottom = top + rowHeight;
  if (align === "center") return Math.max(0, top - Math.max(0, (viewport - rowHeight) / 2));
  if (top < scrollTop) return top;
  if (viewport > 0 && bottom > scrollTop + viewport) return bottom - viewport;
  return null;
}

export interface VirtualListHandle {
  /** Scrolls just enough for row `index` to be seen (or centers it). */
  scrollToIndex(index: number, align?: "nearest" | "center"): void;
  /** The scrolling element (to focus it). */
  element: HTMLDivElement | null;
}

export interface VirtualListProps extends Omit<HTMLAttributes<HTMLDivElement>, "children"> {
  count: number;
  /** In CSS pixels; every row has it. */
  rowHeight: number;
  /** Draws row `index`; it is placed by the list (absolute, at its offset). */
  renderRow: (index: number) => ReactNode;
  /** Rendered inside the scroller after the rows (an empty state, a notice). */
  footer?: ReactNode;
}

/**
 * The scroller is the element with the given role and props (a `tree`, a
 * `listbox`…), so keyboard handling and ARIA stay with the caller.
 */
export const VirtualList = forwardRef<VirtualListHandle, VirtualListProps>(function VirtualList(
  { count, rowHeight, renderRow, footer, className, style, onScroll, ...rest },
  ref,
) {
  const scroller = useRef<HTMLDivElement>(null);
  const [scrollTop, setScrollTop] = useState(0);
  const [viewport, setViewport] = useState(0);

  useLayoutEffect(() => {
    const el = scroller.current;
    if (!el) return;
    setViewport(el.clientHeight);
    if (typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(() => setViewport(el.clientHeight));
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  const scrollToIndex = useCallback(
    (index: number, align: "nearest" | "center" = "nearest") => {
      const el = scroller.current;
      if (!el) return;
      const next = scrollFor(index, rowHeight, scrollTop, el.clientHeight || FALLBACK_HEIGHT, align);
      if (next === null) return;
      el.scrollTop = next;
      setScrollTop(next);
    },
    [rowHeight, scrollTop],
  );

  useImperativeHandle(
    ref,
    () => ({
      scrollToIndex,
      get element() {
        return scroller.current;
      },
    }),
    [scrollToIndex],
  );

  const { start, end } = visibleRange(count, rowHeight, scrollTop, viewport);
  const rows: ReactNode[] = [];
  for (let index = start; index < end; index++) rows.push(renderRow(index));

  return (
    <div
      ref={scroller}
      className={["vlist", className].filter(Boolean).join(" ")}
      style={style}
      onScroll={(e) => {
        setScrollTop(e.currentTarget.scrollTop);
        onScroll?.(e);
      }}
      {...rest}
    >
      <div className="vlist__space" style={{ height: count * rowHeight }}>
        <div className="vlist__window" style={{ transform: `translateY(${start * rowHeight}px)` }}>
          {rows}
        </div>
      </div>
      {footer}
    </div>
  );
});
