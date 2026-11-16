// English text for the sample data, only while capturing the intro video and
// the README stills. The app's own sample data stays as it is: this overlay is
// applied by `vite.config.mjs` when the dev server reads the mock files.
//
// Each entry is [file suffix, exact source text, replacement]; every
// occurrence is replaced, in order. A text that is no longer in the source is
// reported as a warning, so an outdated overlay is easy to spot.

const IMPUESTOS_EN = `"""Invoice taxes."""

from decimal import ROUND_HALF_UP, Decimal

VAT = Decimal("0.19")
CENT = Decimal("0.01")


def round_cents(value: Decimal) -> Decimal:
    """Round to the cent, half up."""
    return value.quantize(CENT, rounding=ROUND_HALF_UP)


def line_vat(price: Decimal, qty: int) -> Decimal:
    """VAT of one invoice line."""
    return price * qty * VAT


def total_vat(lines: list[tuple[Decimal, int]]) -> Decimal:
    """VAT of the whole invoice."""
    return round_cents(sum(line_vat(p, q) for p, q in lines))
`;

const TOTALES_EN = `"""Invoice totals."""

from decimal import Decimal

from invoices.tax import VAT, round_cents, total_vat


def subtotal(lines: list[tuple[Decimal, int]]) -> Decimal:
    """Price times quantity, before tax."""
    return round_cents(sum(p * q for p, q in lines))


def total(lines: list[tuple[Decimal, int]]) -> Decimal:
    """Subtotal plus VAT."""
    return round_cents(subtotal(lines) * (1 + VAT))
`;

const TEST_EN = `from decimal import Decimal

from invoices.totals import subtotal, total


def test_subtotal():
    assert subtotal([(Decimal("1000"), 2)]) == Decimal("2000.00")


def test_total_with_vat():
    lines = [(Decimal("990"), 1), (Decimal("10.50"), 3)]
    assert total(lines) == Decimal("1215.58")
`;

const SAMPLES = "api/mockCodeSamples.ts";
const MOCK = "api/MockApi.ts";

/** Replaces a template literal's body, keeping the `const NAME = ` before it. */
function body(name, text) {
  return [SAMPLES, new RegExp(`const ${name} = \`[\\s\\S]*?\`;`), `const ${name} = \`${text}\`;`];
}

