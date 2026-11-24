import time

from .errors import NotEnoughStock, ReservationExpired, UnknownProduct, UnknownReservation
from .models import Product, Reservation


HOLD_SECONDS = 15 * 60


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

    def expired(self, reservation):
        return not reservation.confirmed and self._clock() - reservation.created_at >= HOLD_SECONDS

    def active(self, reservation):
        return not reservation.confirmed and not self.expired(reservation)

    def release_expired(self):
        gone = [r.id for r in self._reservations.values() if self.expired(r)]
        for rid in gone:
            del self._reservations[rid]
        return len(gone)

    def reserved(self, sku):
        return sum(r.qty for r in self._reservations.values() if r.sku == sku and self.active(r))

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
        if self.expired(reservation):
            raise ReservationExpired(reservation_id)
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
