// The home screen: greeting, plan rings, what needs a look, projects and the latest tasks.

const es = {
  "welcome.greeting": "¡Hola!",
  "welcome.greetingNamed": "¡Hola, {name}!",
  "welcome.plans.title": "Tus planes",
  "welcome.plans.payAsYouGo": "Pago por uso",
  "welcome.plans.noData": "Sin datos",
  "welcome.plans.retry": "Reintentar",
  "welcome.plans.retryLabel": "Volver a consultar el plan de {name}",
  "welcome.usage.total": "En Hedy: {tokens} tokens esta semana{cost}{busiest}.",
  "welcome.usage.cost": " (unos {cost} en API keys)",
  "welcome.usage.busiest": "; {name} fue el proyecto más activo",
  "welcome.addFirstAccount": "Agregar mi primera cuenta",
  "welcome.empty.noAccounts": "Primero agrega una cuenta; luego asígnala a tus proyectos.",
};

const en: Record<keyof typeof es, string> = {
  "welcome.greeting": "Hi!",
  "welcome.greetingNamed": "Hi, {name}!",
  "welcome.plans.title": "Your plans",
  "welcome.plans.payAsYouGo": "Pay as you go",
  "welcome.plans.noData": "No data",
  "welcome.plans.retry": "Retry",
  "welcome.plans.retryLabel": "Check the plan of {name} again",
  "welcome.usage.total": "In Hedy: {tokens} tokens this week{cost}{busiest}.",
  "welcome.usage.cost": " (about {cost} on API keys)",
  "welcome.usage.busiest": "; {name} was the busiest project",
  "welcome.addFirstAccount": "Add my first account",
  "welcome.empty.noAccounts": "Add an account first, then assign it to your projects.",
};

export default { es, en };
