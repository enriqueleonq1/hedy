"""Cliente del servicio de mensajería (en pruebas no sale nada a la red)."""


class Client:
    def __init__(self, api_key):
        if not api_key:
            raise ValueError("falta la clave del servicio")
        self.api_key = api_key
        self.sent = []

    def send(self, to, body):
        self.sent.append((to, body))
        return True
