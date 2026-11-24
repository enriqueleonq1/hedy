"""Los servicios que ofrece el taller."""
from .errors import NotFound
from .models import Service


class Catalog:
    def __init__(self, services: list[Service]):
        self._services = {s.code: s for s in services}

    def all(self) -> list[Service]:
        return list(self._services.values())

    def get(self, code: str) -> Service:
        try:
            return self._services[code.strip().upper()]
        except KeyError:
            raise NotFound(f"Servicio inexistente: {code}") from None

    def exists(self, code: str) -> bool:
        return code.strip().upper() in self._services

    def by_skill(self, skill: str) -> list[Service]:
        return [s for s in self._services.values() if s.skill == skill]
