"""Facturas mensuales de los socios y sus pagos."""
from dataclasses import dataclass, field
from datetime import date

from .members import family_members, frozen_days, get_member, plan_on
from .models import GymError, Payment, days_in_month, parse_month
from .pricing import (
    FAMILY_DISCOUNT_PERCENT,
    NO_SHOW_FEE,
    NO_SHOW_PLANS,
    PACK_CLASS,
    money,
    percent_of,
    plan_fee,
    prorate,
    vat_of,
)


@dataclass
class Invoice:
    member_id: str
    member_name: str
    month: str
    lines: list = field(default_factory=list)  # [(descripción, centavos)]
    number: str = ""

    @property
    def subtotal(self):
        return sum(cents for _, cents in self.lines)

    @property
    def vat(self):
        return vat_of(self.subtotal)

    @property
    def total(self):
        return self.subtotal + self.vat

    def text(self):
        """La factura como la ve el socio."""
        title = "Factura"
        if self.number:
            title += f" {self.number}"
        rows = [f"{title} — {self.member_name} — {self.month}"]
        for description, cents in self.lines:
            rows.append(f"{description}: {money(cents)}")
        rows.append(f"Subtotal: {money(self.subtotal)}")
        rows.append(f"IVA (21%): {money(self.vat)}")
        rows.append(f"Total: {money(self.total)}")
        return "\n".join(rows)


def _month_bookings(state, member_id, year, month):
    """Las reservas con plaza del socio en clases de ese mes."""
    return [
        b
        for b in state.bookings
        if b.member_id == member_id
        and b.status == "plaza"
        and (state.classes[b.class_id].start.year, state.classes[b.class_id].start.month) == (year, month)
    ]


def _has_family(state, member):
    return len(family_members(state, member.family)) >= 2


def invoice(state, member_id, month):
    """La factura de un socio en un mes ('AAAA-MM'), sin número."""
    member = get_member(state, member_id)
    year, number = parse_month(month)
    plan = plan_on(state, member.id, date(year, number, 1))
    result = Invoice(member_id=member.id, member_name=member.name, month=month)
    booked = _month_bookings(state, member.id, year, number)
    if plan == "pack":
        attended = sum(1 for b in booked if b.checked_in)
        result.lines.append((f"Clases sueltas: {attended} × {money(PACK_CLASS)}", attended * PACK_CLASS))
    else:
        days = days_in_month(year, number)
        frozen = frozen_days(state, member.id, year, number, days)
        fee = plan_fee(plan)
        description = f"Plan {plan}"
        if frozen:
            fee = prorate(fee, days - frozen, days)
            description += f" ({days - frozen} de {days} días)"
        result.lines.append((description, fee))
        if _has_family(state, member):
            discount = percent_of(fee, FAMILY_DISCOUNT_PERCENT)
            result.lines.append((f"Descuento familiar ({FAMILY_DISCOUNT_PERCENT}%)", -discount))
    if plan in NO_SHOW_PLANS:
        missed = sum(1 for b in booked if not b.checked_in)
        if missed:
            result.lines.append((f"Inasistencias: {missed} × {money(NO_SHOW_FEE)}", missed * NO_SHOW_FEE))
    return result


def issue(state, member_id, month):
    """La factura con su número: el mismo cada vez que se pide la misma, y el
    siguiente de una sola serie para una nueva."""
    result = invoice(state, member_id, month)
    key = f"{result.member_id}|{month}"
    if key not in state.invoice_numbers:
        year, _ = parse_month(month)
        state.invoice_numbers[key] = f"F-{year}-{state.next_invoice:04d}"
        state.next_invoice += 1
    result.number = state.invoice_numbers[key]
    return result


def record_payment(state, member_id, cents, day, note=""):
    get_member(state, member_id)
    if cents <= 0:
        raise GymError("El pago tiene que ser positivo")
    payment = Payment(member_id=member_id, day=day, cents=cents, note=note)
    state.payments.append(payment)
    return payment


def paid_in(state, member_id, month):
    year, number = parse_month(month)
    return sum(
        p.cents
        for p in state.payments
        if p.member_id == member_id and (p.day.year, p.day.month) == (year, number)
    )


def balance(state, member_id, month):
    """Lo que el socio debe del mes (negativo si pagó de más)."""
    return invoice(state, member_id, month).total - paid_in(state, member_id, month)


def first_day(month):
    year, number = parse_month(month)
    return date(year, number, 1)
