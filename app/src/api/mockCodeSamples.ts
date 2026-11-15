// Sample repositories for `MockApi`'s code navigation: small, realistic and
// deterministic. `/code/hedy` is a Python app (task s5 fixes the invoice
// taxes); `/code/homming` a TypeScript shop panel.

/** A file at the repository's `HEAD`. */
export type SampleFile =
  | { kind: "text"; text: string; commits: string[] }
  | { kind: "image"; mime: string; base64: string; commits: string[] }
  | { kind: "binary"; size: number; commits: string[] }
  /** Bigger than the text limit: only its size is known. */
  | { kind: "large"; size: number; commits: string[] }
  /** A protected file: its contents are never served. */
  | { kind: "locked"; commits: string[] };

export interface SampleCommit {
  hash: string;
  author: string;
  /** Unix seconds. */
  date: number;
  subject: string;
}

export interface SampleRepo {
  branch: string;
  /** Newest first; the first one is `HEAD`. */
  commits: SampleCommit[];
  /** Files at `HEAD`; `commits` lists who touched each one, newest first. */
  files: Record<string, SampleFile>;
  /** The user's uncommitted changes in the folder: new text, or `null` = deleted. */
  folderChanges: Record<string, string | null>;
  /** New files in the folder git does not know yet. */
  untracked: Record<string, string>;
  /** Git-ignored entries (folders end with `/`). */
  ignored: string[];
  /** Each task's changes against `HEAD` (its base): new text, or `null` = deleted. */
  tasks: Record<string, Record<string, string | null>>;
}

/** A 1×1 PNG. */
const PIXEL =
  "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";

const DAY = 86_400;
const T0 = 1_762_000_000;

const text = (body: string, ...commits: string[]): SampleFile => ({ kind: "text", text: body, commits });

const IMPUESTOS = `"""Impuestos de las facturas."""

from decimal import ROUND_HALF_UP, Decimal

IVA = Decimal("0.19")
CENTAVO = Decimal("0.01")


def redondear(valor: Decimal) -> Decimal:
    """Redondea al centavo, la mitad hacia arriba."""
    return valor.quantize(CENTAVO, rounding=ROUND_HALF_UP)


def iva_de_linea(precio: Decimal, cantidad: int) -> Decimal:
    """IVA de una línea de la factura."""
    return precio * cantidad * IVA


def iva_total(lineas: list[tuple[Decimal, int]]) -> Decimal:
    """IVA de toda la factura."""
    return redondear(sum(iva_de_linea(p, c) for p, c in lineas))
`;

const TOTALES = `"""Totales de una factura."""

from decimal import Decimal

from facturas.impuestos import iva_total, redondear


def subtotal(lineas: list[tuple[Decimal, int]]) -> Decimal:
    """Suma de precio por cantidad, sin impuestos."""
    return redondear(sum(p * c for p, c in lineas))


def total(lineas: list[tuple[Decimal, int]]) -> Decimal:
    """Subtotal más IVA."""
    return redondear(subtotal(lineas) * (1 + IVA))
`;

const TEST_FACTURAS = `from decimal import Decimal

from facturas.totales import subtotal, total


def test_subtotal():
    assert subtotal([(Decimal("1000"), 2)]) == Decimal("2000.00")


def test_total_con_iva():
    lineas = [(Decimal("990"), 1), (Decimal("10.50"), 3)]
    assert total(lineas) == Decimal("1215.58")
`;

const README = `# Hedy Citas

Agenda de citas para una peluquería: reservas, recordatorios por WhatsApp y
facturas con IVA.

## Instalar

\`\`\`sh
uv sync
uv run pytest
\`\`\`

## Estructura

- \`citas/\`: modelo y servicio de reservas.
- \`notificaciones/\`: recordatorios por WhatsApp.
- \`facturas/\`: impuestos, totales y PDF.
`;

