import csv
import io
import unittest

from hidden_workshop import MONDAY, SATURDAY, cli, fresh

REMINDER = ("Hola Laura, te esperamos el 10/03 a las 09:00 para cambio de aceite (patente AB123CD). "
            "Si no puedes venir, avísanos con {hours} horas de anticipación.")


class Base(unittest.TestCase):
    def setUp(self):
        self.shop, self.path = fresh()
        from taller.errors import BookingError, ShopError

        self.BookingError, self.ShopError = BookingError, ShopError

    def book(self, plate, code, start, day=MONDAY):
        return self.shop.book(plate, code, day, start)

    def refused(self, message, plate, code, start, day=MONDAY, error=None):
        with self.assertRaises(error or self.BookingError) as caught:
            self.book(plate, code, start, day)
        self.assertEqual(str(caught.exception), message)

    def refused_call(self, message, call, *args):
        with self.assertRaises(self.BookingError) as caught:
            call(*args)
        self.assertEqual(str(caught.exception), message)


class R01(Base):
    def test_an_appointment(self):
        a = self.book("AB123CD", "OIL", "09:00")
        self.assertEqual((a.id, a.plate, a.service, a.mechanic), (1, "AB123CD", "OIL", "M1"))
        self.assertEqual((a.day, a.start, a.end, a.status), (MONDAY, "09:00", "09:30", "reservado"))
        b = self.book("AC456EF", "ALI", "10:00")
        self.assertEqual((b.id, b.end), (2, "11:00"))
        self.assertEqual(self.book("KLM321", "OIL", "15:00").id, 3)
        self.assertEqual(self.shop.appointment(2).plate, "AC456EF")


class R02(Base):
    def test_opening_hours(self):
        self.refused("Fuera del horario del taller", "AB123CD", "OIL", "09:00", SATURDAY)
        self.refused("Fuera del horario del taller", "AB123CD", "OIL", "07:30")
        self.assertEqual(self.book("AB123CD", "OIL", "17:30").end, "18:00")
        self.refused("Fuera del horario del taller", "AC456EF", "ALI", "17:30")


class R03(Base):
    def test_lunch_break(self):
        self.assertEqual(self.book("AB123CD", "OIL", "12:30").end, "13:00")
        self.refused("Ese horario cruza la pausa de almuerzo (13:00–14:00)", "AC456EF", "ALI", "12:30")
        self.refused("Ese horario cruza la pausa de almuerzo (13:00–14:00)", "AC456EF", "OIL", "13:00")
        self.assertEqual(self.book("AC456EF", "OIL", "14:00").start, "14:00")


class R03Changed(Base):
    def test_lunch_break_moved(self):
        self.assertEqual(self.book("AB123CD", "OIL", "13:00").end, "13:30")
        self.assertEqual(self.book("AC456EF", "ALI", "12:30").end, "13:30")
        self.refused("Ese horario cruza la pausa de almuerzo (13:30–14:30)", "KLM321", "OIL", "13:30")
        self.refused("Ese horario cruza la pausa de almuerzo (13:30–14:30)", "KLM321", "OIL", "14:00")
        self.assertEqual(self.book("KLM321", "OIL", "14:30").start, "14:30")


class R04(Base):
    def test_every_half_hour(self):
        self.refused("Los turnos empiezan cada 30 minutos", "AB123CD", "OIL", "09:15")
        self.assertEqual(self.book("AB123CD", "OIL", "09:30").start, "09:30")


class R05(Base):
    def test_services_and_skills(self):
        self.refused("Servicio desconocido: XYZ", "AB123CD", "XYZ", "09:00")
        self.refused("Ningún mecánico hace ese servicio", "AB123CD", "AIR", "09:00")
        self.assertEqual(self.book("AB123CD", "ELE", "09:00").mechanic, "M3")


class R06(Base):
    def test_first_free_mechanic(self):
        self.assertEqual(self.book("AB123CD", "OIL", "09:00").mechanic, "M1")
        self.assertEqual(self.book("AC456EF", "OIL", "09:00").mechanic, "M2")
        self.refused("No hay mecánicos libres en ese horario", "KLM321", "OIL", "09:00")
        self.assertEqual(self.book("KLM321", "OIL", "09:30").mechanic, "M1")


class R07(Base):
    def test_one_car_one_place(self):
        self.book("AB123CD", "OIL", "09:00")
        self.refused("Ese auto ya tiene un turno en ese horario", "AB123CD", "ALI", "09:00")
        self.assertEqual(self.book("AB123CD", "ALI", "09:30").start, "09:30")


