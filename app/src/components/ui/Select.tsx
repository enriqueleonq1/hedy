import { useEffect, useId, useRef, useState, type KeyboardEvent, type ReactNode } from "react";

import { CheckIcon, ChevronIcon } from "./icons";
import { useAnchoredPopover } from "./popover";

export interface SelectOption<T extends string = string> {
  value: T;
  label: string;
  /** A short second line under the label. */
  detail?: string;
  /** Right-aligned: a shortcut or a figure. */
  hint?: string;
  /** Before the label (a color dot, an icon); decorative. */
  icon?: ReactNode;
  disabled?: boolean;
  /** Indented under the option before it: a setting of that option. */
  nested?: boolean;
}

export interface SelectSection<T extends string = string> {
  /** A small heading over the section's options. */
  title?: string;
  options: SelectOption<T>[];
}

export interface SelectProps<T extends string> {
  value: T;
  onChange: (value: T) => void;
  /** A flat list of options... */
  options?: SelectOption<T>[];
  /** ...or sections, with a title each and a line between them. */
  sections?: SelectSection<T>[];
  /** Accessible name of the control; leave it out when a `<label htmlFor={id}>` names it. */
  label?: string;
  /** For a `<label htmlFor>`. */
  id?: string;
  /** Heading at the top of the list (also its accessible name). */
  title?: string;
  /** Fine print under the list. */
  footnote?: ReactNode;
  disabled?: boolean;
  /** Tooltip of the control. */
  tooltip?: string;
  /** `field`: as wide as a text field; `chip`: a small pill (the composer); `pill`: compact, in tables. */
  variant?: "field" | "chip" | "pill";
  /** What the control shows for the chosen option (by default its icon and label). */
  renderValue?: (option: SelectOption<T> | undefined) => ReactNode;
  className?: string;
}

/** Typed letters within this long make one search ("cla" → "Claude"). */
const TYPEAHEAD_MS = 600;

/**
 * A select with a list of our own (the WAI-ARIA select-only combobox): the focus
 * stays on the control while arrows, Home, End and typed letters move through
 * the list; Enter or Space chooses, Escape closes, a click outside closes. The
 * list opens upward when there is no room below.
 */