export const HEDY_REPO: SampleRepo = {
  branch: "main",
  commits: [
    { hash: "a3f9c1e", author: "Enrique León", date: T0 - 2 * DAY, subject: "Arreglo del envío" },
    { hash: "7bd20aa", author: "Enrique León", date: T0 - 5 * DAY, subject: "Plantillas de correo" },
    { hash: "5c81d02", author: "Camila Rojas", date: T0 - 9 * DAY, subject: "Facturas con IVA" },
    { hash: "2e4b7a9", author: "Enrique León", date: T0 - 15 * DAY, subject: "Citas y recordatorios" },
    { hash: "0f12ab3", author: "Enrique León", date: T0 - 30 * DAY, subject: "Inicio" },
  ],
  files: {
    ".env.example": { kind: "locked", commits: ["0f12ab3"] },
    ".gitignore": text(".env\n.venv/\n__pycache__/\nbuild/\n", "0f12ab3"),
    "README.md": text(README, "5c81d02", "0f12ab3"),
    "pyproject.toml": text(
      '[project]\nname = "hedy-citas"\nversion = "0.3.0"\nrequires-python = ">=3.12"\ndependencies = ["httpx>=0.28", "reportlab>=4.2"]\n\n[tool.pytest.ini_options]\ntestpaths = ["tests"]\n',
      "5c81d02",
      "0f12ab3",
    ),
    "config/settings.py": text(
      '"""Ajustes leídos del entorno."""\n\nimport os\n\nWHATSAPP_TOKEN = os.environ.get("WHATSAPP_TOKEN", "")\nZONA_HORARIA = "America/Santiago"\nHORAS_ANTES_RECORDATORIO = 24\n',
      "2e4b7a9",
      "0f12ab3",
    ),
    "config/logging.toml": text('[root]\nlevel = "INFO"\nhandlers = ["consola"]\n', "0f12ab3"),
    "citas/__init__.py": text("", "2e4b7a9"),
    "citas/modelos.py": text(
      '"""Citas de la agenda."""\n\nfrom dataclasses import dataclass\nfrom datetime import datetime\n\n\n@dataclass\nclass Cita:\n    cliente: str\n    servicio: str\n    hora: datetime\n\n    @classmethod\n    def nueva(cls, cliente: str, servicio: str, hora: datetime) -> "Cita":\n        return cls(cliente, servicio, hora)\n\n    def guardar(self) -> None:\n        AGENDA.append(self)\n\n\nAGENDA: list[Cita] = []\n',
      "2e4b7a9",
    ),
    "citas/servicio.py": text(
      '"""Reservar una cita y programar su recordatorio."""\n\nfrom citas.modelos import Cita\nfrom notificaciones.recordatorios import programar_recordatorio\n\n\ndef crear_cita(cliente, servicio, hora):\n    cita = Cita.nueva(cliente, servicio, hora)\n    cita.guardar()\n    programar_recordatorio(cita, horas=24)\n    return cita\n',
      "a3f9c1e",
      "2e4b7a9",
    ),
    "citas/vistas.py": text(
      'from citas.servicio import crear_cita\n\n\ndef reservar(peticion):\n    datos = peticion.json()\n    cita = crear_cita(datos["cliente"], datos["servicio"], datos["hora"])\n    return {"id": id(cita), "estado": "reservada"}\n',
      "2e4b7a9",
    ),
    "facturas/__init__.py": text("", "5c81d02"),
    "facturas/impuestos.py": text(IMPUESTOS, "5c81d02"),
    "facturas/totales.py": text(TOTALES, "5c81d02"),
    "facturas/pdf.py": text(
      '"""Factura en PDF."""\n\nfrom reportlab.pdfgen import canvas\n\nfrom facturas.totales import total\n\n\ndef generar_pdf(lineas, destino):\n    hoja = canvas.Canvas(destino)\n    hoja.drawString(72, 720, f"Total: {total(lineas)}")\n    hoja.save()\n',
      "5c81d02",
    ),
    "notificaciones/__init__.py": text("", "2e4b7a9"),
    "notificaciones/recordatorios.py": text(
      '"""Recordatorios antes de cada cita."""\n\nfrom datetime import timedelta\n\nfrom notificaciones.whatsapp import enviar_whatsapp\n\nPENDIENTES = []\n\n\ndef programar_recordatorio(cita, horas=24):\n    cuando = cita.hora - timedelta(hours=horas)\n    PENDIENTES.append((cuando, cita))\n\n\ndef enviar_pendientes(ahora):\n    for cuando, cita in list(PENDIENTES):\n        if cuando <= ahora:\n            enviar_whatsapp(cita.cliente, f"Te esperamos a las {cita.hora:%H:%M}")\n            PENDIENTES.remove((cuando, cita))\n',
      "7bd20aa",
      "2e4b7a9",
    ),
    "notificaciones/whatsapp.py": text(
      '"""Envío por la API de WhatsApp Business."""\n\nimport httpx\n\nfrom config.settings import WHATSAPP_TOKEN\n\nURL = "https://graph.facebook.com/v21.0/mensajes"\n\n\ndef enviar_whatsapp(telefono: str, texto: str) -> bool:\n    respuesta = httpx.post(\n        URL,\n        headers={"Authorization": f"Bearer {WHATSAPP_TOKEN}"},\n        json={"to": telefono, "text": texto},\n        timeout=10,\n    )\n    return respuesta.is_success\n',
      "a3f9c1e",
      "2e4b7a9",
    ),
    "tests/__init__.py": text("", "2e4b7a9"),
    "tests/conftest.py": text(
      'import pytest\n\nfrom citas.modelos import AGENDA\n\n\n@pytest.fixture(autouse=True)\ndef agenda_vacia():\n    AGENDA.clear()\n',
      "2e4b7a9",
    ),
    "tests/test_citas.py": text(
      'from datetime import datetime\n\nfrom citas.servicio import crear_cita\n\n\ndef test_crear_cita():\n    cita = crear_cita("Ana", "corte", datetime(2026, 11, 20, 10))\n    assert cita.cliente == "Ana"\n',
      "2e4b7a9",
    ),
    "tests/test_facturas.py": text(TEST_FACTURAS, "5c81d02"),
    "tests/test_recordatorios.py": text(
      'from datetime import datetime\n\nfrom citas.modelos import Cita\nfrom notificaciones.recordatorios import PENDIENTES, programar_recordatorio\n\n\ndef test_recordatorio_24_horas_antes():\n    cita = Cita.nueva("Ana", "corte", datetime(2026, 11, 20, 10))\n    programar_recordatorio(cita)\n    assert PENDIENTES[-1][0] == datetime(2026, 11, 19, 10)\n',
      "7bd20aa",
      "2e4b7a9",
    ),
    "docs/arquitectura.md": text(
      "# Arquitectura\n\n## Citas\n\nUna cita se guarda y programa su recordatorio.\n\n## Notificaciones\n\nLos recordatorios salen por WhatsApp 24 h antes.\n\n## Facturas\n\nEl IVA es del 19 %.\n",
      "7bd20aa",
      "0f12ab3",
    ),
    "docs/facturacion.md": text(
      "# Facturación\n\n## Impuestos\n\nEl IVA se calcula sobre el total de la factura.\n\n## PDF\n\nCada factura se puede descargar en PDF.\n",
      "5c81d02",
    ),
    "scripts/migrar.sh": text(
      '#!/usr/bin/env bash\nset -euo pipefail\n\nuv run python -m citas.migraciones "$@"\n',
      "0f12ab3",
    ),
    "static/logo.png": { kind: "image", mime: "image/png", base64: PIXEL, commits: ["0f12ab3"] },
    "static/fuentes/Inter.woff2": { kind: "binary", size: 48_212, commits: ["0f12ab3"] },
    "data/clientes_demo.csv": { kind: "large", size: 3_412_880, commits: ["2e4b7a9"] },
  },
  folderChanges: {
    "README.md": README.replace(
      "## Estructura",
      "## Variables\n\nCopia `.env.example` a `.env` y completa `WHATSAPP_TOKEN`.\n\n## Estructura",
    ),
  },
  untracked: {
    "docs/borrador.md": "# Borrador\n\nIdeas para la agenda online:\n\n- Pagos por adelantado.\n- Lista de espera.\n",
  },
  ignored: [".venv/", "__pycache__/", ".env"],
  tasks: {
    // "Arregla el cálculo de impuestos en las facturas": the VAT is rounded per line.
    s5: {
      "facturas/impuestos.py": IMPUESTOS.replace(
        "    return precio * cantidad * IVA\n",
        "    neto = precio * cantidad\n    # Se redondea por línea, como exige el SII.\n    return redondear(neto * IVA)\n",
      ),
      "facturas/totales.py": TOTALES.replace(
        "    return redondear(subtotal(lineas) * (1 + IVA))\n",
        "    neto = subtotal(lineas)\n    impuesto = iva_total(lineas)\n    return neto + impuesto\n",
      ),
      "tests/test_facturas.py": TEST_FACTURAS.replace(
        '    assert total(lineas) == Decimal("1215.58")\n',
        '    # 188.10 + 5.99: el IVA se redondea por línea.\n    assert subtotal(lineas) == Decimal("1021.50")\n    assert total(lineas) == Decimal("1215.59")\n',
      ),
    },
  },
};

