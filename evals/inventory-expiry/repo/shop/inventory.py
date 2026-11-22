import time

from .errors import NotEnoughStock, UnknownProduct, UnknownReservation
from .models import Product, Reservation


class Inventory:
    """Products and the reservations held against their stock.

    `clock` returns the current time in seconds; tests pass a fake one.
    """

    def __init__(self, clock=time.time):
        self._clock = clock
        self._products = {}
        self._reservations = {}
        self._next_id = 1

    def add_product(self, sku, name, stock):
        self._products[sku] = Product(sku, name, stock)

    def product(self, sku):
        try:
            return self._products[sku]
        except KeyError:
            raise UnknownProduct(sku) from None

    def reserved(self, sku):
        return sum(r.qty for r in self._reservations.values() if r.sku == sku and not r.confirmed)

    def available(self, sku):
        return self.product(sku).stock - self.reserved(sku)

    def reserve(self, sku, qty):
        if qty <= 0:
            raise ValueError("qty must be positive")
        if self.available(sku) < qty:
            raise NotEnoughStock(sku)
        reservation = Reservation(self._next_id, sku, qty, self._clock())
        self._reservations[reservation.id] = reservation
        self._next_id += 1
        return reservation

    def reservation(self, reservation_id):
        try:
            return self._reservations[reservation_id]
        except KeyError:
            raise UnknownReservation(reservation_id) from None

    def confirm(self, reservation_id):
        reservation = self.reservation(reservation_id)
        if reservation.confirmed:
            return reservation
        self.product(reservation.sku).stock -= reservation.qty
        reservation.confirmed = True
        return reservation

    def cancel(self, reservation_id):
        reservation = self.reservation(reservation_id)
        if reservation.confirmed:
            raise ValueError("a confirmed reservation cannot be cancelled")
        del self._reservations[reservation_id]

    def reservations(self):
        return list(self._reservations.values())
