import unittest
from datetime import date

from cowork import billing, extras, members, rooms
from tests.helpers import fresh_store


class InvoiceTest(unittest.TestCase):
    def setUp(self):
        self.store = fresh_store(self)

    def lines(self, item):
        return [(line.description, line.cents) for line in item.lines]

    def test_full_month_with_vat(self):
        item = billing.invoice(self.store, "S001", "2025-03")
        self.assertEqual(item.number, "F-2025-0001")
        self.assertEqual(self.lines(item), [("Plan Fijo — marzo 2025", 25000)])
        self.assertEqual((item.subtotal_cents, item.vat_cents, item.total_cents), (25000, 5250, 30250))

    def test_prorated(self):
        item = billing.invoice(self.store, "S002", "2025-03")
        self.assertEqual(self.lines(item)[0], ("Plan Flex — marzo 2025 (22 de 31 días)", 10645))

    def test_discount_hours_and_extras(self):
        members.set_discount(self.store, "S002", "AMIGO10")
        rooms.book(self.store, "S002", "azul", date(2025, 3, 11), "09:00", "13:00")
        rooms.book(self.store, "S002", "verde", date(2025, 3, 12), "10:00", "10:30")
        extras.consume(self.store, "S002", "impresion", 3, date(2025, 3, 12))
        extras.consume(self.store, "S002", "cafe", 2, date(2025, 3, 13))
        extras.consume(self.store, "S002", "cafe", 1, date(2025, 3, 14))
        self.assertEqual(self.lines(billing.invoice(self.store, "S002", "2025-03")), [
            ("Plan Flex — marzo 2025 (22 de 31 días)", 10645),
            ("Descuento AMIGO10 (10%)", -1065),
            ("Horas extra de sala: 1 h", 1500),
            ("Café x 3", 360),
            ("Impresión x 3", 30),
        ])

    def test_refusals(self):
        billing.invoice(self.store, "S001", "2025-03")
        with self.assertRaisesRegex(ValueError, "Ya existe una factura de S001 para 2025-03"):
            billing.invoice(self.store, "S001", "2025-03")
        with self.assertRaisesRegex(ValueError, "S004 no estuvo activo en 2025-03"):
            billing.invoice(self.store, "S004", "2025-03")
        self.assertEqual(billing.invoice(self.store, "S003", "2025-03").number, "F-2025-0002")

    def test_invoice_all(self):
        made = billing.invoice_all(self.store, "2025-03")
        self.assertEqual([i.member_id for i in made], ["S001", "S002", "S003"])
        self.assertEqual(billing.invoice_all(self.store, "2025-03"), [])
        self.assertEqual(self.store.outbox[-1]["subject"], f"Factura {made[-1].number}")

    def test_text_and_email(self):
        from cowork import messages

        members.set_discount(self.store, "S001", "AMIGO10")
        extras.consume(self.store, "S001", "cafe", 12, date(2025, 3, 10))
        item = billing.invoice(self.store, "S001", "2025-03")
        text = billing.invoice_text(self.store, item)
        self.assertTrue(text.startswith("Factura F-2025-0001\nSocio: Ana Ruiz (S001)\nPeriodo: marzo 2025\n"))
        self.assertTrue(text.endswith("Subtotal: 239,40 €\nIVA 21%: 50,27 €\nTotal: 289,67 €"))
        self.assertEqual(
            messages.invoice_email(self.store, item.number),
            "Hola Ana, tu factura F-2025-0001 de marzo 2025 por 289,67 € ya está disponible.",
        )


class CreditNotesTest(unittest.TestCase):
    def setUp(self):
        self.store = fresh_store(self)
        self.item = billing.invoice(self.store, "S001", "2025-03")

    def test_note_and_month_total(self):
        note = billing.credit_note(self.store, self.item.number, 5000, "Error en horas de sala")
        self.assertEqual((note.number, note.total_cents), ("NC-2025-0001", -5000))
        self.assertEqual(
            billing.credit_note_text(note),
            "Nota de crédito NC-2025-0001\nFactura: F-2025-0001\nMotivo: Error en horas de sala\nTotal: -50,00 €",
        )
        self.assertEqual(billing.month_total(self.store, "2025-03"), 25250)

    def test_limits(self):
        with self.assertRaisesRegex(ValueError, "No existe la factura F-2025-0009"):
            billing.credit_note(self.store, "F-2025-0009", 100, "x")
        billing.credit_note(self.store, self.item.number, 30000, "Casi todo")
        with self.assertRaisesRegex(ValueError, "La nota supera lo facturado en F-2025-0001"):
            billing.credit_note(self.store, self.item.number, 251, "Más")

    def test_payments_survive_a_reload(self):
        from cowork.storage import Store

        billing.mark_paid(self.store, self.item.number, date(2025, 4, 5))
        billing.invoice(self.store, "S003", "2025-03")
        self.store.save()
        store = Store.load(self.store.folder)
        self.assertEqual(billing.pending(store), ["F-2025-0002"])
        self.assertEqual(billing.find_invoice(store, "F-2025-0001").vat_cents, 5250)


class ChangeTest(unittest.TestCase):
    def test_numbering_per_year_and_vat_per_line(self):
        store = fresh_store(self)
        billing.invoice(store, "S001", "2025-03")
        self.assertEqual(billing.invoice(store, "S001", "2026-01").number, "F-2026-0001")
        members.set_discount(store, "S003", "ANUAL15")
        item = billing.invoice(store, "S003", "2025-03")
        # 9000 -> 1890; -1350 -> -283.5 -> -284
        self.assertEqual(item.vat_cents, 1606)
