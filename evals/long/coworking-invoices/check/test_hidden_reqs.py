"""Hidden checks: one class per requirement (R01…R24), REG for what worked at
the start, and R01Changed / R02Changed for the change of the third session."""
import csv
import io
from datetime import date

from hidden_cw import Case

MARCH = "2025-03"


class R01(Case):
    def test_one_series_across_years(self):
        self.assertEqual(self.invoice("S001").number, "F-2025-0001")
        self.assertEqual(self.invoice("S003").number, "F-2025-0002")
        self.assertEqual(self.invoice("S001", "2026-01").number, "F-2026-0003")
        self.assertEqual(self.invoice("S003", "2025-04").number, "F-2025-0004")


class R01Changed(Case):
    def test_each_year_starts_again(self):
        self.assertEqual(self.invoice("S001").number, "F-2025-0001")
        self.assertEqual(self.invoice("S003").number, "F-2025-0002")
        self.assertEqual(self.invoice("S001", "2026-01").number, "F-2026-0001")
        self.assertEqual(self.invoice("S003", "2026-01").number, "F-2026-0002")
        self.assertEqual(self.invoice("S003", "2025-04").number, "F-2025-0003")


class R02(Case):
    def test_vat_on_the_subtotal(self):
        item = self.invoice("S001")
        self.assertEqual((item.subtotal_cents, item.vat_cents, item.total_cents), (25000, 5250, 30250))

    def test_half_up(self):
        self.consume("S001", "impresion", 15)
        item = self.invoice("S001")
        # 25150 x 21 % = 5281.5
        self.assertEqual((item.subtotal_cents, item.vat_cents, item.total_cents), (25150, 5282, 30432))

    def test_on_the_subtotal_not_per_line(self):
        from cowork import members

        members.set_discount(self.store, "S001", "ANUAL15")
        item = self.invoice("S001")
        # 21250 x 21 % = 4462.5 -> 4463 (per line it would be 5250 - 788 = 4462)
        self.assertEqual((item.subtotal_cents, item.vat_cents, item.total_cents), (21250, 4463, 25713))


class R02Changed(Case):
    def test_vat_per_line(self):
        from cowork import members

        members.set_discount(self.store, "S001", "ANUAL15")
        item = self.invoice("S001")
        # 25000 -> 5250; -3750 -> -787.5 -> -788
        self.assertEqual((item.subtotal_cents, item.vat_cents, item.total_cents), (21250, 4462, 25712))

    def test_positive_lines(self):
        self.consume("S001", "cafe", 1)
        self.consume("S001", "impresion", 3)
        item = self.invoice("S001")
        # 5250 + 25 (25.2) + 6 (6.3) = 5281; on the subtotal 25150 it was 5282
        self.assertEqual((item.subtotal_cents, item.vat_cents, item.total_cents), (25150, 5281, 30431))


class R03(Case):
    def test_full_month(self):
        self.assertEqual(self.lines(self.invoice("S001"))[0], ("Plan Fijo — marzo 2025", 25000))

    def test_joined_in_the_month(self):
        self.assertEqual(
            self.lines(self.invoice("S002"))[0], ("Plan Flex — marzo 2025 (22 de 31 días)", 10645)
        )
        self.assertEqual(
            self.lines(self.invoice("S003", "2025-01"))[0],
            ("Plan Nómada — enero 2025 (17 de 31 días)", 4935),
        )

    def test_left_in_the_month(self):
        # S004 left on 28 February: all of February.
        self.assertEqual(self.lines(self.invoice("S004", "2025-02"))[0], ("Plan Fijo — febrero 2025", 25000))
        self.store.members[0].left = date(2025, 3, 20)
        self.assertEqual(
            self.lines(self.invoice("S001"))[0], ("Plan Fijo — marzo 2025 (20 de 31 días)", 16129)
        )


class R04(Case):
    def test_started_extra_hours(self):
        from cowork import rooms

        rooms.book(self.store, "S002", "azul", date(2025, 3, 11), "09:00", "13:00")
        rooms.book(self.store, "S002", "verde", date(2025, 3, 12), "10:00", "11:30")
        rooms.book(self.store, "S002", "verde", date(2025, 4, 2), "10:00", "12:00")
        lines = self.lines(self.invoice("S002"))
        self.assertEqual(lines[1], ("Horas extra de sala: 2 h", 3000))
        self.assertEqual(len(lines), 2)

    def test_within_the_plan(self):
        from cowork import rooms

        rooms.book(self.store, "S001", "azul", date(2025, 3, 11), "09:00", "12:00")
        self.assertEqual(len(self.invoice("S001").lines), 1)

    def test_cancelled_do_not_count(self):
        from cowork import rooms

        rooms.book(self.store, "S002", "azul", date(2025, 3, 11), "09:00", "13:00")
        extra = rooms.book(self.store, "S002", "verde", date(2025, 3, 12), "10:00", "12:00")
        extra.cancelled = True
        self.assertEqual(len(self.invoice("S002").lines), 1)


