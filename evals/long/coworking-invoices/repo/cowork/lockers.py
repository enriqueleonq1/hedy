"""Taquillas: cada socio puede tener una asignada (numeradas del 1 al 40)."""

from .members import find_member

LOCKERS = 40


def _assigned(store):
    """Taquilla -> socio, guardado en el estado."""
    return store.lockers


def assign(store, member_id, number=None):
    """Asigna una taquilla a un socio: la pedida o la primera libre."""
    find_member(store, member_id)
    taken = _assigned(store)
    if member_id in taken.values():
        raise ValueError(f"{member_id} ya tiene taquilla")
    if number is None:
        free = [n for n in range(1, LOCKERS + 1) if n not in taken]
        if not free:
            raise ValueError("No quedan taquillas libres")
        number = free[0]
    if not 1 <= number <= LOCKERS:
        raise ValueError(f"Taquilla inexistente: {number}")
    if number in taken:
        raise ValueError(f"La taquilla {number} está ocupada")
    taken[number] = member_id
    return number


def release(store, member_id):
    taken = _assigned(store)
    for number, owner in list(taken.items()):
        if owner == member_id:
            del taken[number]
            return number
    raise ValueError(f"{member_id} no tiene taquilla")


def locker_of(store, member_id):
    for number, owner in _assigned(store).items():
        if owner == member_id:
            return number
    return None


def lockers_text(store):
    taken = _assigned(store)
    lines = [f"Taquillas ocupadas: {len(taken)} de {LOCKERS}"]
    for number in sorted(taken):
        lines.append(f"  {number:>2}: {taken[number]}")
    return "\n".join(lines)
