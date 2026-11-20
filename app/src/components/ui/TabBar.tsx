// A row of tabs with an icon and a label (the label folds away when the bar is
// narrow; the tooltip still names the tab and its shortcut). Arrow keys move
// between tabs (WAI-ARIA tabs, automatic activation), Home/End jump.

import { useRef, type KeyboardEvent, type ReactNode } from "react";

export interface TabItem<T extends string> {
  id: T;
  label: string;
  icon: ReactNode;
  /** Shown in the tooltip after the label ("Ctrl+Shift+E"). */
  shortcut?: string;
  /** The same for assistive technology (`aria-keyshortcuts`: "Control+Shift+E"). */
  keyshortcuts?: string;
  /** The panel it shows now, when not `${idPrefix}-panel-${id}` (a tab with two views). */
  controls?: string;
}

export function TabBar<T extends string>({
  label,
  tabs,
  value,
  onChange,
  idPrefix,
  end,
}: {
  label: string;
  tabs: TabItem<T>[];
  value: T;
  onChange: (id: T) => void;
  /** Tab ids are `${idPrefix}-tab-${id}`; panels `${idPrefix}-panel-${id}`. */
  idPrefix: string;
  /** Actions at the end of the bar. */
  end?: ReactNode;
}) {
  const refs = useRef(new Map<T, HTMLButtonElement>());

  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    const index = tabs.findIndex((tab) => tab.id === value);
    let next = -1;
    if (e.key === "ArrowRight") next = (index + 1) % tabs.length;
    else if (e.key === "ArrowLeft") next = (index - 1 + tabs.length) % tabs.length;
    else if (e.key === "Home") next = 0;
    else if (e.key === "End") next = tabs.length - 1;
    const tab = tabs[next];
    if (!tab) return;
    e.preventDefault();
    onChange(tab.id);
    refs.current.get(tab.id)?.focus();
  };

  return (
    <div className="tabbar">
      <div className="tabbar__tabs" role="tablist" aria-label={label} onKeyDown={onKeyDown}>
        {tabs.map((tab) => {
          const selected = tab.id === value;
          return (
            <button
              key={tab.id}
              ref={(el) => {
                if (el) refs.current.set(tab.id, el);
                else refs.current.delete(tab.id);
              }}
              type="button"
              role="tab"
              id={`${idPrefix}-tab-${tab.id}`}
              aria-controls={tab.controls ?? `${idPrefix}-panel-${tab.id}`}
              aria-selected={selected}
              aria-keyshortcuts={tab.keyshortcuts}
              tabIndex={selected ? 0 : -1}
              className="tabbar__tab"
              title={tab.shortcut ? `${tab.label} (${tab.shortcut})` : tab.label}
              onClick={() => onChange(tab.id)}
            >
              <span className="tabbar__icon" aria-hidden>
                {tab.icon}
              </span>
              <span className="tabbar__label">{tab.label}</span>
            </button>
          );
        })}
      </div>
      {end && <div className="tabbar__end">{end}</div>}
    </div>
  );
}
