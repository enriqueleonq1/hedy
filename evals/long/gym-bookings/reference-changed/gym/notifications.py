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


def waitlist_text(position):
    return f"Clase llena: quedas en lista de espera (posición {position})"


def promoted_text(gym_class):
    return (
        f"Se liberó una plaza en {gym_class.name} del {gym_class.start:%d/%m} "
        f"a las {gym_class.start:%H:%M}: ya tienes tu lugar."
    )


def reminder_text(gym_class):
    return f"Recordatorio: {gym_class.name} mañana a las {gym_class.start:%H:%M}"


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


def pending_lines(state):
    """Los mensajes pendientes como '<socio>: <texto>', y los marca enviados."""
    waiting = pending(state)
    mark_sent(waiting)
    return [f"{m.to}: {m.body}" for m in waiting]
