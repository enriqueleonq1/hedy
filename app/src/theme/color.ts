// Small color helpers for building themes: hex parsing, mixing and lightness.
// Every token Hedy shows is a 6-digit uppercase hex (`#1A1B26`); only the
// shadow and the scrim carry an alpha channel.

export interface Rgb {
  r: number;
  g: number;
  b: number;
}

const HEX = /^#?([0-9a-f]{3,4}|[0-9a-f]{6}|[0-9a-f]{8})$/i;

/** `#rgb`, `#rgba`, `#rrggbb` or `#rrggbbaa` → channels and alpha (0–1); null when it is not a color. */
export function parseHex(value: string): (Rgb & { a: number }) | null {
  const found = HEX.exec(value.trim())?.[1];
  if (!found) return null;
  let digits = found;
  if (digits.length <= 4) digits = [...digits].map((d) => d + d).join("");
  const n = (i: number) => Number.parseInt(digits.slice(i, i + 2), 16);
  return { r: n(0), g: n(2), b: n(4), a: digits.length === 8 ? n(6) / 255 : 1 };
}

export function isHex(value: unknown): value is string {
  return typeof value === "string" && parseHex(value) !== null;
}

export function toHex({ r, g, b }: Rgb): string {
  const part = (c: number) =>
    Math.round(Math.min(255, Math.max(0, c)))
      .toString(16)
      .padStart(2, "0");
  return `#${part(r)}${part(g)}${part(b)}`.toUpperCase();
}

function rgb(value: string): Rgb {
  const parsed = parseHex(value);
  if (!parsed) throw new Error(`not a color: ${value}`);
  return parsed;
}

/** `a` moved `amount` (0–1) of the way to `b`. */
export function mix(a: string, b: string, amount: number): string {
  const [x, y] = [rgb(a), rgb(b)];
  return toHex({ r: x.r + (y.r - x.r) * amount, g: x.g + (y.g - x.g) * amount, b: x.b + (y.b - x.b) * amount });
}

/** A color with alpha painted over an opaque background (`#61AFEF33` over the editor). */
export function flatten(value: string, background: string): string {
  const color = parseHex(value);
  if (!color) throw new Error(`not a color: ${value}`);
  return color.a >= 1 ? toHex(color) : mix(background, toHex(color), color.a);
}

/** `#RRGGBB` + alpha as two hex digits (`withAlpha("#2B2340", 0.17)` → `#2B23402B`). */
export function withAlpha(value: string, alpha: number): string {
  const a = Math.round(Math.min(1, Math.max(0, alpha)) * 255)
    .toString(16)
    .padStart(2, "0")
    .toUpperCase();
  return `${toHex(rgb(value))}${a}`;
}

interface Hsl {
  h: number;
  s: number;
  l: number;
}

function toHsl({ r, g, b }: Rgb): Hsl {
  const [x, y, z] = [r / 255, g / 255, b / 255];
  const max = Math.max(x, y, z);
  const min = Math.min(x, y, z);
  const l = (max + min) / 2;
  if (max === min) return { h: 0, s: 0, l };
  const d = max - min;
  const s = l > 0.5 ? d / (2 - max - min) : d / (max + min);
  const h = max === x ? (y - z) / d + (y < z ? 6 : 0) : max === y ? (z - x) / d + 2 : (x - y) / d + 4;
  return { h: h / 6, s, l };
}

function fromHsl({ h, s, l }: Hsl): Rgb {
  if (s === 0) return { r: l * 255, g: l * 255, b: l * 255 };
  const q = l < 0.5 ? l * (1 + s) : l + s - l * s;
  const p = 2 * l - q;
  const channel = (t: number) => {
    const u = t < 0 ? t + 1 : t > 1 ? t - 1 : t;
    if (u < 1 / 6) return p + (q - p) * 6 * u;
    if (u < 1 / 2) return q;
    if (u < 2 / 3) return p + (q - p) * (2 / 3 - u) * 6;
    return p;
  };
  return { r: channel(h + 1 / 3) * 255, g: channel(h) * 255, b: channel(h - 1 / 3) * 255 };
}

/** HSL lightness, 0–1. */
export function lightness(value: string): number {
  return toHsl(rgb(value)).l;
}

/** The same hue and saturation at another lightness (0–1). */
export function withLightness(value: string, l: number): string {
  const hsl = toHsl(rgb(value));
  return toHex(fromHsl({ ...hsl, l: Math.min(1, Math.max(0, l)) }));
}
