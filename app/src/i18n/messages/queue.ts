// Writing to the agent while it works: the queue of messages ("En cola").

const es = {
  "queue.composerBusy": "Escribe mientras {agent} trabaja: lo envío cuando termine su turno",
  "queue.add": "Poner en cola",
  "queue.list": "Mensajes en cola",
  "queue.badge": "En cola",
  "queue.badgeHeld": "En pausa",
  "queue.edit": "Editar",
  "queue.editLabel": "Editar el mensaje en cola",
  "queue.remove": "Quitar",
  "queue.removeLabel": "Quitar el mensaje de la cola",
  "queue.save": "Guardar",
  "queue.waiting": "Todo lo que está en cola se envía como un solo mensaje cuando {agent} termine su turno.",
  "queue.waitingCard": "Primero responde la tarjeta de arriba; después se envía la cola.",
  "queue.held.failed": "No se envió: la tarea terminó con error. Nada se envía solo.",
  "queue.held.stopped": "No se envió: detuviste al agente. Nada se envía solo.",
  "queue.held.limit":
    "No se envió: la cuenta llegó a su límite. Hedy no lo envía solo ni lo pasa a otra cuenta; tú decides.",
  "queue.idle": "El agente no está trabajando ahora. Envíalo cuando quieras.",
  "queue.sendNow": "Enviar ahora",
  "queue.sendAfterTurn": "Enviar al terminar el turno",
  "queue.count": "{n} en cola",
};

const en: Record<keyof typeof es, string> = {
  "queue.composerBusy": "Write while {agent} works: I'll send it when its turn ends",
  "queue.add": "Queue",
  "queue.list": "Queued messages",
  "queue.badge": "Queued",
  "queue.badgeHeld": "On hold",
  "queue.edit": "Edit",
  "queue.editLabel": "Edit the queued message",
  "queue.remove": "Remove",
  "queue.removeLabel": "Remove the message from the queue",
  "queue.save": "Save",
  "queue.waiting": "Everything queued goes as a single message when {agent} finishes its turn.",
  "queue.waitingCard": "Answer the card above first; the queue goes after that.",
  "queue.held.failed": "Not sent: the task ended with an error. Nothing goes out by itself.",
  "queue.held.stopped": "Not sent: you stopped the agent. Nothing goes out by itself.",
  "queue.held.limit":
    "Not sent: the account reached its limit. Hedy does not send it by itself or move it to another account; you decide.",
  "queue.idle": "The agent is not working right now. Send it whenever you like.",
  "queue.sendNow": "Send now",
  "queue.sendAfterTurn": "Send when the turn ends",
  "queue.count": "{n} queued",
};

export default { es, en };
