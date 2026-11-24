"""The requirements the session-3 change rewrites (R03 and R05), as they are
after it; each also checks that the old behaviour is gone."""
from hidden_gym import EARLY, GymCase, at


class R03(GymCase):
    def test_cancel_up_to_four_hours_before(self):
        booking = self.gym.book("s001", "yoga", EARLY)
        self.gym.cancel(booking.id, at("2025-03-10T14:00"))
        self.assertEqual(self.gym.attendees("yoga"), [])
        late = self.gym.book("s002", "yoga", EARLY)
        self.assertEqual(
            self.error(self.gym.cancel, late.id, at("2025-03-10T14:01")),
            "Ya no se puede cancelar: faltan menos de 4 horas",
        )
        # Three hours before was fine under the old rule; not any more.
        self.assertEqual(
            self.error(self.gym.cancel, late.id, at("2025-03-10T15:00")),
            "Ya no se puede cancelar: faltan menos de 4 horas",
        )
        self.assertEqual(self.gym.attendees("yoga"), ["Luis Gómez"])


class R05(GymCase):
    def test_basic_has_four_active_places(self):
        for class_id in ("pila", "box", "func", "yoga2"):
            self.assertEqual(self.gym.book("s002", class_id, EARLY).status, "plaza")
        self.assertEqual(self.error(self.gym.book, "s002", "func2", EARLY), "Tu plan basic permite 4 reservas activas")

    def test_waiting_does_not_count_and_started_classes_neither(self):
        self.gym.book("s001", "yoga", EARLY)
        self.gym.book("s004", "yoga", EARLY)
        for class_id in ("pila", "box", "func", "yoga2"):
            self.gym.book("s002", class_id, EARLY)
        self.assertEqual(self.gym.book("s002", "yoga", EARLY).status, "espera")
        self.assertEqual(self.gym.book("s002", "func2", at("2025-03-12T21:00")).status, "plaza")