class R05(Case):
    def test_codes(self):
        from cowork import members

        members.set_discount(self.store, "S001", "AMIGO10")
        members.set_discount(self.store, "S002", "ANUAL15")
        self.assertEqual(self.lines(self.invoice("S001"))[1], ("Descuento AMIGO10 (10%)", -2500))
        # 10645 x 15 % = 1596.75
        self.assertEqual(self.lines(self.invoice("S002"))[1], ("Descuento ANUAL15 (15%)", -1597))

    def test_only_on_the_plan_and_removable(self):
        from cowork import members

        self.consume("S003", "taquilla", 1)
        members.set_discount(self.store, "S003", "AMIGO10")
        members.set_discount(self.store, "S001", "AMIGO10")
        members.set_discount(self.store, "S001", None)
        self.assertEqual(self.lines(self.invoice("S003"))[1], ("Descuento AMIGO10 (10%)", -900))
        self.assertEqual(len(self.invoice("S001").lines), 1)

    def test_unknown(self):
        from cowork import members

        self.raises("Código de descuento desconocido: GRATIS", members.set_discount, self.store, "S001", "GRATIS")


class R06(Case):
    def test_grouped_in_catalog_order(self):
        self.consume("S001", "impresion", 30, date(2025, 3, 11))
        self.consume("S001", "cafe", 2, date(2025, 3, 10))
        self.consume("S001", "cafe", 10, date(2025, 3, 12))
        self.consume("S001", "taquilla", 1, date(2025, 3, 1))
        self.consume("S001", "cafe", 4, date(2025, 4, 1))
        self.consume("S002", "cafe", 4, date(2025, 3, 12))
        lines = self.lines(self.invoice("S001"))
        self.assertEqual(
            lines[1:], [("Café x 12", 1440), ("Impresión x 30", 300), ("Taquilla x 1", 2000)]
        )

    def test_after_the_extra_hours(self):
        from cowork import rooms

        self.consume("S003", "cafe", 1)
        rooms.book(self.store, "S003", "azul", date(2025, 3, 11), "09:00", "10:00")
        lines = self.lines(self.invoice("S003"))
        self.assertEqual(lines[1:], [("Horas extra de sala: 1 h", 1500), ("Café x 1", 120)])


class R07(Case):
    def test_twice(self):
        from cowork import billing

        self.invoice("S001")
        self.raises("Ya existe una factura de S001 para 2025-03", billing.invoice, self.store, "S001", MARCH)

    def test_not_active(self):
        from cowork import billing

        self.raises("S004 no estuvo activo en 2025-03", billing.invoice, self.store, "S004", MARCH)
        self.raises("S002 no estuvo activo en 2025-02", billing.invoice, self.store, "S002", "2025-02")

    def test_rejected_spend_no_number(self):
        from cowork import billing

        self.invoice("S001")
        for args in (("S001", MARCH), ("S004", MARCH)):
            with self.assertRaises(ValueError):
                billing.invoice(self.store, *args)
        self.assertTrue(self.invoice("S003").number.endswith("0002"))
        self.assertEqual(len(self.store.invoices), 2)


class R08(Case):
    def test_all_active_without_invoice(self):
        from cowork import billing

        self.invoice("S002")
        made = billing.invoice_all(self.store, MARCH)
        self.assertEqual([i.member_id for i in made], ["S001", "S003"])
        self.assertEqual(billing.invoice_all(self.store, MARCH), [])
        self.assertEqual([i.member_id for i in billing.invoice_all(self.store, "2025-02")], ["S001", "S003", "S004"])


