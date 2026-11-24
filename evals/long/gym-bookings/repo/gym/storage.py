"""Carga y guarda los datos del gimnasio en una carpeta.

- classes.json: las clases, [{"id", "name", "start" (ISO), "capacity",
  "instructor", "room", "minutes"}]
- members.csv: los socios, columnas id, name, email, plan, family
- state.json: lo que cambia día a día (reservas, pagos, mensajes). Si no
  existe, se empieza de cero.
"""
import csv
import json
from dataclasses import dataclass, field
from datetime import date, datetime
from pathlib import Path

from .models import Booking, GymClass, Member, Message, Payment

MEMBER_COLUMNS = ["id", "name", "email", "plan", "family"]


@dataclass
class State:
    classes: dict = field(default_factory=dict)
    members: dict = field(default_factory=dict)
    bookings: list = field(default_factory=list)
    payments: list = field(default_factory=list)
    outbox: list = field(default_factory=list)
    next_booking: int = 1


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
    }


def _booking_from_dict(data):
    return Booking(
        id=int(data["id"]),
        member_id=data["member"],
        class_id=data["class"],
        created_at=datetime.fromisoformat(data["created_at"]),
    )


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
    }
    (folder / "state.json").write_text(
        json.dumps(data, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
