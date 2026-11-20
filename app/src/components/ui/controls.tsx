import { useId } from "react";

import { useT } from "../../i18n";
import { accountPalette } from "../../theme/tokens";

const HEX = /^#[0-9a-fA-F]{6}$/;

/** Palette swatches plus a custom hex field; an empty `value` is no color yet. */
export function ColorPicker({
  value,
  onChange,
  label,
}: {
  value: string;
  onChange: (hex: string) => void;
  label: string;
}) {
  const t = useT();
  const isPalette = accountPalette.some((c) => c.hex.toLowerCase() === value.toLowerCase());

  return (
    <div className="color-picker" role="radiogroup" aria-label={label}>
      {accountPalette.map((c) => {
        const selected = c.hex.toLowerCase() === value.toLowerCase();
        return (
          <button
            key={c.hex}
            type="button"
            role="radio"
            aria-checked={selected}
            aria-label={t(c.label)}
            title={t(c.label)}
            className="swatch"
            style={{ background: c.hex }}
            onClick={() => onChange(c.hex)}
          />
        );
      })}
      <input
        key={value}
        className={`hex-input${isPalette || !value ? "" : " hex-input--active"}`}
        aria-label={t("shell.customColor", { label })}
        defaultValue={value}
        maxLength={7}
        spellCheck={false}
        onBlur={(e) => {
          const hex = e.target.value.trim();
          if (HEX.test(hex) && hex.toLowerCase() !== value.toLowerCase()) onChange(hex.toUpperCase());
        }}
        onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
      />
    </div>
  );
}

export function Switch({
  checked,
  onChange,
  label,
  disabled,
}: {
  checked: boolean;
  onChange: (checked: boolean) => void;
  label: string;
  disabled?: boolean;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      disabled={disabled}
      className="switch"
      onClick={() => onChange(!checked)}
    >
      <span className="switch__thumb" />
    </button>
  );
}

export function Segmented<T extends string>({
  value,
  options,
  onChange,
  label,
}: {
  value: T;
  options: { value: T; label: string }[];
  onChange: (value: T) => void;
  label: string;
}) {
  return (
    <div className="segmented" role="radiogroup" aria-label={label}>
      {options.map((o) => (
        <button
          key={o.value}
          type="button"
          role="radio"
          aria-checked={o.value === value}
          className="segmented__option"
          onClick={() => onChange(o.value)}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}

export function Field({ label, hint, children }: { label: string; hint?: string; children: (id: string) => React.ReactNode }) {
  const id = useId();
  return (
    <div className="field">
      <label htmlFor={id}>{label}</label>
      {children(id)}
      {hint && <p className="hint">{hint}</p>}
    </div>
  );
}