class R09(Case):
    def test_notes(self):
        from cowork import billing

        item = self.invoice("S001")
        note = billing.credit_note(self.store, item.number, 5000, "Error en horas de sala")
        self.assertEqual((note.number, note.invoice, note.total_cents, note.reason),
                         ("NC-2025-0001", item.number, -5000, "Error en horas de sala"))
        self.assertEqual(billing.credit_note(self.store, item.number, 25250, "Resto").number, "NC-2025-0002")

    def test_errors(self):
        from cowork import billing

        item = self.invoice("S001")
        self.raises("No existe la factura F-2025-0009", billing.credit_note, self.store, "F-2025-0009", 100, "x")
        billing.credit_note(self.store, item.number, 30000, "Casi todo")
        self.raises(f"La nota supera lo facturado en {item.number}",
                    billing.credit_note, self.store, item.number, 251, "Un poco más")


class R10(Case):
    def test_month_total(self):
        from cowork import billing

        first = self.invoice("S001")
        self.invoice("S003")
        april = self.invoice("S003", "2025-04")
        billing.credit_note(self.store, first.number, 5000, "Error")
        billing.credit_note(self.store, april.number, 1000, "Otro mes")
        self.assertEqual(billing.month_total(self.store, MARCH), 30250 + 10890 - 5000)
        self.assertEqual(billing.month_total(self.store, "2025-04"), 10890 - 1000)
        self.assertEqual(billing.month_total(self.store, "2025-05"), 0)


class R11(Case):
    def test_paid_and_pending(self):
        from cowork import billing

        numbers = [self.invoice(m).number for m in ("S001", "S002", "S003")]
        billing.mark_paid(self.store, numbers[1], date(2025, 4, 5))
        self.assertEqual(billing.pending(self.store), [numbers[0], numbers[2]])
        self.raises("No existe la factura F-2025-0009", billing.mark_paid, self.store, "F-2025-0009", date(2025, 4, 5))


class R12(Case):
    def test_saved_and_loaded(self):
        from cowork import billing, members

        members.set_discount(self.store, "S001", "AMIGO10")
        item = self.invoice("S001")
        billing.credit_note(self.store, item.number, 5000, "Error")
        billing.mark_paid(self.store, item.number, date(2025, 4, 5))
        store = self.reload()
        again = billing.find_invoice(store, item.number)
        self.assertEqual((again.subtotal_cents, again.vat_cents, again.total_cents),
                         (item.subtotal_cents, item.vat_cents, item.total_cents))
        self.assertEqual(self.lines(again), self.lines(item))
        self.assertEqual(billing.pending(store), [])
        self.assertEqual([m.discount for m in store.members if m.id == "S001"], ["AMIGO10"])
        self.assertEqual(billing.month_total(store, MARCH), item.total_cents - 5000)
        self.assertTrue(self.invoice("S003").number.endswith("0002"))
        self.assertEqual(billing.credit_note(store, item.number, 100, "Más").number, "NC-2025-0002")


class R13(Case):
    def test_euros(self):
        from cowork import money

        self.assertEqual(money.eur(123456), "1.234,56 €")
        self.assertEqual(money.eur(-2500), "-25,00 €")
        self.assertEqual(money.eur(5), "0,05 €")
        self.assertEqual(money.eur(100000000), "1.000.000,00 €")

    def test_shown_to_people(self):
        from cowork import plans

        self.assertIn("Fijo: 250,00 € al mes", plans.plans_text(self.store))
        self.assertNotIn("EUR", plans.plans_text(self.store))


def ana_invoice(case):
    from cowork import members

    members.set_discount(case.store, "S001", "AMIGO10")
    case.consume("S001", "cafe", 12)
    return case.invoice("S001")


class R14(Case):
    def test_text(self):
        from cowork import billing

        item = ana_invoice(self)
        self.assertEqual(
            billing.invoice_text(self.store, item),
            f"Factura {item.number}\n"
            "Socio: Ana Ruiz (S001)\n"
            "Periodo: marzo 2025\n"
            "Plan Fijo — marzo 2025: 250,00 €\n"
            "Descuento AMIGO10 (10%): -25,00 €\n"
            "Café x 12: 14,40 €\n"
            "Subtotal: 239,40 €\n"
            "IVA 21%: 50,27 €\n"
            "Total: 289,67 €",
        )


class R15(Case):
    def test_text(self):
        from cowork import billing

        item = self.invoice("S001")
        note = billing.credit_note(self.store, item.number, 5000, "Error en horas de sala")
        self.assertEqual(
            billing.credit_note_text(note),
            f"Nota de crédito {note.number}\nFactura: {item.number}\nMotivo: Error en horas de sala\nTotal: -50,00 €",
        )


HOURS = "Las salas se reservan de 08:00 a 20:00 en franjas de 30 minutos"