class R08(Base):
    def test_registered_vehicle(self):
        self.refused("Vehículo no registrado: AB999ZZ", "AB999ZZ", "OIL", "09:00")
        self.refused("Vehículo no registrado: AB999ZZ", "ab-999-zz", "OIL", "09:00")


class R09(Base):
    def test_plates(self):
        vehicle = self.shop.customers.add_vehicle("ae-111-bb", "C1", "Toyota", "Etios", 2019)
        self.assertEqual(vehicle.plate, "AE111BB")
        self.assertEqual(self.shop.customers.vehicle("kl m-321").plate, "KLM321")
        with self.assertRaises(self.ShopError) as caught:
            self.shop.customers.add_vehicle("XX", "C1", "Toyota", "Etios", 2019)
        self.assertEqual(str(caught.exception), "Patente inválida")
        self.assertEqual(self.book("AB-123-CD", "OIL", "09:00").plate, "AB123CD")
        self.refused("Patente inválida", "1234", "OIL", "09:00", error=self.ShopError)


class R10(Base):
    def test_customer_phone(self):
        self.refused("El cliente no tiene teléfono para avisarle", "AD789GH", "OIL", "09:00")
        self.shop.customers.update_contact("C3", phone="1155550003")
        self.assertEqual(self.book("AD789GH", "OIL", "09:00").plate, "AD789GH")


class R11(Base):
    def test_cancel(self):
        a = self.book("AB123CD", "OIL", "09:00")
        self.assertEqual(self.shop.cancel(a.id).status, "cancelado")
        self.assertEqual(self.shop.appointment(a.id).status, "cancelado")
        self.assertEqual(self.book("AC456EF", "OIL", "09:00").mechanic, "M1")
        self.refused_call("Turno inexistente", self.shop.cancel, a.id)
        self.refused_call("Turno inexistente", self.shop.cancel, 99)


class R12(Base):
    MORNING = ["08:00", "08:30", "09:00", "09:30", "10:00", "10:30", "11:00", "11:30"]

    def test_free_slots(self):
        slots = self.shop.free_slots(MONDAY, "ALI")
        self.assertEqual([s for s in slots if s < "12:00"], self.MORNING)
        self.assertEqual([s for s in slots if s >= "15:00"], ["15:00", "15:30", "16:00", "16:30", "17:00"])
        a = self.book("AB123CD", "ALI", "10:00")
        self.book("AC456EF", "ALI", "10:00")
        busy = [s for s in self.shop.free_slots(MONDAY, "ALI") if s < "12:00"]
        self.assertEqual(busy, ["08:00", "08:30", "09:00", "11:00", "11:30"])
        self.shop.cancel(a.id)
        again = [s for s in self.shop.free_slots(MONDAY, "ALI") if s < "12:00"]
        self.assertEqual(again, self.MORNING)
        self.assertEqual(self.shop.free_slots(SATURDAY, "ALI"), [])


class R13(Base):
    HOURS = 24

    def test_reminder(self):
        a = self.book("AB123CD", "OIL", "09:00")
        text = REMINDER.format(hours=self.HOURS)
        self.assertEqual(self.shop.reminder(a.id), text)
        self.assertEqual((self.shop.outbox[-1].to, self.shop.outbox[-1].text), ("1155550001", text))
        self.shop.cancel(a.id)
        self.refused_call("Turno inexistente", self.shop.reminder, a.id)


class R13Changed(R13):
    HOURS = 48


class R14(Base):
    def test_day_text(self):
        self.book("AB123CD", "OIL", "09:00")
        self.book("AC456EF", "ALI", "10:00")
        last = self.book("KLM321", "OIL", "10:00")
        self.assertEqual(self.shop.day_text(MONDAY), "\n".join([
            "Agenda del taller — 10/03/2025",
            "Rosa Paz:",
            "  09:00–09:30 AB123CD cambio de aceite",
            "  10:00–11:00 AC456EF alineación y balanceo",
            "Juan Ibáñez:",
            "  10:00–10:30 KLM321 cambio de aceite",
            "Ana Torres:",
            "  (libre)",
        ]))
        self.shop.cancel(last.id)
        self.assertIn("Juan Ibáñez:\n  (libre)", self.shop.day_text(MONDAY))


