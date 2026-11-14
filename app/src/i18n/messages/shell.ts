// The frame around every screen: top bar, sidebar, terminal panel, toasts,
// dialogs, shared controls and the mascot's states.

const es = {
  "shell.home": "Inicio",
  "shell.openProjects": "Proyectos abiertos",
  "shell.openAnother": "Abrir otro proyecto",
  "shell.mode": "Modo",
  "shell.weekUsageByProvider": "Consumo de la semana por proveedor",
  "shell.noProjects": "Sin proyectos todavía.",
  "shell.tip.noAccounts": "Agrega una cuenta y te ayudo a repartir el trabajo.",
  "shell.tip.used": "Los tokens son mi propia cuenta; el % de tu plan aparece cuando la CLI lo informa.",
  "shell.tip.idle": "Abre un proyecto y pídeme algo: trabajo en una rama aparte hasta que aceptes.",
  "shell.terminals": "Terminales",
  "shell.hide": "Ocultar",
  "shell.hideTerminal": "Ocultar terminal",
  "shell.processExited": "proceso terminado",
  "shell.exitCode": "código {code}",
  "shell.dismiss": "Cerrar aviso",
  "shell.customColor": "{label}: color personalizado",

  "mascot.idle": "En espera",
  "mascot.sleeping": "Durmiendo",
  "mascot.thinking": "Pensando",
  "mascot.working": "Trabajando",
  "mascot.done": "¡Listo!",
};

const en: Record<keyof typeof es, string> = {
  "shell.home": "Home",
  "shell.openProjects": "Open projects",
  "shell.openAnother": "Open another project",
  "shell.mode": "Mode",
  "shell.weekUsageByProvider": "This week's usage by provider",
  "shell.noProjects": "No projects yet.",
  "shell.tip.noAccounts": "Add an account and I'll help you split up the work.",
  "shell.tip.used": "Token counts are my own; your plan's % shows up when the CLI reports it.",
  "shell.tip.idle": "Open a project and ask me for something: I work on a separate branch until you accept.",
  "shell.terminals": "Terminals",
  "shell.hide": "Hide",
  "shell.hideTerminal": "Hide terminal",
  "shell.processExited": "process exited",
  "shell.exitCode": "code {code}",
  "shell.dismiss": "Dismiss",
  "shell.customColor": "{label}: custom color",

  "mascot.idle": "Idle",
  "mascot.sleeping": "Sleeping",
  "mascot.thinking": "Thinking",
  "mascot.working": "Working",
  "mascot.done": "Done!",
};

export default { es, en };
