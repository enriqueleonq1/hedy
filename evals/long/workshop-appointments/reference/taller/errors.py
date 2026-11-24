"""Errores del taller. El texto de cada error es el que se le muestra a quien atiende."""


class ShopError(Exception):
    """Algo que el taller no puede hacer; str(error) es el mensaje para el mostrador."""


class ValidationError(ShopError):
    """Un dato que no tiene la forma esperada (patente, teléfono, correo)."""


class NotFound(ShopError):
    """Algo que no existe: un cliente, un vehículo, una orden."""


class StockError(ShopError):
    """No alcanza el stock de un repuesto."""


class BookingError(ShopError):
    """Un turno que no se puede dar, cancelar o empezar; str(error) es el motivo exacto."""
