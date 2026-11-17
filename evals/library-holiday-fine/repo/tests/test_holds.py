import unittest
from datetime import date

from library.holds.models import Hold
from library.holds.queue import HoldError, place_hold, position, waiting_holds
from library.loans.checkout import checkout
from library.loans.returns import process_return
from library.storage.csv_store import load_library


class Queue(unittest.TestCase):
    def test_waiting_in_placement_order(self):
        holds = [
            Hold("H2", "M2", "T1", "centro", date(2025, 5, 3)),
            Hold("H1", "M1", "T1", "centro", date(2025, 5, 1)),
            Hold("H3", "M3", "T1", "centro", date(2025, 5, 2), status="cancelled"),
        ]
        self.assertEqual([h.id for h in waiting_holds(holds, "T1")], ["H1", "H2"])
        self.assertEqual(position(holds, "H2"), 2)

    def test_one_hold_per_title(self):
        holds = [Hold("H1", "M1", "T1", "centro", date(2025, 5, 1))]
        with self.assertRaises(HoldError):
            place_hold(holds, "H2", "M1", "T1", "norte", date(2025, 5, 2))


class Allocation(unittest.TestCase):
    def setUp(self):
        self.library = load_library()

    def test_returned_copy_goes_to_a_waiting_hold(self):
        # Rayuela: lend the norte copy, someone holds it, it comes back
        loan = checkout(self.library, "M005", "I00002", "norte", date(2025, 7, 1))
        place_hold(self.library.holds, "H0100", "M007", "T00001", "norte", date(2025, 7, 2))
        outcome = process_return(self.library, loan.id, "norte", date(2025, 7, 7))
        self.assertIsNotNone(outcome.hold)
        self.assertEqual(outcome.hold.member_id, "M007")
        self.assertEqual(outcome.hold.status, "ready")
        self.assertEqual(outcome.hold.item_id, "I00002")

    def test_members_with_debt_are_skipped(self):
        # Cien años de soledad: Joaquín (debt) holds since May 6, Elena since May 20
        outcome = process_return(self.library, "L00142", "centro", date(2025, 5, 26))
        self.assertEqual(outcome.hold.member_id, "M010")

    def test_no_hold_no_allocation(self):
        loan = checkout(self.library, "M005", "I00017", "centro", date(2025, 7, 1))
        self.assertIsNone(process_return(self.library, loan.id, "centro", date(2025, 7, 3)).hold)


if __name__ == "__main__":
    unittest.main()
