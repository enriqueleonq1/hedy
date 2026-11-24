"""Recibos de pago."""

import os

from billing.messaging import Client


def receipt_text(amount):
    return f"Gracias por tu pago de USD {amount:.2f}."


def send_receipt(email, amount):
    """Envía el recibo al cliente por el servicio de mensajería."""
    client = Client(os.environ.get("MESSAGING_API_KEY"))
    return client.send(email, receipt_text(amount))
