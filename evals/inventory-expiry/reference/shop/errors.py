class InventoryError(Exception):
    """Base class for inventory errors."""


class UnknownProduct(InventoryError):
    pass


class NotEnoughStock(InventoryError):
    pass


class UnknownReservation(InventoryError):
    pass


class ReservationExpired(InventoryError):
    pass