class R16(Case):
    def test_opening_hours_and_half_hours(self):
        from cowork import rooms

        day = date(2025, 3, 11)
        for start, end in (("07:30", "09:00"), ("19:30", "20:30"), ("10:15", "11:00"), ("10:00", "10:45")):
            self.raises(HOURS, rooms.book, self.store, "S001", "azul", day, start, end)
        rooms.book(self.store, "S001", "azul", day, "08:00", "08:30")
        rooms.book(self.store, "S001", "azul", day, "19:00", "20:00")


class R17(Case):
    def test_no_overlap(self):
        from cowork import rooms

        day = date(2025, 3, 11)
        rooms.book(self.store, "S001", "azul", day, "10:00", "11:00")
        self.raises("La sala Azul ya está reservada de 10:00 a 11:00", rooms.book, self.store, "S002", "azul", day, "10:30", "11:30")
        self.raises("La sala Azul ya está reservada de 10:00 a 11:00", rooms.book, self.store, "S002", "azul", day, "09:00", "12:00")
        rooms.book(self.store, "S002", "azul", day, "11:00", "12:00")
        rooms.book(self.store, "S002", "verde", day, "10:00", "11:00")
        rooms.book(self.store, "S002", "azul", date(2025, 3, 12), "10:00", "11:00")


class R18(Case):
    def test_cancel(self):
        from cowork import rooms

        day = date(2025, 3, 11)
        booking = rooms.book(self.store, "S001", "azul", day, "10:00", "11:00")
        rooms.cancel_booking(self.store, booking.id)
        self.assertEqual(rooms.member_minutes(self.store, "S001", MARCH), 0)
        self.assertNotIn("10:00-11:00", rooms.schedule_text(self.store, day))
        rooms.book(self.store, "S002", "azul", day, "10:00", "11:00")
        self.raises(f"Reserva inexistente: {booking.id}", rooms.cancel_booking, self.store, booking.id)
        self.raises("Reserva inexistente: 99", rooms.cancel_booking, self.store, 99)


class R19(Case):
    def test_email_and_nif(self):
        from cowork import members

        add = lambda email, nif: members.add_member(self.store, "S005", "Elena Mora", email, nif, "flex", date(2025, 3, 1))
        self.raises("Email inválido: anaexample.com", add, "anaexample.com", "33333333P")
        self.raises("Email inválido: ana @example.com", add, "ana @example.com", "33333333P")
        self.raises("Email inválido: ana@example", add, "ana@example", "33333333P")
        self.raises("NIF inválido: 12345678A", add, "elena@example.com", "12345678A")
        self.raises("NIF inválido: 1234", add, "elena@example.com", "1234")
        self.assertEqual(add("elena@example.com", "33333333P").id, "S005")


class R20(Case):
    def test_deactivate_and_listing(self):
        from cowork import members

        members.deactivate(self.store, "S001", date(2025, 3, 20))
        text = members.members_text(self.store)
        self.assertIn("S001 Ana Ruiz (Fijo) — baja 20/03/2025", text.splitlines())
        self.assertIn("S004 Diego Paz (Fijo) — baja 28/02/2025", text.splitlines())
        self.assertIn("S002 Bruno Sanz (Flex)", text.splitlines())
        self.assertFalse(members.is_active(members.find_member(self.store, "S001"), "2025-04"))
        self.assertEqual(len(self.store.members), 4)


class R21(Case):
    def test_report(self):
        from cowork import reports, rooms

        self.invoice("S001")
        self.invoice("S003")
        rooms.book(self.store, "S001", "azul", date(2025, 3, 10), "10:00", "12:30")
        rooms.book(self.store, "S003", "verde", date(2025, 3, 10), "09:00", "10:00")
        rooms.book(self.store, "S003", "verde", date(2025, 3, 11), "09:00", "10:00").cancelled = True
        lines = reports.month_report(self.store, MARCH).splitlines()
        self.assertEqual(lines[:2], ["Resumen de marzo 2025", "Socios activos: 3"])
        self.assertIn("Facturado: 411,40 €", lines)
        self.assertIn("Ocupación Azul: 2,5 h", lines)
        self.assertIn("Ocupación Verde: 1 h", lines)


