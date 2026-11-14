// The home screen: greeting, the week's usage and the project cards.

const es = {
  "welcome.greeting": "¡Hola!",
  "welcome.greetingNamed": "¡Hola, {name}!",
  "welcome.question": "¿En qué trabajamos hoy?",
  "welcome.mascot": "Hedy te saluda",
  "welcome.weekUsage": "Consumo de esta semana",
  "welcome.usage.empty": "Aún no hay datos. Empezaré a registrarlo cuando trabajemos desde el chat de Hedy.",
  "welcome.usage.loading": "Sumando lo de esta semana…",
  "welcome.usage.total": "Llevamos {tokens} tokens{cost}{busiest}.",
  "welcome.usage.cost": " (unos {cost})",
  "welcome.usage.busiest": "; {name} fue el proyecto más activo",
  "welcome.addFirstAccount": "Agregar mi primera cuenta",
  "welcome.empty.hasAccounts": "Agrega una carpeta de código y elige con qué cuenta trabaja.",
  "welcome.empty.noAccounts": "Primero agrega una cuenta; luego asígnala a tus proyectos.",
  "welcome.card.workHere": "Trabajar aquí",
  "welcome.card.cliInTerminal": "{cli} en terminal",
};

const en: Record<keyof typeof es, string> = {
  "welcome.greeting": "Hi!",
  "welcome.greetingNamed": "Hi, {name}!",
  "welcome.question": "What are we working on today?",
  "welcome.mascot": "Hedy says hi",
  "welcome.weekUsage": "This week's usage",
  "welcome.usage.empty": "No data yet. I'll start tracking it once we work from Hedy's chat.",
  "welcome.usage.loading": "Adding up this week…",
  "welcome.usage.total": "{tokens} tokens so far{cost}{busiest}.",
  "welcome.usage.cost": " (about {cost})",
  "welcome.usage.busiest": "; {name} was the busiest project",
  "welcome.addFirstAccount": "Add my first account",
  "welcome.empty.hasAccounts": "Add a code folder and choose which account works on it.",
  "welcome.empty.noAccounts": "Add an account first, then assign it to your projects.",
  "welcome.card.workHere": "Work here",
  "welcome.card.cliInTerminal": "{cli} in terminal",
};

export default { es, en };
