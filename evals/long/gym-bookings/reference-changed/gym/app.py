"""El gimnasio entero: lo que usan la CLI y la recepción.

    gym = Gym.load("data")
    booking = gym.book("s001", "yoga-1003", now)
    gym.save()
"""
from . import billing, bookings, exports, members, reports, storage
from .notifications import pending, pending_lines, queue, welcome_text


class Gym:
    def __init__(self, state, folder=None):
        self.state = state
        self.folder = folder

    @classmethod
    def load(cls, folder):
        return cls(storage.load(folder), folder)

    def save(self):
        storage.save(self.state, self.folder)

    # Clases y socios

    @property
    def classes(self):
        return list(self.state.classes.values())

    def gym_class(self, class_id):
        return bookings.get_class(self.state, class_id)

    def member(self, member_id):
        return members.get_member(self.state, member_id)

    def add_member(self, member_id, name, email, plan, family="", now=None):
        member = members.add_member(self.state, member_id, name, email, plan, family)
        if now is not None:
            queue(self.state, member.id, "Bienvenida", welcome_text(member), now)
        return member

    # Reservas

    def book(self, member_id, class_id, now):
        return bookings.book(self.state, member_id, class_id, now)

    def cancel(self, booking_id, now):
        return bookings.cancel(self.state, booking_id, now)

    def booking(self, booking_id):
        return bookings.get_booking(self.state, booking_id)

    def attendees(self, class_id):
        return bookings.attendees(self.state, class_id)

    def waitlist(self, class_id):
        return bookings.waitlist(self.state, class_id)

    def check_in(self, booking_id, now):
        return bookings.check_in(self.state, booking_id, now)

    def no_shows(self, class_id, now):
        return bookings.no_shows(self.state, class_id, now)

    def send_reminders(self, now):
        return bookings.send_reminders(self.state, now)

    def class_csv(self, class_id):
        return exports.class_csv(self.state, class_id)

    # Membresías

    def freeze(self, member_id, start, end):
        return members.freeze(self.state, member_id, start, end)

    def change_plan(self, member_id, plan, now):
        return members.change_plan(self.state, member_id, plan, now)

    # Facturación

    def invoice(self, member_id, month):
        return billing.issue(self.state, member_id, month)

    def record_payment(self, member_id, cents, day, note=""):
        return billing.record_payment(self.state, member_id, cents, day, note)

    def balance(self, member_id, month):
        return billing.balance(self.state, member_id, month)

    # Reportes y mensajes

    def occupancy(self, day):
        return reports.occupancy(self.state, day)

    def revenue(self, month):
        return reports.revenue(self.state, month)

    def instructor_report(self, month):
        return reports.instructor_report(self.state, month)

    @property
    def outbox(self):
        return self.state.outbox

    def pending_messages(self):
        return pending(self.state)

    def take_pending_lines(self):
        return pending_lines(self.state)