class R22(Case):
    def test_csv(self):
        from cowork import billing, reports

        first = ana_invoice(self)
        second = self.invoice("S003")
        self.invoice("S003", "2025-04")
        billing.credit_note(self.store, first.number, 5000, "Error")
        rows = list(csv.reader(io.StringIO(reports.invoices_csv(self.store, MARCH))))
        self.assertEqual(rows, [
            ["numero", "factura", "socio", "subtotal", "iva", "total"],
            [first.number, "", "S001", "239.40", "50.27", "289.67"],
            [second.number, "", "S003", "90.00", "18.90", "108.90"],
            ["NC-2025-0001", first.number, "S001", "", "", "-50.00"],
        ])


class R23(Case):
    def test_email(self):
        from cowork import messages

        item = ana_invoice(self)
        text = f"Hola Ana, tu factura {item.number} de marzo 2025 por 289,67 € ya está disponible."
        self.assertEqual(messages.invoice_email(self.store, item.number), text)
        last = self.store.outbox[-1]
        self.assertEqual((last["to"], last["subject"], last["body"]), ("ana@example.com", f"Factura {item.number}", text))

    def test_invoice_all_queues_too(self):
        from cowork import billing

        before = len(self.store.outbox)
        billing.invoice_all(self.store, MARCH)
        self.assertEqual(len(self.store.outbox), before + 3)


class R24(Case):
    def test_cli(self):
        self.assertEqual(self.cli("consumo", "S001", "cafe", "12", "2025-03-10")[0], 0)
        code, out, _ = self.cli("facturar", "S001", MARCH, "--codigo", "AMIGO10")
        self.assertEqual(code, 0)
        number = out.splitlines()[0].split()[-1]
        self.assertIn("Descuento AMIGO10 (10%): -25,00 €", out)
        self.assertIn("Total: 289,67 €", out)
        self.assertEqual(self.cli("facturar", "--todos", MARCH)[1].strip(), "2 facturas emitidas")
        code, out, _ = self.cli("nota", number, "5000", "Error en horas de sala")
        self.assertEqual(out.splitlines()[0], "Nota de crédito NC-2025-0001")
        self.assertEqual(self.cli("cobrar", number, "2025-04-05")[1].strip(), f"{number} cobrada")
        out = self.cli("exportar", MARCH)[1]
        self.assertEqual(out.splitlines()[0], "numero,factura,socio,subtotal,iva,total")
        self.assertEqual(len(out.splitlines()), 5)
        code, _, err = self.cli("nota", "F-2025-0099", "100", "x")
        self.assertEqual(code, 1)
        self.assertIn("Error: No existe la factura F-2025-0099", err)


class REG(Case):
    """What worked at the start keeps working."""

    def test_members(self):
        from cowork import members

        self.assertEqual(members.members_text(self.store).splitlines()[0], "S001 Ana Ruiz (Fijo)")
        self.assertTrue(members.is_active(members.find_member(self.store, "S002"), MARCH))
        self.assertFalse(members.is_active(members.find_member(self.store, "S004"), MARCH))
        members.add_member(self.store, "S005", "Elena Mora", "elena@example.com", "33333333P", "flex", date(2025, 3, 1))
        self.assertEqual(members.next_id(self.store), "S006")

    def test_rooms_extras_access(self):
        from cowork import access, extras, rooms

        booking = rooms.book(self.store, "S001", "azul", date(2025, 3, 10), "10:00", "11:30")
        self.assertEqual(booking.id, 1)
        self.assertIn("  10:00-11:30 S001", rooms.schedule_text(self.store, date(2025, 3, 10)))
        extras.consume(self.store, "S001", "cafe", 3, date(2025, 3, 10))
        self.assertEqual(len(extras.consumptions_in_month(self.store, "S001", MARCH)), 1)
        access.enter(self.store, "S001", date(2025, 3, 10), "09:05")
        with self.assertRaises(ValueError):
            access.enter(self.store, "S004", date(2025, 3, 10), "09:05")

    def test_lockers_guests_reports(self):
        from cowork import guests, lockers, reports

        self.assertEqual(lockers.assign(self.store, "S001"), 1)
        guests.invite(self.store, "S001", "Marta", date(2025, 3, 10))
        store = self.reload()
        self.assertEqual(lockers.locker_of(store, "S001"), 1)
        self.assertEqual(len(store.guests), 1)
        lines = reports.month_report(store, MARCH).splitlines()
        self.assertEqual(lines[:3], ["Resumen de marzo 2025", "Socios activos: 3", "Altas: 1"])

    def test_cli(self):
        code, out, _ = self.cli("socios")
        self.assertEqual(code, 0)
        self.assertIn("S002 Bruno Sanz (Flex)", out)
