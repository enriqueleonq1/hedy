"""Texts sent to members, in Spanish as shown at the front desk."""
from datetime import date

from library.config import DATE_FORMAT
from library.money import fmt_cents

CHECKOUT_REFUSALS = {
    "unknown_member": "No encontramos ese carnet.",
    "unknown_item": "Ese ejemplar no está en el catálogo.",
    "already_lent": "Ese ejemplar ya está prestado.",
    "reserved": "Ese ejemplar está apartado para otra reserva.",
    "suspended": "El carnet está suspendido.",
    "unpaid_fines": "Tiene multas pendientes por encima del límite.",
    "loan_limit": "Alcanzó el máximo de préstamos de su categoría.",
}

RENEWAL_REFUSALS = {
    "unknown_loan": "Ese préstamo no está activo.",
    "max_renewals": "Ya se renovó el máximo de veces.",
    "overdue": "El préstamo está vencido: hay que devolverlo.",
    "on_hold": "Otra persona está esperando este título.",
}


def _day(d: date) -> str:
    return d.strftime(DATE_FORMAT)


def due_reminder(name: str, title: str, due: date) -> str:
    return f"Hola {name}: «{title}» vence el {_day(due)}. Puedes renovarlo en la web."


def overdue_notice(name: str, title: str, due: date, fine_cents: int) -> str:
    return (
        f"Hola {name}: «{title}» venció el {_day(due)}. "
        f"La multa acumulada es de {fmt_cents(fine_cents)}."
    )


def hold_ready(name: str, title: str, branch: str, pickup_by: date) -> str:
    return (
        f"Hola {name}: «{title}» te espera en la sucursal {branch}. "
        f"Puedes retirarlo hasta el {_day(pickup_by)}."
    )


def fine_charged(name: str, title: str, fine_cents: int) -> str:
    return f"Hola {name}: se registró una multa de {fmt_cents(fine_cents)} por «{title}»."
