// The frame around every screen: top bar, sidebar, terminal panel, toasts,
// dialogs and shared controls.

const es = {
  "shell.home": "Inicio",
  "shell.weekUsageByProvider": "Consumo de la semana por proveedor",
  "shell.usage.trigger": "Uso de los planes: {summary}. Ver todas las cuentas",
  "shell.usage.title": "Uso de los planes",
  "shell.usage.all": "Ver cuentas y modelos",
  "shell.openAccount": "{name}: {usage}. Abrir la cuenta",
  "shell.noProjects": "Sin proyectos todavía.",
  "shell.terminals": "Terminales",
  "shell.hide": "Ocultar",
  "shell.hideTerminal": "Ocultar terminal",
  "shell.processExited": "proceso terminado",
  "shell.exitCode": "código {code}",
  "shell.dismiss": "Cerrar aviso",
  "shell.customColor": "{label}: color personalizado",
  "shell.sidebar.collapse": "Contraer la barra lateral",
  "shell.sidebar.expand": "Expandir la barra lateral",
  "shell.usage.note": "El % de cada plan lo leo de su CLI, también lo que usas fuera de Hedy. Los tokens son solo lo que corrí yo.",
};

const en: Record<keyof typeof es, string> = {
  "shell.home": "Home",
  "shell.weekUsageByProvider": "This week's usage by provider",
  "shell.usage.trigger": "Plan usage: {summary}. See every account",
  "shell.usage.title": "Plan usage",
  "shell.usage.all": "See accounts and models",
  "shell.openAccount": "{name}: {usage}. Open the account",
  "shell.noProjects": "No projects yet.",
  "shell.terminals": "Terminals",
  "shell.hide": "Hide",
  "shell.hideTerminal": "Hide terminal",
  "shell.processExited": "process exited",
  "shell.exitCode": "code {code}",
  "shell.dismiss": "Dismiss",
  "shell.customColor": "{label}: custom color",
  "shell.sidebar.collapse": "Collapse the sidebar",
  "shell.sidebar.expand": "Expand the sidebar",
  "shell.usage.note": "I read each plan's % from its CLI, including what you use outside Hedy. Token counts are only what I ran.",
};

export default { es, en };