class R15(Base):
    def test_parts_reserved(self):
        self.book("AB123CD", "BRK", "08:00")
        self.assertEqual(self.shop.inventory.available("PAS-DEL"), 0)
        self.assertEqual(self.shop.inventory.stock("PAS-DEL"), 1)
        self.refused("Sin stock de pastillas delanteras para ese servicio", "AC456EF", "BRK", "10:00")
        self.assertEqual(self.shop.inventory.available("FIL-ACE"), 6)


class R16(Base):
    def test_parts_released_or_taken(self):
        a = self.book("AB123CD", "BRK", "08:00")
        self.shop.cancel(a.id)
        self.assertEqual(self.shop.inventory.available("PAS-DEL"), 1)
        self.assertEqual(self.book("AC456EF", "BRK", "10:00").start, "10:00")
        b = self.book("AB123CD", "OIL", "15:00")
        order = self.shop.start_order(b.id)
        self.shop.close_order(order.id, "2025-03-10")
        self.assertEqual(self.shop.inventory.stock("FIL-ACE"), 5)
        self.assertEqual(self.shop.inventory.available("FIL-ACE"), 5)
        self.assertEqual(self.shop.inventory.stock("ACE-5W30"), 16)


class R17(Base):
    def test_low_stock_counts_reservations(self):
        codes = lambda: [p.code for p in self.shop.inventory.low_stock()]
        self.assertNotIn("FIL-ACE", codes())
        for plate, start in [("AB123CD", "09:00"), ("AC456EF", "09:00"), ("KLM321", "10:00"), ("AB123CD", "11:00")]:
            self.book(plate, "OIL", start)
        self.assertIn("FIL-ACE", codes())
        self.assertEqual(self.shop.inventory.stock("FIL-ACE"), 6)
        self.shop.save()
        code, out, _ = cli(self.path, "repuestos", "--bajo-stock")
        self.assertEqual(code, 0)
        self.assertIn("FIL-ACE", out)


class R18(Base):
    def test_start_order(self):
        a = self.book("AB123CD", "OIL", "09:00")
        order = self.shop.start_order(a.id)
        self.assertEqual((order.id, order.plate, order.services, order.mechanic, order.opened),
                         (5, "AB123CD", ["OIL"], "M1", "2025-03-10"))
        self.assertTrue(order.is_open)
        self.assertEqual((self.shop.appointment(a.id).status, self.shop.appointment(a.id).order), ("en_taller", 5))
        self.refused_call("El turno ya está en el taller", self.shop.cancel, a.id)
        self.refused_call("El turno ya está en el taller", self.shop.start_order, a.id)
        self.refused_call("Turno inexistente", self.shop.start_order, 99)


class R19(Base):
    def test_close_finishes_the_appointment(self):
        a = self.book("AB123CD", "OIL", "09:00")
        order = self.shop.start_order(a.id)
        self.shop.close_order(order.id, "2025-03-10")
        self.assertEqual(self.shop.appointment(a.id).status, "terminado")
        self.assertIn("ya está listo", self.shop.outbox[-1].text)
        self.refused_call("El turno ya terminó", self.shop.cancel, a.id)


class R20(Base):
    def test_saved_and_loaded(self):
        from taller.shop import Shop

        a = self.book("AB123CD", "OIL", "09:00")
        self.book("AC456EF", "BRK", "10:00")
        self.shop.cancel(a.id)
        self.shop.save()
        again = Shop.load(self.path)
        self.assertEqual(again.appointment(1).status, "cancelado")
        b = again.appointment(2)
        self.assertEqual((b.plate, b.service, b.mechanic, b.day, b.start, b.end, b.status),
                         ("AC456EF", "BRK", "M1", MONDAY, "10:00", "11:30", "reservado"))
        self.assertEqual(again.inventory.available("PAS-DEL"), 0)
        self.assertEqual(again.inventory.available("FIL-ACE"), 6)
        self.assertEqual(again.book("KLM321", "OIL", MONDAY, "09:00").id, 3)


class R21(Base):
    def test_cli(self):
        code, out, _ = cli(self.path, "turno", "AB123CD", "OIL", "2025-03-10", "09:00")
        self.assertEqual(code, 0)
        self.assertEqual(out.strip(), "Turno #1: 10/03 09:00–09:30, cambio de aceite con Rosa Paz")
        code, out, _ = cli(self.path, "agenda", "2025-03-10")
        self.assertEqual(code, 0)
        self.assertEqual(out.splitlines()[:3], ["Agenda del taller — 10/03/2025", "Rosa Paz:",
                                                "  09:00–09:30 AB123CD cambio de aceite"])
        code, out, err = cli(self.path, "turno", "AC456EF", "OIL", "2025-03-10", "09:15")
        self.assertEqual(code, 1)
        self.assertIn("Los turnos empiezan cada 30 minutos", err)
        code, out, _ = cli(self.path, "cancelar", "1")
        self.assertEqual((code, out.strip()), (0, "Turno #1 cancelado"))
        from taller.shop import Shop

        self.assertEqual(Shop.load(self.path).appointment(1).status, "cancelado")


