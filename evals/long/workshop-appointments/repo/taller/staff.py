"""Los mecánicos del taller, en el orden del archivo."""
from .errors import NotFound
from .models import Mechanic


class Staff:
    def __init__(self, mechanics: list[Mechanic]):
        self._mechanics = list(mechanics)

    def all(self) -> list[Mechanic]:
        return list(self._mechanics)

    def get(self, mechanic_id: str) -> Mechanic:
        for m in self._mechanics:
            if m.id == mechanic_id:
                return m
        raise NotFound(f"Mecánico inexistente: {mechanic_id}")

    def with_skill(self, skill: str) -> list[Mechanic]:
        return [m for m in self._mechanics if m.can(skill)]

    def skills(self) -> list[str]:
        seen = []
        for m in self._mechanics:
            for s in m.skills:
                if s not in seen:
                    seen.append(s)
        return seen
