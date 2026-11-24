"""Carga y guarda los datos del gimnasio en una carpeta.

- classes.json: las clases, [{"id", "name", "start" (ISO), "capacity",
  "instructor", "room", "minutes"}]
- members.csv: los socios, columnas id, name, email, plan, family
- state.json: lo que cambia día a día (reservas y su lista de espera,
  check-ins, pagos, mensajes, congelamientos, cambios de plan, números de
  factura y recordatorios enviados). Si no existe, se empieza de cero.
"""
import csv
import json
from dataclasses import dataclass, field
from datetime import date, datetime
from pathlib import Path

from .models import Booking, Freeze, GymClass, Member, Message, Payment, PlanChange

MEMBER_COLUMNS = ["id", "name", "email", "plan", "family"]


@dataclass
class State:
    classes: dict = field(default_factory=dict)
    members: dict = field(default_factory=dict)
    bookings: list = field(default_factory=list)
    payments: list = field(default_factory=list)
    outbox: list = field(default_factory=list)
    next_booking: int = 1
    freezes: list = field(default_factory=list)
    plan_changes: list = field(default_factory=list)
    # "socio|AAAA-MM" -> "F-2025-0001"
    invoice_numbers: dict = field(default_factory=dict)
    next_invoice: int = 1
    # Ids de las reservas a las que ya se mandó recordatorio.
    reminded: set = field(default_factory=set)


def read_classes(path):
    with open(path, encoding="utf-8") as f:
        data = json.load(f)
    classes = {}
    for c in data:
        gym_class = GymClass(
            id=c["id"],
            name=c["name"],
            start=datetime.fromisoformat(c["start"]),
            capacity=int(c["capacity"]),
            instructor=c["instructor"],
            room=c.get("room", "Sala 1"),
            minutes=int(c.get("minutes", 60)),
        )
        classes[gym_class.id] = gym_class
    return classes


def read_members(path):
    members = {}
    with open(path, encoding="utf-8", newline="") as f:
        for row in csv.DictReader(f):
            member = Member(
                id=row["id"],
                name=row["name"],
                email=row.get("email", ""),
                plan=row["plan"],
                family=row.get("family", "") or "",
            )
            members[member.id] = member
    return members


def write_members(members, path):
    with open(path, "w", encoding="utf-8", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=MEMBER_COLUMNS)
        writer.writeheader()
        for member in members.values():
            writer.writerow(
                {
                    "id": member.id,
                    "name": member.name,
                    "email": member.email,
                    "plan": member.plan,
                    "family": member.family,
                }
            )


def _booking_to_dict(booking):
    return {
        "id": booking.id,
        "member": booking.member_id,
        "class": booking.class_id,
        "created_at": booking.created_at.isoformat(),
        "status": booking.status,
        "checked_in": booking.checked_in,
    }


def _booking_from_dict(data):
    return Booking(
        id=int(data["id"]),
        member_id=data["member"],
        class_id=data["class"],
        created_at=datetime.fromisoformat(data["created_at"]),
        status=data.get("status", "plaza"),
        checked_in=bool(data.get("checked_in", False)),
    )


def _freeze_to_dict(freeze):
    return {"member": freeze.member_id, "start": freeze.start.isoformat(), "end": freeze.end.isoformat()}


def _freeze_from_dict(data):
    return Freeze(data["member"], date.fromisoformat(data["start"]), date.fromisoformat(data["end"]))


def _change_to_dict(change):
    return {"member": change.member_id, "plan": change.plan, "since": change.since.isoformat()}


def _change_from_dict(data):
    return PlanChange(data["member"], data["plan"], date.fromisoformat(data["since"]))


def _payment_to_dict(payment):
    return {
        "member": payment.member_id,
        "day": payment.day.isoformat(),
        "cents": payment.cents,
        "note": payment.note,
    }


def _payment_from_dict(data):
    return Payment(
        member_id=data["member"],
        day=date.fromisoformat(data["day"]),
        cents=int(data["cents"]),
        note=data.get("note", ""),
    )


def _message_to_dict(message):
    return {
        "to": message.to,
        "subject": message.subject,
        "body": message.body,
        "created_at": message.created_at.isoformat(),
        "sent": message.sent,
    }


def _message_from_dict(data):
    return Message(
        to=data["to"],
        subject=data["subject"],
        body=data["body"],
        created_at=datetime.fromisoformat(data["created_at"]),
        sent=bool(data.get("sent", False)),
    )


def load(folder):
    """El estado completo guardado en `folder`."""
    folder = Path(folder)
    state = State(
        classes=read_classes(folder / "classes.json"),
        members=read_members(folder / "members.csv"),
    )
    saved = folder / "state.json"
    if saved.exists():
        data = json.loads(saved.read_text(encoding="utf-8"))
        state.bookings = [_booking_from_dict(b) for b in data.get("bookings", [])]
        state.payments = [_payment_from_dict(p) for p in data.get("payments", [])]
        state.outbox = [_message_from_dict(m) for m in data.get("outbox", [])]
        state.next_booking = int(data.get("next_booking", len(state.bookings) + 1))
        state.freezes = [_freeze_from_dict(f) for f in data.get("freezes", [])]
        state.plan_changes = [_change_from_dict(c) for c in data.get("plan_changes", [])]
        state.invoice_numbers = dict(data.get("invoice_numbers", {}))
        state.next_invoice = int(data.get("next_invoice", len(state.invoice_numbers) + 1))
        state.reminded = set(int(b) for b in data.get("reminded", []))
    return state


def save(state, folder):
    """Guarda los socios y lo que cambia (las clases no se tocan desde aquí)."""
    folder = Path(folder)
    write_members(state.members, folder / "members.csv")
    data = {
        "bookings": [_booking_to_dict(b) for b in state.bookings],
        "payments": [_payment_to_dict(p) for p in state.payments],
        "outbox": [_message_to_dict(m) for m in state.outbox],
        "next_booking": state.next_booking,
        "freezes": [_freeze_to_dict(f) for f in state.freezes],
        "plan_changes": [_change_to_dict(c) for c in state.plan_changes],
        "invoice_numbers": state.invoice_numbers,
        "next_invoice": state.next_invoice,
        "reminded": sorted(state.reminded),
    }
    (folder / "state.json").write_text(
        json.dumps(data, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
