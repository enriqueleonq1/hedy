import { useEffect, useState, type ButtonHTMLAttributes } from "react";

type Variant = "primary" | "secondary" | "ghost" | "danger";

interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: Variant;
  small?: boolean;
}

export function Button({ variant = "secondary", small, className, type = "button", ...rest }: ButtonProps) {
  const classes = ["btn", `btn--${variant}`, small && "btn--small", className].filter(Boolean).join(" ");
  return <button type={type} className={classes} {...rest} />;
}

/** Asks for a second click before running a destructive action. */
export function ConfirmButton({
  children,
  confirmLabel,
  onConfirm,
  className,
  ...rest
}: Omit<ButtonProps, "onClick"> & { confirmLabel: string; onConfirm: () => void }) {
  const [armed, setArmed] = useState(false);

  useEffect(() => {
    if (!armed) return;
    const timer = setTimeout(() => setArmed(false), 4000);
    return () => clearTimeout(timer);
  }, [armed]);

  // Red text from the start; armed, it fills to ask for the second click.
  const classes = [armed && "btn--armed", className].filter(Boolean).join(" ") || undefined;
  return (
    <Button
      variant="danger"
      className={classes}
      onClick={() => (armed ? (setArmed(false), onConfirm()) : setArmed(true))}
      {...rest}
    >
      {armed ? confirmLabel : children}
    </Button>
  );
}
