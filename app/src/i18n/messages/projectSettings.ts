// A project's settings sheet: its account and protection, who does each step,
// its client, color and folder, where its memory lives, and removing it.

const es = {
  "projectSettings.title": "Ajustes de {name}",
  "projectSettings.back": "Volver a {name}",
  "projectSettings.missing": "Este proyecto ya no está en Hedy.",
  "projectSettings.goHome": "Ir a Inicio",

  "projectSettings.account.title": "Cuenta",
  "projectSettings.account.text": "La cuenta con la que trabaja la IA en este proyecto.",
  "projectSettings.account.label": "Cuenta del proyecto",
  "projectSettings.account.unprotectToChange": "Quita la protección para cambiar la cuenta.",
  "projectSettings.protect": "Proteger",
  "projectSettings.protectHint": "Solo usa su cuenta, y esa cuenta no se ofrece en los demás proyectos ni en los chats.",
  "projectSettings.protectNeedsAccount": "Elige una cuenta antes de protegerlo.",

  "projectSettings.steps.title": "Quién hace cada paso",
  "projectSettings.steps.text":
    "Cada paso puede hacerlo otra cuenta o modelo. Si la misma IA planifica e implementa, sigue en la misma sesión; si no, la siguiente recibe el pedido, el plan aprobado y lo hecho hasta ahora.",
  "projectSettings.steps.protected": "Protegido: todos los pasos usan su cuenta; aquí solo eliges el modelo.",
  "projectSettings.step.plan": "Planificar",
  "projectSettings.step.implement": "Implementar",
  "projectSettings.step.tests": "Escribir tests",
  "projectSettings.step.review": "Verificar",
  "projectSettings.step.accountFor": "Quién hace «{step}»",
  "projectSettings.step.modelFor": "Modelo para «{step}»",
  "projectSettings.step.projectAccount": "Cuenta del proyecto",
  "projectSettings.step.projectAccountNamed": "Cuenta del proyecto ({name})",
  "projectSettings.step.skip": "No usar este paso",
  "projectSettings.step.auto": "Automático",
  "projectSettings.step.autoDetail": "Lo elige la IA",

  "projectSettings.general.title": "General",
  "projectSettings.client": "Cliente",
  "projectSettings.clientHint": "Agrupa los proyectos de un mismo cliente para ver cuánto uso lleva cada uno.",
  "projectSettings.color": "Color",
  "projectSettings.colorOf": "Color de {name}",
  "projectSettings.noColor": "Sin color",
  "projectSettings.noColorHint": "Sin color, usa el de su cuenta.",
  "projectSettings.folder": "Carpeta",

  "projectSettings.danger.title": "Zona de peligro",
  "projectSettings.remove": "Quitar de Hedy…",
  "projectSettings.removeConfirm": "¿Quitar {name} de Hedy?",
  "projectSettings.removeHint":
    "Hedy deja de mostrarlo y olvida su cuenta y sus pasos. No se borra nada del disco: la carpeta y su código se quedan donde están.",
};

const en: Record<keyof typeof es, string> = {
  "projectSettings.title": "{name} settings",
  "projectSettings.back": "Back to {name}",
  "projectSettings.missing": "This project is no longer in Hedy.",
  "projectSettings.goHome": "Go to Home",

  "projectSettings.account.title": "Account",
  "projectSettings.account.text": "The account the AI works with on this project.",
  "projectSettings.account.label": "Project account",
  "projectSettings.account.unprotectToChange": "Unprotect the project to change its account.",
  "projectSettings.protect": "Protect",
  "projectSettings.protectHint": "It only uses its account, and that account isn't offered on other projects or in chats.",
  "projectSettings.protectNeedsAccount": "Choose an account before protecting it.",

  "projectSettings.steps.title": "Who does each step",
  "projectSettings.steps.text":
    "Each step can use a different account or model. If the same AI plans and implements, it stays in the same session; otherwise the next one gets the request, the approved plan and the work so far.",
  "projectSettings.steps.protected": "Protected: every step uses its account; here you only choose the model.",
  "projectSettings.step.plan": "Plan",
  "projectSettings.step.implement": "Implement",
  "projectSettings.step.tests": "Write tests",
  "projectSettings.step.review": "Verify",
  "projectSettings.step.accountFor": "Who does “{step}”",
  "projectSettings.step.modelFor": "Model for “{step}”",
  "projectSettings.step.projectAccount": "Project account",
  "projectSettings.step.projectAccountNamed": "Project account ({name})",
  "projectSettings.step.skip": "Skip this step",
  "projectSettings.step.auto": "Automatic",
  "projectSettings.step.autoDetail": "The AI picks it",

  "projectSettings.general.title": "General",
  "projectSettings.client": "Client",
  "projectSettings.clientHint": "Groups the projects of one client to see how much usage each one has.",
  "projectSettings.color": "Color",
  "projectSettings.colorOf": "Color of {name}",
  "projectSettings.noColor": "No color",
  "projectSettings.noColorHint": "Without a color, it uses its account's.",
  "projectSettings.folder": "Folder",

  "projectSettings.danger.title": "Danger zone",
  "projectSettings.remove": "Remove from Hedy…",
  "projectSettings.removeConfirm": "Remove {name} from Hedy?",
  "projectSettings.removeHint":
    "Hedy stops showing it and forgets its account and steps. Nothing on disk is deleted: the folder and its code stay where they are.",
};

export default { es, en };