export function Select<T extends string>({
  value,
  onChange,
  options,
  sections,
  label,
  id,
  title,
  footnote,
  disabled,
  tooltip,
  variant = "field",
  renderValue,
  className,
}: SelectProps<T>) {
  const baseId = useId();
  const triggerId = id ?? `${baseId}-control`;
  const listId = `${baseId}-list`;
  const titleId = `${baseId}-title`;
  const groups: SelectSection<T>[] = sections ?? [{ options: options ?? [] }];
  const all = groups.flatMap((g) => g.options);
  const selectedIndex = all.findIndex((o) => o.value === value);
  const selected = all[selectedIndex];

  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(-1);
  const wrap = useRef<HTMLSpanElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  const popover = useRef<HTMLDivElement>(null);
  const typed = useRef({ text: "", at: 0 });
  useAnchoredPopover(open, trigger, popover);
  const optionId = (index: number) => `${baseId}-option-${index}`;

  const enabledIndexes = all.flatMap((o, i) => (o.disabled ? [] : [i]));
  const firstEnabled = enabledIndexes[0] ?? -1;
  const lastEnabled = enabledIndexes.at(-1) ?? -1;

  function show(at: number) {
    if (disabled) return;
    setActive(at);
    setOpen(true);
  }

  function close() {
    setOpen(false);
    typed.current = { text: "", at: 0 };
    trigger.current?.focus();
  }

  function choose(index: number) {
    const option = all[index];
    if (!option || option.disabled) return;
    close();
    if (option.value !== value) onChange(option.value);
  }

  /** The next enabled option after (or before) `from`, staying at the ends. */
  function step(from: number, by: 1 | -1): number {
    const after = by === 1 ? enabledIndexes.filter((i) => i > from) : enabledIndexes.filter((i) => i < from).reverse();
    return after[0] ?? from;
  }

  /** Typed letters: the next option whose label starts with them. */
  function typeahead(key: string, from: number, now: number): number {
    const text = (now - typed.current.at > TYPEAHEAD_MS ? "" : typed.current.text) + key.toLowerCase();
    typed.current = { text, at: now };
    // Repeating one letter cycles through the options that start with it.
    const repeated = text.length > 1 && [...text].every((c) => c === text[0]);
    const needle = repeated ? key.toLowerCase() : text;
    const start = repeated || text.length === 1 ? from + 1 : from;
    const order = all.map((_, k) => (start + k) % all.length);
    return order.find((i) => all[i] && !all[i].disabled && all[i].label.toLowerCase().startsWith(needle)) ?? from;
  }

  function onKeyDown(e: KeyboardEvent<HTMLButtonElement>) {
    const printable = e.key.length === 1 && e.key !== " " && !e.ctrlKey && !e.metaKey && !e.altKey;
    const current = selectedIndex >= 0 && !all[selectedIndex]?.disabled ? selectedIndex : firstEnabled;
    if (!open) {
      if (e.key === "ArrowDown" || e.key === "ArrowUp" || e.key === "Enter" || e.key === " ") {
        e.preventDefault();
        show(current);
      } else if (e.key === "Home" || e.key === "End") {
        e.preventDefault();
        show(e.key === "Home" ? firstEnabled : lastEnabled);
      } else if (printable) {
        e.preventDefault();
        show(typeahead(e.key, current, e.timeStamp));
      }
      return;
    }
    const move = (to: number) => {
      e.preventDefault();
      setActive(to);
    };
    if (e.key === "ArrowDown") move(e.altKey ? active : step(active, 1));
    else if (e.key === "ArrowUp" && e.altKey) {
      e.preventDefault();
      choose(active);
    } else if (e.key === "ArrowUp") move(step(active, -1));
    else if (e.key === "Home" || e.key === "PageUp") move(firstEnabled);
    else if (e.key === "End" || e.key === "PageDown") move(lastEnabled);
    else if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      choose(active);
    } else if (e.key === "Escape") {
      e.preventDefault();
      // A dialog around it must not close too.
      e.stopPropagation();
      close();
    } else if (e.key === "Tab") setOpen(false);
    else if (printable) move(typeahead(e.key, active, e.timeStamp));
  }

  // A click anywhere else closes it.
  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (!wrap.current?.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("mousedown", onDown);
    return () => document.removeEventListener("mousedown", onDown);
  }, [open]);

  // The active option stays in sight while the arrows move.
  const activeId = open && active >= 0 ? optionId(active) : undefined;
  useEffect(() => {
    if (activeId) document.getElementById(activeId)?.scrollIntoView?.({ block: "nearest" });
  }, [activeId]);

  // Where each section starts in the flat list of options.
  const offsets = groups.map((_, g) => groups.slice(0, g).reduce((n, x) => n + x.options.length, 0));
  return (
    <span ref={wrap} className={["select", `select--${variant}`, className].filter(Boolean).join(" ")}>
      <button
        ref={trigger}
        id={triggerId}
        type="button"
        role="combobox"
        className="select__control"
        aria-label={label}
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-controls={open ? listId : undefined}
        aria-activedescendant={activeId}
        title={tooltip}
        disabled={disabled}
        onClick={() => (open ? setOpen(false) : show(selectedIndex >= 0 ? selectedIndex : firstEnabled))}
        onKeyDown={onKeyDown}
        onBlur={() => setOpen(false)}
      >
        <span className="select__value">
          {renderValue ? (
            renderValue(selected)
          ) : (
            <>
              {selected?.icon && (
                <span className="select__icon" aria-hidden>
                  {selected.icon}
                </span>
              )}
              <span className="select__text">{selected?.label ?? ""}</span>
            </>
          )}
        </span>
        <span className="select__chevron" aria-hidden>
          <ChevronIcon />
        </span>
      </button>
      {open && (
        <div
          ref={popover}
          className="popover select__popover"
          // The focus stays on the control: a click in the list must not take it.
          onMouseDown={(e) => e.preventDefault()}
        >
          {title && (
            <div id={titleId} className="popover__title">
              {title}
            </div>
          )}
          <div
            role="listbox"
            id={listId}
            className="select__list"
            aria-labelledby={title ? titleId : undefined}
            aria-label={title ? undefined : label}
          >
            {groups.map((group, g) => (
              <div
                key={group.title ?? g}
                role={groups.length > 1 ? "group" : undefined}
                aria-label={groups.length > 1 ? group.title : undefined}
                className="select__group"
              >
                {group.title && (
                  <div className="popover__title" aria-hidden>
                    {group.title}
                  </div>
                )}
                {group.options.map((option, i) => {
                  const at = (offsets[g] ?? 0) + i;
                  const isSelected = option.value === value;
                  return (
                    <div
                      key={option.value}
                      id={optionId(at)}
                      role="option"
                      aria-selected={isSelected}
                      // The label names it; the second line and the hint describe it.
                      aria-labelledby={`${optionId(at)}-label`}
                      aria-describedby={
                        [option.detail && `${optionId(at)}-detail`, option.hint && `${optionId(at)}-hint`]
                          .filter(Boolean)
                          .join(" ") || undefined
                      }
                      aria-disabled={option.disabled || undefined}
                      className={[
                        "popover__item",
                        at === active && "popover__item--active",
                        option.nested && "popover__item--nested",
                      ]
                        .filter(Boolean)
                        .join(" ")}
                      onMouseMove={() => !option.disabled && at !== active && setActive(at)}
                      onClick={() => choose(at)}
                    >
                      <span className="popover__check" aria-hidden>
                        {isSelected && <CheckIcon />}
                      </span>
                      {option.icon && (
                        <span className="popover__icon" aria-hidden>
                          {option.icon}
                        </span>
                      )}
                      <span className="popover__text">
                        <span id={`${optionId(at)}-label`} className="popover__label">
                          {option.label}
                        </span>
                        {option.detail && (
                          <span id={`${optionId(at)}-detail`} className="popover__detail">
                            {option.detail}
                          </span>
                        )}
                      </span>
                      {option.hint && (
                        <span id={`${optionId(at)}-hint`} className="popover__hint">
                          {option.hint}
                        </span>
                      )}
                    </div>
                  );
                })}
              </div>
            ))}
          </div>
          {footnote && <p className="popover__footnote">{footnote}</p>}
        </div>
      )}
    </span>
  );
}

/** A small round swatch: an account's or a project's color, next to its name. */
export function ColorDot({ color }: { color: string }) {
  return <span className="color-dot" style={{ background: color }} />;
}
