"""Altas y consultas de socios."""
from .models import GymError, Member
from .validation import check_member_id, check_name, check_plan, normalize_email


def add_member(state, member_id, name, email, plan, family=""):
    """Da de alta un socio y lo devuelve."""
    member = Member(
        id=check_member_id(member_id, state.members),
        name=check_name(name),
        email=normalize_email(email),
        plan=check_plan(plan),
        family=family.strip(),
    )
    state.members[member.id] = member
    return member


def get_member(state, member_id):
    try:
        return state.members[member_id]
    except KeyError:
        raise GymError(f"Socio inexistente: {member_id}") from None


def format_member(member):
    """'Ana Pérez (premium) · ana@example.com'."""
    text = f"{member.name} ({member.plan})"
    if member.email:
        text += f" · {member.email}"
    return text


def by_plan(state, plan):
    return sorted((m for m in state.members.values() if m.plan == plan), key=lambda m: m.name)


def search(state, text):
    """Socios cuyo nombre o email contienen `text` (sin distinguir mayúsculas)."""
    text = text.lower()
    return sorted(
        (m for m in state.members.values() if text in m.name.lower() or text in m.email),
        key=lambda m: m.name,
    )


def family_members(state, family):
    if not family:
        return []
    return sorted((m for m in state.members.values() if m.family == family), key=lambda m: m.id)
