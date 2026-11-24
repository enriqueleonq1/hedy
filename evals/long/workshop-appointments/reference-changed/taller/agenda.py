"""La agenda de turnos: horario del taller, franjas y qué mecánico atiende cada una."""
from datetime import date

from .errors import BookingError
from .models import Appointment

OPENS = "08:00"
CLOSES = "18:00"
LUNCH = ("13:30", "14:30")
STEP_MINUTES = 30


def minutes(hhmm: str) -> int:
    hours, mins = hhmm.split(":")
    return int(hours) * 60 + int(mins)


def hhmm(total: int) -> str:
    return f"{total // 60:02d}:{total % 60:02d}"


def overlaps(start_a: str, end_a: str, start_b: str, end_b: str) -> bool:
    """Dos franjas se pisan si comparten algún minuto (una que termina cuando la otra empieza, no)."""
    return minutes(start_a) < minutes(end_b) and minutes(start_b) < minutes(end_a)


def lunch_text() -> str:
    return f"{LUNCH[0]}–{LUNCH[1]}"


def check_hours(day: date, start: str, length: int) -> str:
    """Comprueba que la franja quepa en el horario del taller; devuelve su fin."""
    begin = minutes(start)
    end = begin + length
    if day.weekday() >= 5 or begin < minutes(OPENS) or end > minutes(CLOSES):
        raise BookingError("Fuera del horario del taller")
    if (begin - minutes(OPENS)) % STEP_MINUTES:
        raise BookingError(f"Los turnos empiezan cada {STEP_MINUTES} minutos")
    if overlaps(start, hhmm(end), *LUNCH):
        raise BookingError(f"Ese horario cruza la pausa de almuerzo ({lunch_text()})")
    return hhmm(end)


class Agenda:
    def __init__(self, appointments: list[Appointment]):
        self._items = {a.id: a for a in appointments}

    def all(self) -> list[Appointment]:
        return sorted(self._items.values(), key=lambda a: a.id)

    def next_id(self) -> int:
        """Los números no se reusan: uno cancelado sigue ocupando el suyo."""
        return max(self._items, default=0) + 1

    def get(self, appointment_id: int) -> Appointment:
        try:
            return self._items[int(appointment_id)]
        except (KeyError, TypeError, ValueError):
            raise BookingError("Turno inexistente") from None

    def active(self, appointment_id: int) -> Appointment:
        """Un turno que no está cancelado."""
        appointment = self.get(appointment_id)
        if not appointment.active:
            raise BookingError("Turno inexistente")
        return appointment

    def on(self, day: date) -> list[Appointment]:
        """Los turnos del día (cancelados incluidos), por hora y número."""
        return sorted((a for a in self._items.values() if a.day == day), key=lambda a: (a.start, a.id))

    def busy(self, day: date, start: str, end: str, *, mechanic: str | None = None,
             plate: str | None = None) -> bool:
        for a in self.on(day):
            if not a.active or not overlaps(start, end, a.start, a.end):
                continue
            if (mechanic is not None and a.mechanic == mechanic) or (plate is not None and a.plate == plate):
                return True
        return False

    def free_mechanic(self, mechanics, day: date, start: str, end: str):
        """El primer mecánico (en el orden dado) que no tiene otro turno en esa franja."""
        for m in mechanics:
            if not self.busy(day, start, end, mechanic=m.id):
                return m
        return None

    def add(self, appointment: Appointment) -> Appointment:
        self._items[appointment.id] = appointment
        return appointment

    def slots(self, mechanics, day: date, length: int) -> list[str]:
        """Las horas de inicio en que algún mecánico de `mechanics` puede atender `length` minutos."""
        found = []
        start = minutes(OPENS)
        while start + length <= minutes(CLOSES):
            begin = hhmm(start)
            try:
                end = check_hours(day, begin, length)
            except BookingError:
                start += STEP_MINUTES
                continue
            if self.free_mechanic(mechanics, day, begin, end) is not None:
                found.append(begin)
            start += STEP_MINUTES
        return found