export const overlay = [
  body("IMPUESTOS", IMPUESTOS_EN),
  body("TOTALES", TOTALES_EN),
  body("TEST_FACTURAS", TEST_EN),
  // The sample task's change, on the English code.
  [SAMPLES, '"    return precio * cantidad * IVA\\n"', '"    return price * qty * VAT\\n"'],
  [
    SAMPLES,
    '"    neto = precio * cantidad\\n    # Se redondea por línea, como exige el SII.\\n    return redondear(neto * IVA)\\n"',
    '"    net = price * qty\\n    # Round each line, as the tax office requires.\\n    return round_cents(net * VAT)\\n"',
  ],
  [SAMPLES, '"    return redondear(subtotal(lineas) * (1 + IVA))\\n"', '"    return round_cents(subtotal(lines) * (1 + VAT))\\n"'],
  [
    SAMPLES,
    '"    neto = subtotal(lineas)\\n    impuesto = iva_total(lineas)\\n    return neto + impuesto\\n"',
    '"    net = subtotal(lines)\\n    tax = total_vat(lines)\\n    return net + tax\\n"',
  ],
  [SAMPLES, `'    assert total(lineas) == Decimal("1215.58")\\n'`, `'    assert total(lines) == Decimal("1215.58")\\n'`],
  [
    SAMPLES,
    `'    # 188.10 + 5.99: el IVA se redondea por línea.\\n    assert subtotal(lineas) == Decimal("1021.50")\\n    assert total(lineas) == Decimal("1215.59")\\n'`,
    `'    # 188.10 + 5.99: VAT is rounded per line.\\n    assert subtotal(lines) == Decimal("1021.50")\\n    assert total(lines) == Decimal("1215.59")\\n'`,
  ],
  [SAMPLES, "from facturas.totales import total", "from invoices.totals import total"],
  [SAMPLES, '"facturas/impuestos.py"', '"invoices/tax.py"'],
  [SAMPLES, '"facturas/totales.py"', '"invoices/totals.py"'],
  [SAMPLES, '"facturas/__init__.py"', '"invoices/__init__.py"'],
  [SAMPLES, '"facturas/pdf.py"', '"invoices/pdf.py"'],
  [SAMPLES, '"tests/test_facturas.py"', '"tests/test_invoices.py"'],
  [SAMPLES, "- \\`facturas/\\`: impuestos, totales y PDF.", "- \\`invoices/\\`: taxes, totals and PDF."],
  [SAMPLES, '"Arreglo del envío"', '"Fix shipping"'],
  [SAMPLES, '"Plantillas de correo"', '"Email templates"'],
  [MOCK, '"Arreglo del envío"', '"Fix shipping"'],
  [MOCK, '"Plantillas de correo"', '"Email templates"'],
  [SAMPLES, '"Facturas con IVA"', '"Invoices with VAT"'],
  [SAMPLES, '"Citas y recordatorios"', '"Appointments and reminders"'],
  [SAMPLES, 'subject: "Inicio"', 'subject: "First commit"'],
  [SAMPLES, '"Descuentos por código"', '"Discount codes"'],
  [SAMPLES, '"Reporte de comisiones"', '"Commissions report"'],
  [SAMPLES, 'subject: "Carrito"', 'subject: "Cart"'],

  // Accounts, tasks, queues and conversations.
  [MOCK, '"Claude trabajo"', '"Claude work"'],
  [MOCK, '"Agrega recordatorios por WhatsApp antes de cada cita"', '"Send WhatsApp reminders before each appointment"'],
  [MOCK, '"1. Buscar\\n2. Programar\\n3. Tests"', '"1. Find\\n2. Schedule\\n3. Tests"'],
  [MOCK, '"Migra el reporte de comisiones a la API nueva"', '"Move the commissions report to the new API"'],
  [MOCK, '"Traduce los correos de confirmación"', '"Translate the confirmation emails"'],
  [MOCK, '"Revisa los tests del carrito"', '"Check the cart tests"'],
  [MOCK, 'summary: "carrito"', 'summary: "cart"'],
  [MOCK, '"Cuando termines, corre también los tests del checkout."', '"When you finish, run the checkout tests too."'],
  [MOCK, '"Y deja un resumen corto de lo que cambiaste."', '"And leave a short summary of what you changed."'],
  [MOCK, '"Revisa también el asunto del correo."', '"Check the email subject too."'],
  [MOCK, '"Arregla el cálculo de impuestos en las facturas"', '"Fix the tax calculation on invoices"'],
  [MOCK, '"hedy/arregla-el-calculo-de-impuestos-s5"', '"hedy/fix-invoice-tax-s5"'],
  [MOCK, '"facturas/impuestos.py"', '"invoices/tax.py"'],
  [MOCK, '"pytest tests/test_facturas.py"', '"pytest tests/test_invoices.py"'],
  [MOCK, '"El IVA se redondea por línea y no al total."', '"VAT is now rounded per line, not on the total."'],
  [MOCK, '"¿Qué diferencia hay entre OAuth y OpenID Connect?"', '"OAuth vs OpenID Connect?"'],
  [
    MOCK,
    '"OAuth 2.0 autoriza: da a una app acceso a algo tuyo. OpenID Connect va encima y autentica: dice quién eres, con un ID token."',
    '"OAuth 2.0 authorizes: it gives an app access to something of yours. OpenID Connect sits on top and authenticates: it says who you are, with an ID token."',
  ],
  [MOCK, '"¿Dónde se calculan las comisiones?"', '"Where are commissions calculated?"'],
  [MOCK, '"Actualiza las dependencias del panel"', '"Update the dashboard dependencies"'],
  [MOCK, '"npm install falló"', '"npm install failed"'],
  [MOCK, '"Cambia el color del botón de reservar"', '"Change the color of the booking button"'],
  [MOCK, '"Voy a revisar cómo se calcula el envío del carrito."', '"Let me check how cart shipping is calculated."'],
  [MOCK, "`Hecho: ${text}`", '"On it: adding a test with a three-line invoice."'],
  [MOCK, '"Revisar los casos de borde del envío"', '"Check the shipping edge cases"'],
  [MOCK, '"docs de la librería de pagos"', '"payments library docs"'],

  // Notes and skills waiting for approval (the task board).
  [MOCK, '"Los formularios validan con zod"', '"Forms validate with zod"'],
  [
    MOCK,
    '"Las citas usan la zona horaria del salón; el servidor guarda las fechas en UTC"',
    '"Appointments use the salon\'s time zone; the server stores UTC"',
  ],
  [MOCK, '"Los precios se guardan en USD"', '"Prices are stored in USD"'],
  [MOCK, '"agregar-endpoint"', '"add-endpoint"'],
  [MOCK, '"Cómo agregar un endpoint nuevo a la API de citas."', '"How to add an endpoint to the appointments API."'],
  [MOCK, '"correr-tests"', '"run-tests"'],
  [
    MOCK,
    '"Correr los tests del backend, del frontend y los e2e antes de entregar una tarea."',
    '"Run the backend, frontend and e2e tests before handing in a task."',
  ],
  [
    MOCK,
    '"Correr los tests del backend y del frontend antes de entregar una tarea."',
    '"Run the backend and frontend tests before handing in a task."',
  ],
];

/** Applies the overlay to one module's source; `warn` gets the texts not found. */
export function applyOverlay(id, code, warn) {
  let out = code;
  for (const [suffix, from, to] of overlay) {
    if (!id.endsWith(suffix)) continue;
    const found = typeof from === "string" ? out.includes(from) : from.test(out);
    if (!found) warn(`sample-en: not found in ${suffix}: ${String(from).slice(0, 70)}`);
    out = typeof from === "string" ? out.split(from).join(to) : out.replace(from, () => to);
  }
  return out;
}
