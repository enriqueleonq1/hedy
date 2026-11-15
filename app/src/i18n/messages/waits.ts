// Waiting for an account's limit to renew ("Esperar"): Hedy resumes the task
// on the same account when it does, never on another one.

const es = {
  "wait.counting": "Se renueva en {time}. Entonces Hedy retoma la tarea con {account}.",
  "wait.countingLabel": "Cuenta atrás hasta que se renueve el límite",
  "wait.unknown": "Esperando a que se renueve. {account} no dijo cuándo: continúa cuando quieras.",
  "wait.resuming": "El límite se renovó. Retomando la tarea con {account}…",
  "wait.ready": "Ya puedes continuar: el límite de {account} se renovó.",
  "wait.sameAccount": "Solo con esta cuenta: Hedy nunca cambia de cuenta por su cuenta.",
  "wait.cancel": "Cancelar espera",
  "wait.continue": "Continuar",
  "wait.board": "sigue en {time}",
  "wait.boardReady": "ya puede continuar",
  "wait.time.hours": "{h} h {m} min",
  "wait.time.minutes": "{m} min",
  "wait.time.minutesSeconds": "{m} min {s} s",
  "wait.time.seconds": "{s} s",
};

const en: Record<keyof typeof es, string> = {
  "wait.counting": "Renews in {time}. Then Hedy resumes the task with {account}.",
  "wait.countingLabel": "Countdown until the limit renews",
  "wait.unknown": "Waiting for it to renew. {account} didn't say when: continue whenever you want.",
  "wait.resuming": "The limit renewed. Resuming the task with {account}…",
  "wait.ready": "You can continue now: {account}'s limit renewed.",
  "wait.sameAccount": "Only with this account: Hedy never switches accounts by itself.",
  "wait.cancel": "Stop waiting",
  "wait.continue": "Continue",
  "wait.board": "resumes in {time}",
  "wait.boardReady": "can continue now",
  "wait.time.hours": "{h} h {m} min",
  "wait.time.minutes": "{m} min",
  "wait.time.minutesSeconds": "{m} min {s} s",
  "wait.time.seconds": "{s} s",
};

export default { es, en };
