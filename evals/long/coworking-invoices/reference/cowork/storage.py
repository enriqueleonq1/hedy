"""Los datos del coworking en una carpeta: catálogos en JSON (planes, salas,
extras, socios) y lo que cambia día a día en state.json."""

import json
from pathlib import Path

from .models import (
    AccessEntry, Booking, Consumption, CreditNote, Extra, Invoice, Line, Member, Plan, Room,
)
from .dates import parse_day

CATALOGS = ("plans.json", "rooms.json", "extras.json", "members.json")


def _day(value):
    return parse_day(value) if value else None


def member_from(data):
    return Member(
        id=data["id"],
        name=data["name"],
        email=data["email"],
        nif=data["nif"],
        plan=data["plan"],
        joined=_day(data["joined"]),
        left=_day(data.get("left")),
        discount=data.get("discount"),
    )


def member_to(member):
    return {
        "id": member.id,
        "name": member.name,
        "email": member.email,
        "nif": member.nif,
        "plan": member.plan,
        "joined": member.joined.isoformat(),
        "left": member.left.isoformat() if member.left else None,
        "discount": member.discount,
    }


def booking_from(data):
    return Booking(
        id=data["id"],
        member=data["member"],
        room=data["room"],
        day=parse_day(data["day"]),
        start=data["start"],
        end=data["end"],
        cancelled=data.get("cancelled", False),
    )


def booking_to(booking):
    return {
        "id": booking.id,
        "member": booking.member,
        "room": booking.room,
        "day": booking.day.isoformat(),
        "start": booking.start,
        "end": booking.end,
        "cancelled": booking.cancelled,
    }


def consumption_from(data):
    return Consumption(data["member"], data["extra"], data["qty"], parse_day(data["day"]))


def consumption_to(item):
    return {"member": item.member, "extra": item.extra, "qty": item.qty, "day": item.day.isoformat()}


def invoice_from(data):
    return Invoice(
        number=data["number"],
        member_id=data["member_id"],
        month=data["month"],
        lines=[Line(l["description"], l["cents"]) for l in data["lines"]],
        total_cents=data["total_cents"],
        subtotal_cents=data.get("subtotal_cents", data["total_cents"]),
        vat_cents=data.get("vat_cents", 0),
        paid_on=_day(data.get("paid_on")),
    )


def invoice_to(invoice):
    return {
        "number": invoice.number,
        "member_id": invoice.member_id,
        "month": invoice.month,
        "lines": [{"description": l.description, "cents": l.cents} for l in invoice.lines],
        "total_cents": invoice.total_cents,
        "subtotal_cents": invoice.subtotal_cents,
        "vat_cents": invoice.vat_cents,
        "paid_on": invoice.paid_on.isoformat() if invoice.paid_on else None,
    }


def note_from(data):
    return CreditNote(data["number"], data["invoice"], data["total_cents"], data["reason"])


def note_to(note):
    return {"number": note.number, "invoice": note.invoice, "total_cents": note.total_cents, "reason": note.reason}


def access_from(data):
    return AccessEntry(data["member"], parse_day(data["day"]), data["time"], data["door"])


def access_to(entry):
    return {"member": entry.member, "day": entry.day.isoformat(), "time": entry.time, "door": entry.door}


class Store:
    """Todo lo del coworking, cargado de una carpeta. save() escribe socios y
    estado de vuelta en la misma carpeta."""

    def __init__(self, folder):
        self.folder = Path(folder)
        self.plans = {}
        self.rooms = {}
        self.extras = {}
        self.members = []
        self.bookings = []
        self.consumptions = []
        self.invoices = []
        self.credit_notes = []
        self.access_log = []
        self.outbox = []
        self.lockers = {}
        self.guests = []
        self.next_booking = 1

    @classmethod
    def load(cls, folder):
        store = cls(folder)
        folder = Path(folder)
        read = lambda name: json.loads((folder / name).read_text(encoding="utf-8"))
        store.plans = {p["id"]: Plan(**p) for p in read("plans.json")}
        store.rooms = {r["id"]: Room(**r) for r in read("rooms.json")}
        store.extras = {e["code"]: Extra(**e) for e in read("extras.json")}
        store.members = [member_from(m) for m in read("members.json")]
        state_file = folder / "state.json"
        state = json.loads(state_file.read_text(encoding="utf-8")) if state_file.exists() else {}
        store.next_booking = state.get("next_booking", 1)
        store.bookings = [booking_from(b) for b in state.get("bookings", [])]
        store.consumptions = [consumption_from(c) for c in state.get("consumptions", [])]
        store.invoices = [invoice_from(i) for i in state.get("invoices", [])]
        store.credit_notes = [note_from(n) for n in state.get("credit_notes", [])]
        store.access_log = [access_from(a) for a in state.get("access_log", [])]
        store.outbox = list(state.get("outbox", []))
        store.lockers = {int(n): m for n, m in state.get("lockers", {}).items()}
        store.guests = list(state.get("guests", []))
        return store

    def state(self):
        return {
            "next_booking": self.next_booking,
            "bookings": [booking_to(b) for b in self.bookings],
            "consumptions": [consumption_to(c) for c in self.consumptions],
            "invoices": [invoice_to(i) for i in self.invoices],
            "credit_notes": [note_to(n) for n in self.credit_notes],
            "access_log": [access_to(a) for a in self.access_log],
            "outbox": self.outbox,
            "lockers": {str(n): m for n, m in sorted(self.lockers.items())},
            "guests": self.guests,
        }

    def save(self):
        write = lambda name, data: (self.folder / name).write_text(
            json.dumps(data, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
        )
        write("members.json", [member_to(m) for m in self.members])
        write("state.json", self.state())