const CARRITO = `import type { Linea, Producto } from "./tipos";
import { aplicarDescuento } from "./descuentos";

export class Carrito {
  private lineas: Linea[] = [];

  agregar(producto: Producto, cantidad = 1): void {
    const linea = this.lineas.find((l) => l.producto.id === producto.id);
    if (linea) linea.cantidad += cantidad;
    else this.lineas.push({ producto, cantidad });
  }

  quitar(id: string): void {
    this.lineas = this.lineas.filter((l) => l.producto.id !== id);
  }

  subtotal(): number {
    return this.lineas.reduce((suma, l) => suma + l.producto.precio * l.cantidad, 0);
  }

  total(codigo?: string): number {
    return aplicarDescuento(this.subtotal(), codigo);
  }
}
`;

export const HOMMING_REPO: SampleRepo = {
  branch: "main",
  commits: [
    { hash: "c41e9b2", author: "Camila Rojas", date: T0 - 1 * DAY, subject: "Descuentos por código" },
    { hash: "98d0f5a", author: "Diego Soto", date: T0 - 6 * DAY, subject: "Reporte de comisiones" },
    { hash: "1a7c3e4", author: "Camila Rojas", date: T0 - 20 * DAY, subject: "Carrito" },
  ],
  files: {
    ".env.example": { kind: "locked", commits: ["1a7c3e4"] },
    ".gitignore": text("node_modules/\ndist/\n.env\n", "1a7c3e4"),
    "README.md": text("# Panel Homming\n\nCarrito y reportes para la tienda.\n\n## Desarrollo\n\n```sh\nnpm install\nnpm test\n```\n", "1a7c3e4"),
    "package.json": text(
      '{\n  "name": "panel-homming",\n  "private": true,\n  "type": "module",\n  "scripts": {\n    "dev": "vite",\n    "test": "vitest run"\n  }\n}\n',
      "98d0f5a",
      "1a7c3e4",
    ),
    "tsconfig.json": text('{\n  "compilerOptions": {\n    "strict": true,\n    "target": "ES2022"\n  }\n}\n', "1a7c3e4"),
    "vite.config.ts": text('import { defineConfig } from "vite";\n\nexport default defineConfig({});\n', "1a7c3e4"),
    "src/main.ts": text('import { montarCarrito } from "./ui/Carrito";\n\nmontarCarrito(document.getElementById("app")!);\n', "1a7c3e4"),
    "src/api/cliente.ts": text(
      'const BASE = "/api/v2";\n\nexport async function pedir<T>(ruta: string): Promise<T> {\n  const respuesta = await fetch(`${BASE}${ruta}`);\n  if (!respuesta.ok) throw new Error(`HTTP ${respuesta.status}`);\n  return respuesta.json() as Promise<T>;\n}\n',
      "98d0f5a",
    ),
    "src/carrito/carrito.ts": text(CARRITO, "c41e9b2", "1a7c3e4"),
    "src/carrito/descuentos.ts": text(
      'const CODIGOS: Record<string, number> = { BIENVENIDA: 0.1, VERANO: 0.15 };\n\nexport function aplicarDescuento(monto: number, codigo?: string): number {\n  const tasa = codigo ? (CODIGOS[codigo.toUpperCase()] ?? 0) : 0;\n  return Math.round(monto * (1 - tasa));\n}\n',
      "c41e9b2",
    ),
    "src/carrito/tipos.ts": text(
      "export interface Producto {\n  id: string;\n  nombre: string;\n  precio: number;\n}\n\nexport interface Linea {\n  producto: Producto;\n  cantidad: number;\n}\n",
      "1a7c3e4",
    ),
    "src/comisiones/api.ts": text(
      'import { pedir } from "../api/cliente";\n\nexport interface Venta {\n  vendedor: string;\n  monto: number;\n}\n\nexport const ventasDelMes = (mes: string) => pedir<Venta[]>(`/ventas?mes=${mes}`);\n',
      "98d0f5a",
    ),
    "src/comisiones/reporte.ts": text(
      'import { ventasDelMes, type Venta } from "./api";\n\nconst TASA = 0.05;\n\nexport function comisiones(ventas: Venta[]): Map<string, number> {\n  const porVendedor = new Map<string, number>();\n  for (const venta of ventas) {\n    porVendedor.set(venta.vendedor, (porVendedor.get(venta.vendedor) ?? 0) + venta.monto * TASA);\n  }\n  return porVendedor;\n}\n\nexport async function reporteDelMes(mes: string) {\n  return comisiones(await ventasDelMes(mes));\n}\n',
      "98d0f5a",
    ),
    "src/ui/Boton.tsx": text(
      'export function Boton({ texto, onClick }: { texto: string; onClick: () => void }) {\n  return <button onClick={onClick}>{texto}</button>;\n}\n',
      "1a7c3e4",
    ),
    "src/ui/Carrito.tsx": text(
      'import { Carrito } from "../carrito/carrito";\n\nconst carrito = new Carrito();\n\nexport function montarCarrito(raiz: HTMLElement) {\n  raiz.textContent = `Total: ${carrito.total()}`;\n}\n',
      "1a7c3e4",
    ),
    "tests/carrito.test.ts": text(
      'import { describe, expect, it } from "vitest";\nimport { Carrito } from "../src/carrito/carrito";\n\ndescribe("Carrito", () => {\n  it("suma las líneas", () => {\n    const carrito = new Carrito();\n    carrito.agregar({ id: "a", nombre: "Taza", precio: 4990 }, 2);\n    expect(carrito.subtotal()).toBe(9980);\n  });\n});\n',
      "1a7c3e4",
    ),
    "tests/descuentos.test.ts": text(
      'import { expect, it } from "vitest";\nimport { aplicarDescuento } from "../src/carrito/descuentos";\n\nit("aplica BIENVENIDA", () => {\n  expect(aplicarDescuento(10000, "bienvenida")).toBe(9000);\n});\n',
      "c41e9b2",
    ),
    "tests/comisiones.test.ts": text(
      'import { expect, it } from "vitest";\nimport { comisiones } from "../src/comisiones/reporte";\n\nit("calcula el 5 %", () => {\n  expect(comisiones([{ vendedor: "Ana", monto: 1000 }]).get("Ana")).toBe(50);\n});\n',
      "98d0f5a",
    ),
    "docs/api-comisiones.md": text("# API de comisiones\n\n## GET /api/v2/ventas\n\nVentas de un mes (`?mes=2026-11`).\n", "98d0f5a"),
    "public/favicon.png": { kind: "image", mime: "image/png", base64: PIXEL, commits: ["1a7c3e4"] },
    "public/fonts/Inter.woff2": { kind: "binary", size: 48_212, commits: ["1a7c3e4"] },
    "fixtures/ventas-2025.json": { kind: "large", size: 2_960_114, commits: ["98d0f5a"] },
  },
  folderChanges: {},
  untracked: { "notas.txt": "Revisar el redondeo de las comisiones.\n" },
  ignored: ["node_modules/", "dist/", ".env"],
  tasks: {},
};

export const SAMPLE_REPOS: Record<string, SampleRepo> = {
  "/code/hedy": HEDY_REPO,
  "/code/homming": HOMMING_REPO,
};
