import io
import unittest
from datetime import date

from taller import quotes
from taller.cli import main
from taller.errors import BookingError, ShopError
from taller.shop import Shop
from tests.helpers import fresh_shop

MONDAY = date(2025, 3, 10)


class BookingTest(unittest.TestCase):
    def setUp(self):
        self.shop, self.path = fresh_shop()

    def test_book_and_cancel(self):
        a = self.shop.book("ab 123 cd", "OIL", MONDAY, "09:00")
        self.assertEqual((a.id, a.plate, a.mechanic, a.end, a.status), (1, "AB123CD", "M1", "09:30", "reservado"))
        self.assertEqual(self.shop.inventory.available("FIL-ACE"), 5)
        self.shop.cancel(a.id)
        self.assertEqual(self.shop.inventory.available("FIL-ACE"), 6)
        self.assertEqual(self.shop.book("AB123CD", "OIL", MONDAY, "09:00").id, 2)

    def test_rules(self):
        with self.assertRaises(BookingError):
            self.shop.book("AB123CD", "OIL", MONDAY, "12:45")
        with self.assertRaises(BookingError):
            self.shop.book("AB123CD", "OIL", date(2025, 3, 8), "09:00")
        with self.assertRaises(ShopError):
            self.shop.book("A1", "OIL", MONDAY, "09:00")

    def test_slots_and_agenda(self):
        self.shop.book("AB123CD", "BRK", MONDAY, "08:00")
        self.assertEqual(self.shop.free_slots(MONDAY, "BRK")[0], "08:00")
        self.assertIn("Rosa Paz:", self.shop.day_text(MONDAY))

    def test_order_from_an_appointment(self):
        a = self.shop.book("AB123CD", "OIL", MONDAY, "09:00")
        order = self.shop.start_order(a.id)
        self.shop.close_order(order.id, "2025-03-10")
        self.assertEqual(self.shop.appointment(a.id).status, "terminado")
        self.assertEqual(self.shop.inventory.stock("FIL-ACE"), 5)
        self.assertEqual(self.shop.inventory.available("FIL-ACE"), 5)

    def test_saved_and_loaded(self):
        self.shop.book("AB123CD", "OIL", MONDAY, "09:00")
        self.shop.save()
        again = Shop.load(self.path)
        self.assertEqual(again.appointment(1).start, "09:00")
        self.assertEqual(again.inventory.available("FIL-ACE"), 5)

    def test_frequent_customer(self):
        quote = self.shop.quote("AC456EF", ["OIL"])
        self.assertIn("Descuento cliente frecuente (10%)", [l.description for l in quote.lines])
        self.assertNotIn("Descuento", quotes.text(self.shop.quote("AB123CD", ["OIL"])))

    def test_cli(self):
        out = io.StringIO()
        self.assertEqual(main(["--data", str(self.path), "turno", "AB123CD", "OIL", "2025-03-10", "09:00"], out=out), 0)
        self.assertIn("Turno #1", out.getvalue())


