"""Facturas mensuales de los socios y sus pagos."""
from dataclasses import dataclass, field
from datetime import date

from .members import get_member
from .models import GymError, Payment, parse_month
from .pricing import money, plan_fee, vat_of


@dataclass
class Invoice:
    member_id: str
    member_name: str
    month: str
    lines: list = field(default_factory=list)  # [(descripción, centavos)]

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
        rows = [f"Factura — {self.member_name} — {self.month}"]
        for description, cents in self.lines:
            rows.append(f"{description}: {money(cents)}")
        rows.append(f"Subtotal: {money(self.subtotal)}")
        rows.append(f"IVA (21%): {money(self.vat)}")
        rows.append(f"Total: {money(self.total)}")
        return "\n".join(rows)


def invoice(state, member_id, month):
    """La factura de un socio en un mes ('AAAA-MM'): la cuota de su plan."""
    member = get_member(state, member_id)
    parse_month(month)
    result = Invoice(member_id=member.id, member_name=member.name, month=month)
    result.lines.append((f"Plan {member.plan}", plan_fee(member.plan)))
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