class R22(Base):
    def test_appointment_quote(self):
        from taller import quotes

        a = self.book("AB123CD", "OIL", "09:00")
        quote = self.shop.appointment_quote(a.id)
        self.assertEqual(quotes.text(quote).splitlines()[0], "Presupuesto turno #1 — AB123CD")
        self.assertEqual(quote.total_cents, self.shop.quote("AB123CD", ["OIL"]).total_cents)
        self.assertEqual(quote.total_cents, 8651500)


class R23(Base):
    def test_frequent_customer(self):
        from taller import invoices

        quote = self.shop.quote("AC456EF", ["OIL"])
        lines = [(l.description, l.cents) for l in quote.lines]
        self.assertIn(("Descuento cliente frecuente (10%)", -150000), lines)
        self.assertEqual(quote.total_cents, 8470000)
        self.assertEqual(self.shop.quote("AB123CD", ["OIL"]).total_cents, 8651500)
        order = self.shop.open_order("KLM321", ["OIL"], "M2", "2025-03-10")
        self.assertEqual(self.shop.close_order(order.id, "2025-03-10").total_cents, 8470000)
        self.assertNotIn("Descuento", invoices.invoice_text(self.shop, 2))
        a = self.book("AC456EF", "OIL", "09:00")
        self.assertEqual(self.shop.appointment_quote(a.id).total_cents, 8470000)


class R24(Base):
    HEADER = ["id", "patente", "servicio", "mecanico", "inicio", "fin", "estado"]
    ROWS = [["2", "AC456EF", "ALI", "Rosa Paz", "08:00", "09:00", "reservado"],
            ["1", "AB123CD", "OIL", "Rosa Paz", "09:00", "09:30", "cancelado"]]

    def test_csv(self):
        a = self.book("AB123CD", "OIL", "09:00")
        self.book("AC456EF", "ALI", "08:00")
        self.shop.cancel(a.id)
        text = self.shop.appointments_csv(MONDAY)
        self.assertEqual(text.splitlines()[0], ",".join(self.HEADER))
        self.assertEqual(list(csv.reader(io.StringIO(text)))[1:], self.ROWS)


class R24Changed(R24):
    HEADER = R24.HEADER + ["cliente"]
    ROWS = [R24.ROWS[0] + ["Pedro Salas"], R24.ROWS[1] + ["Laura Gómez"]]


class REG(unittest.TestCase):
    def test_what_already_worked(self):
        from taller import export, history, invoices, quotes, reports

        shop, path = fresh()
        self.assertEqual([v.plate for v in shop.customers.vehicles_of("C2")], ["AC456EF", "KLM321"])
        self.assertEqual(shop.customers.vehicle(" ab123cd ").brand, "Fiat")
        self.assertEqual(shop.quote("AB123CD", ["OIL", "BRK"]).total_cents, 17968500)
        self.assertTrue(quotes.text(shop.quote("AB123CD", ["OIL"])).startswith("Presupuesto AB123CD"))
        order = shop.open_order("AB123CD", ["OIL"], "M1", "2025-03-10")
        self.assertEqual(shop.close_order(order.id, "2025-03-10").total_cents, 8651500)
        self.assertEqual(shop.inventory.stock("FIL-ACE"), 5)
        self.assertTrue(reports.monthly_text(shop.orders, "2025-02").startswith("Facturación de febrero 2025"))
        self.assertTrue(invoices.invoice_text(shop, 2).startswith("Factura T-0002 — 2025-02-18"))
        self.assertEqual(history.next_due(shop.orders, "AC456EF")["OIL"].isoformat(), "2025-08-03")
        self.assertIn("2,2025-02-18,AC456EF,Pedro Salas,BRK,93170.00", export.orders_csv(shop, "2025-02"))
        code, out, _ = cli(path, "mecanicos")
        self.assertEqual(code, 0)
        self.assertIn("M1  Rosa Paz: general, frenos", out)
        code, out, _ = cli(path, "repuestos", "--bajo-stock")
        self.assertIn("PAS-DEL", out)
        self.assertNotIn("FIL-ACE", out)


if __name__ == "__main__":
    unittest.main()
