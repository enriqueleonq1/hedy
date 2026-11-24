"""Reglas de forma de los datos que entran: socios y clases."""
from .models import GymError
from .pricing import PLANS


def normalize_email(email):
    return email.strip().lower()


def check_plan(plan):
    if plan not in PLANS:
        raise GymError(f"Plan desconocido: {plan}")
    return plan


def check_member_id(member_id, members):
    member_id = member_id.strip()
    if not member_id:
        raise GymError("Falta el id del socio")
    if member_id in members:
        raise GymError(f"Ya existe el socio {member_id}")
    return member_id


def check_name(name):
    name = " ".join(name.split())
    if not name:
        raise GymError("Falta el nombre")
    return name


def check_capacity(capacity):
    if not isinstance(capacity, int) or capacity <= 0:
        raise GymError("La capacidad tiene que ser un entero positivo")
    return capacity


def check_class(gym_class):
    """Problemas de una clase, como lista de textos (vacía si está bien)."""
    problems = []
    if gym_class.capacity <= 0:
        problems.append(f"{gym_class.id}: capacidad inválida")
    if gym_class.minutes <= 0:
        problems.append(f"{gym_class.id}: duración inválida")
    if not gym_class.instructor.strip():
        problems.append(f"{gym_class.id}: sin instructor")
    return problems
