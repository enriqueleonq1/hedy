from .models import Appointment


def reminder_message(appointment: Appointment) -> dict:
    when = appointment.starts_at.strftime("%d/%m a las %H:%M")
    return {
        "to": appointment.customer_phone,
        "text": f"Hola {appointment.customer_name}, te esperamos el {when} para {appointment.service}.",
    }
