import unittest
from datetime import date

from library.loans.checkout import CheckoutError, checkout
from library.loans.renewals import RenewalError, renew
from library.loans.returns import process_return
from library.storage.csv_store import load_library


class Checkout(unittest.TestCase):
    def setUp(self):
        self.library = load_library()

    def test_lends_a_free_copy(self):
        loan = checkout(self.library, "M001", "I00002", "norte", date(2025, 7, 1))
        self.assertTrue(loan.active)
        self.assertIn("I00002", self.library.lent_item_ids())

    def test_refuses_a_lent_copy(self):
        with self.assertRaises(CheckoutError) as e:
            checkout(self.library, "M001", "I00004", "centro", date(2025, 7, 1))
        self.assertEqual(e.exception.reason, "already_lent")

    def test_refuses_members_with_debt(self):
        with self.assertRaises(CheckoutError) as e:
            checkout(self.library, "M009", "I00002", "norte", date(2025, 7, 1))
        self.assertEqual(e.exception.reason, "unpaid_fines")

    def test_refuses_suspended_members(self):
        with self.assertRaises(CheckoutError) as e:
            checkout(self.library, "M011", "I00002", "norte", date(2025, 7, 1))
        self.assertEqual(e.exception.reason, "suspended")


class Returns(unittest.TestCase):
    def setUp(self):
        self.library = load_library()

    def test_on_time_return_has_no_fine(self):
        loan = checkout(self.library, "M005", "I00002", "norte", date(2025, 7, 1))
        outcome = process_return(self.library, loan.id, "norte", date(2025, 7, 10))
        self.assertEqual(outcome.fine_cents, 0)
        self.assertFalse(self.library.loans[loan.id].active)

    def test_late_return_is_charged(self):
        loan = checkout(self.library, "M005", "I00002", "norte", date(2025, 7, 1))
        before = self.library.ledger.unpaid_cents("M005")
        # due Tue 15, back Fri 18: Wed, Thu, Fri
        outcome = process_return(self.library, loan.id, "norte", date(2025, 7, 18))
        self.assertEqual(outcome.fine_cents, 75)
        self.assertEqual(self.library.ledger.unpaid_cents("M005"), before + 75)


class Renewals(unittest.TestCase):
    def setUp(self):
        self.library = load_library()

    def test_renews_when_nobody_waits(self):
        loan = checkout(self.library, "M005", "I00002", "norte", date(2025, 7, 1))
        self.assertEqual(renew(self.library, loan.id, date(2025, 7, 10)).renewals, 1)

    def test_not_when_someone_waits(self):
        # Joaquín's copy of "Cien años de soledad" has holds waiting
        with self.assertRaises(RenewalError) as e:
            renew(self.library, "L00142", date(2025, 5, 10))
        self.assertEqual(e.exception.reason, "on_hold")


if __name__ == "__main__":
    unittest.main()