class MoreRulesTest(unittest.TestCase):
    def setUp(self):
        self.shop, self.path = fresh_shop()

    def message(self, *args):
        with self.assertRaises(ShopError) as caught:
            self.shop.book(*args)
        return str(caught.exception)

    def test_unknown_service_and_skill(self):
        self.assertEqual(self.message("AB123CD", "XYZ", MONDAY, "09:00"), "Servicio desconocido: XYZ")
        self.assertEqual(self.message("AB123CD", "AIR", MONDAY, "09:00"), "Ningún mecánico hace ese servicio")

    def test_mechanics_are_taken_in_order(self):
        self.assertEqual(self.shop.book("AB123CD", "OIL", MONDAY, "09:00").mechanic, "M1")
        self.assertEqual(self.shop.book("AC456EF", "OIL", MONDAY, "09:00").mechanic, "M2")
        self.assertEqual(self.message("KLM321", "OIL", MONDAY, "09:00"), "No hay mecánicos libres en ese horario")

    def test_same_car_twice(self):
        self.shop.book("AB123CD", "OIL", MONDAY, "09:00")
        self.assertEqual(self.message("AB123CD", "ALI", MONDAY, "09:00"), "Ese auto ya tiene un turno en ese horario")

    def test_vehicle_and_phone(self):
        self.assertEqual(self.message("AB999ZZ", "OIL", MONDAY, "09:00"), "Vehículo no registrado: AB999ZZ")
        self.assertEqual(self.message("AD789GH", "OIL", MONDAY, "09:00"), "El cliente no tiene teléfono para avisarle")

    def test_parts_run_out(self):
        self.shop.book("AB123CD", "BRK", MONDAY, "08:00")
        self.assertEqual(self.message("AC456EF", "BRK", MONDAY, "10:00"),
                         "Sin stock de pastillas delanteras para ese servicio")

    def test_cannot_cancel_twice_or_once_started(self):
        a = self.shop.book("AB123CD", "OIL", MONDAY, "09:00")
        self.shop.cancel(a.id)
        with self.assertRaises(BookingError):
            self.shop.cancel(a.id)
        b = self.shop.book("AB123CD", "OIL", MONDAY, "10:00")
        self.shop.start_order(b.id)
        with self.assertRaises(BookingError):
            self.shop.cancel(b.id)

    def test_low_stock_counts_reservations(self):
        for plate, start in [("AB123CD", "09:00"), ("AC456EF", "09:00"), ("KLM321", "10:00"), ("AB123CD", "11:00")]:
            self.shop.book(plate, "OIL", MONDAY, start)
        self.assertIn("FIL-ACE", [p.code for p in self.shop.inventory.low_stock()])

    def test_appointment_quote_title(self):
        a = self.shop.book("AB123CD", "OIL", MONDAY, "09:00")
        self.assertTrue(quotes.text(self.shop.appointment_quote(a.id)).startswith("Presupuesto turno #1 — AB123CD"))

    def test_plates_are_normalized(self):
        self.assertEqual(self.shop.customers.add_vehicle("ae-111-bb", "C1", "Toyota", "Etios", 2019).plate, "AE111BB")
        with self.assertRaises(ShopError):
            self.shop.customers.add_vehicle("XX", "C1", "Toyota", "Etios", 2019)


class AgendaAndStorageTest(unittest.TestCase):
    def setUp(self):
        self.shop, self.path = fresh_shop()

    def test_weekend_has_no_slots(self):
        self.assertEqual(self.shop.free_slots(date(2025, 3, 8), "OIL"), [])

    def test_cancelled_slots_come_back(self):
        a = self.shop.book("AB123CD", "ALI", MONDAY, "10:00")
        self.shop.book("AC456EF", "ALI", MONDAY, "10:00")
        self.assertNotIn("10:00", self.shop.free_slots(MONDAY, "ALI"))
        self.shop.cancel(a.id)
        self.assertIn("10:00", self.shop.free_slots(MONDAY, "ALI"))

    def test_reminder_goes_to_the_outbox(self):
        a = self.shop.book("AB123CD", "OIL", MONDAY, "09:00")
        self.assertEqual(self.shop.outbox[-1].to, "1155550001")
        self.assertEqual(self.shop.outbox[-1].text, self.shop.reminder(a.id))
        self.assertTrue(self.shop.reminder(a.id).startswith("Hola Laura, te esperamos el 10/03 a las 09:00"))

    def test_cancelled_ones_are_kept_and_numbering_goes_on(self):
        a = self.shop.book("AB123CD", "OIL", MONDAY, "09:00")
        self.shop.cancel(a.id)
        self.shop.save()
        again = Shop.load(self.path)
        self.assertEqual(again.appointment(a.id).status, "cancelado")
        self.assertEqual(again.book("AB123CD", "OIL", MONDAY, "09:00").id, 2)

    def test_cli_agenda_and_cancel(self):
        main(["--data", str(self.path), "turno", "AB123CD", "OIL", "2025-03-10", "09:00"], out=io.StringIO())
        out = io.StringIO()
        main(["--data", str(self.path), "agenda", "2025-03-10"], out=out)
        self.assertTrue(out.getvalue().startswith("Agenda del taller — 10/03/2025\nRosa Paz:"))
        out = io.StringIO()
        main(["--data", str(self.path), "cancelar", "1"], out=out)
        self.assertEqual(out.getvalue().strip(), "Turno #1 cancelado")
        self.assertEqual(Shop.load(self.path).appointment(1).status, "cancelado")


if __name__ == "__main__":
    unittest.main()
