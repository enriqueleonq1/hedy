import unittest
from datetime import date

from cowork import members
from tests.helpers import fresh_store


class MembersTest(unittest.TestCase):
    def setUp(self):
        self.store = fresh_store(self)

    def test_listing(self):
        text = members.members_text(self.store)
        self.assertEqual(text.splitlines()[0], "S001 Ana Ruiz (Fijo)")
        self.assertEqual(len(text.splitlines()), 4)

    def test_active_in_a_month(self):
        self.assertTrue(members.is_active(members.find_member(self.store, "S002"), "2025-03"))
        self.assertFalse(members.is_active(members.find_member(self.store, "S002"), "2025-02"))
        self.assertFalse(members.is_active(members.find_member(self.store, "S004"), "2025-03"))

    def test_add_member(self):
        member = members.add_member(
            self.store, "S005", "Elena Mora", "elena@example.com", "33333333P", "flex", date(2025, 3, 1)
        )
        self.assertEqual(member.first_name, "Elena")
        self.assertEqual(members.next_id(self.store), "S006")
        with self.assertRaises(ValueError):
            members.add_member(self.store, "S005", "Otra", "o@example.com", "33333333P", "flex", date(2025, 3, 1))
