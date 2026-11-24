"""Clientes y sus vehículos."""
from .errors import NotFound, ShopError
from .models import Customer, Vehicle
from .validation import check_email, check_year, normalize_phone, normalize_plate


class CustomerBook:
    def __init__(self, customers: list[Customer], vehicles: list[Vehicle]):
        self._customers = {c.id: c for c in customers}
        self._vehicles = {v.plate: v for v in vehicles}

    # Clientes

    def all(self) -> list[Customer]:
        return list(self._customers.values())

    def get(self, customer_id: str) -> Customer:
        try:
            return self._customers[customer_id]
        except KeyError:
            raise NotFound(f"Cliente inexistente: {customer_id}") from None

    def add(self, name: str, phone: str = "", email: str = "") -> Customer:
        name = (name or "").strip()
        if not name:
            raise ShopError("Falta el nombre del cliente")
        number = len(self._customers) + 1
        while f"C{number}" in self._customers:
            number += 1
        customer = Customer(f"C{number}", name, normalize_phone(phone), check_email(email))
        self._customers[customer.id] = customer
        return customer

    def update_contact(self, customer_id: str, phone: str | None = None, email: str | None = None) -> Customer:
        customer = self.get(customer_id)
        if phone is not None:
            customer.phone = normalize_phone(phone)
        if email is not None:
            customer.email = check_email(email)
        return customer

    def search(self, text: str) -> list[Customer]:
        text = text.strip().lower()
        return [c for c in self._customers.values() if text in c.name.lower()]

    # Vehículos

    def vehicles(self) -> list[Vehicle]:
        return list(self._vehicles.values())

    def vehicle(self, plate: str) -> Vehicle:
        key = normalize_plate(plate)
        try:
            return self._vehicles[key]
        except KeyError:
            raise NotFound(f"Vehículo inexistente: {key}") from None

    def has_vehicle(self, plate: str) -> bool:
        return normalize_plate(plate) in self._vehicles

    def add_vehicle(self, plate: str, customer_id: str, brand: str, model: str, year: int) -> Vehicle:
        self.get(customer_id)
        key = normalize_plate(plate)
        if key in self._vehicles:
            raise ShopError(f"La patente {key} ya está registrada")
        vehicle = Vehicle(key, customer_id, brand.strip(), model.strip(), check_year(year))
        self._vehicles[key] = vehicle
        return vehicle

    def vehicles_of(self, customer_id: str) -> list[Vehicle]:
        self.get(customer_id)
        return [v for v in self._vehicles.values() if v.customer == customer_id]

    def owner(self, plate: str) -> Customer:
        return self.get(self.vehicle(plate).customer)
