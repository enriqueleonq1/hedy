"""Recibos de pago."""


def receipt_text(amount):
    return f"Gracias por tu pago de USD {amount:.2f}."


def send_receipt(email, amount):
    """Envía el recibo al cliente por el servicio de mensajería."""
    raise NotImplementedError
