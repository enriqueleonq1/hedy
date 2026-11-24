import unittest

from taller.errors import NotFound, ShopError, ValidationError
from tests.helpers import fresh_shop


class CustomersTest(unittest.TestCase):
    def setUp(self):
        self.shop, self.path = fresh_shop()

    def test_vehicles_of_a_customer(self):
        plates = [v.plate for v in self.shop.customers.vehicles_of("C2")]
        self.assertEqual(plates, ["AC456EF", "KLM321"])

    def test_plate_lookup_ignores_case_and_spaces(self):
        self.assertEqual(self.shop.customers.vehicle(" ab123cd ").brand, "Fiat")

    def test_add_customer_and_vehicle(self):
        customer = self.shop.customers.add("Sofía Ríos", "11 5555-0009", "sofia@example.com")
        self.assertEqual(customer.id, "C4")
        self.assertEqual(customer.phone, "1155550009")
        vehicle = self.shop.customers.add_vehicle("ae111bb", customer.id, "Toyota", "Etios", 2019)
        self.assertEqual(vehicle.plate, "AE111BB")
        with self.assertRaises(ShopError):
            self.shop.customers.add_vehicle("AE111BB", customer.id, "Toyota", "Etios", 2019)

    def test_invalid_data(self):
        with self.assertRaises(ValidationError):
            self.shop.customers.add("X", "123")
        with self.assertRaises(NotFound):
            self.shop.customers.vehicle("ZZ999ZZ")


if __name__ == "__main__":
    unittest.main()
