"""Mensajes a los socios. Se guardan en una bandeja de salida (outbox) y otro
proceso los envía; aquí solo se escriben."""
from .catalog import short_when
from .models import Message


def welcome_text(member):
    return f"Hola {member.name.split()[0]}, te damos la bienvenida al Gimnasio Núcleo."


def confirmation_text(gym_class):
    return f"Reserva confirmada: {gym_class.name} el {short_when(gym_class)}"


def cancellation_text(gym_class):
    return f"Reserva cancelada: {gym_class.name} el {short_when(gym_class)}"


def queue(state, member_id, subject, body, now):
    message = Message(to=member_id, subject=subject, body=body, created_at=now)
    state.outbox.append(message)
    return message


def pending(state):
    return [m for m in state.outbox if not m.sent]


def mark_sent(messages):
    for message in messages:
        message.sent = True


def messages_for(state, member_id):
    return [m for m in state.outbox if m.to == member_id]
