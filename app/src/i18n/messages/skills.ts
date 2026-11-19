// Learned skills: procedures the AIs propose and the user approves, in the
// memory screen's "Habilidades" tab.

const es = {
  "skills.tabNotes": "Notas",
  "skills.tabSkills": "Habilidades",
  "skills.tabs": "Memoria y habilidades",
  "skills.pendingBadge": "{n} por aprobar",
  "skills.subtitle":
    "Procedimientos que tus IAs descubrieron y tú aprobaste: cómo correr los tests, cómo publicar una versión… Las IAs los consultan antes de averiguarlo otra vez.",
  "skills.newButton": "+ Nueva habilidad",
  "skills.loading": "Cargando las habilidades…",
  "skills.loadError": "No se pudieron leer las habilidades: {error}",
  "skills.protected": "Proyecto protegido: sus habilidades se quedan aquí y no usa tus habilidades de todos los proyectos.",

  "skills.group.proposed": "Propuestas por tus IAs · para aprobar",
  "skills.group.project": "De este proyecto",
  "skills.group.global": "Tuyas · en todos los proyectos",
  "skills.empty": "Todavía no hay habilidades.",
  "skills.emptyHint":
    "Cuando una IA termine una tarea y haya descubierto un procedimiento reutilizable, te lo propondrá aquí. Nada se usa sin tu aprobación.",

  "skills.card.steps": "Pasos",
  "skills.card.showSteps": "Ver los pasos",
  "skills.card.proposed": "propuso",
  "skills.card.updated": " · actualizada {when}",
  "skills.card.fromTask": "de una tarea",
  "skills.card.global": "todos los proyectos",
  "skills.card.approve": "Aprobar",
  "skills.card.remove": "Borrar",
  "skills.card.removeConfirm": "¿Borrarla?",
  "skills.update.title": "Actualización de una habilidad aprobada",
  "skills.update.before": "Ahora",
  "skills.update.after": "Propuesta",
  "skills.update.approve": "Aprobar cambio",
  "skills.update.added": "línea nueva",
  "skills.update.removed": "línea quitada",

  "skills.form.name": "Nombre",
  "skills.form.nameHint": "Minúsculas y guiones, p. ej. correr-tests-e2e. Es el nombre de su carpeta en cada CLI.",
  "skills.form.description": "Cuándo usarla",
  "skills.form.descriptionHint": "Una línea: las IAs la leen para decidir si la necesitan.",
  "skills.form.body": "Pasos (Markdown)",
  "skills.form.where": "Dónde",
  "skills.form.project": "Este proyecto",
  "skills.form.global": "Todos los proyectos",
  "skills.form.saveApprove": "Guardar y aprobar",
  "skills.new.title": "Nueva habilidad",
  "skills.new.save": "Guardar habilidad",
  "skills.new.bodyPlaceholder": "1. npm run build\n2. npm run test:e2e",

  "skills.where.title": "Dónde la leen tus IAs",
  "skills.where.native":
    "Las de este proyecto se escriben como archivos de cada CLI: .claude/skills (Claude Code) y .agents/skills (Codex, Gemini CLI, Kimi Code y Antigravity), en el proyecto y en las carpetas de sus tareas. Hedy los excluye de git en .git/info/exclude.",
  "skills.where.tools":
    "Todas, también las de todos los proyectos, están en las herramientas de Hedy (skill_list y skill_get), para cualquier IA que lea la memoria.",
  "skills.where.own": "Si ya tienes una habilidad tuya con el mismo nombre, Hedy no la toca.",

  "skills.files.written": "Habilidad «{name}» lista: tus IAs la usan dentro de Hedy ({n} tareas abiertas ya la tienen).",
  "skills.files.writtenNone": "Habilidad «{name}» lista: tus IAs la usan dentro de Hedy.",
  "skills.files.removed": "Habilidad «{name}» borrada: {n} archivos quitados.",
  "skills.files.removedNone": "Habilidad «{name}» borrada.",
  "skills.files.skipped": "No se tocó {path}: ya existe y no es de Hedy. Renombra la habilidad para tener las dos.",
};

const en: Record<keyof typeof es, string> = {
  "skills.tabNotes": "Notes",
  "skills.tabSkills": "Skills",
  "skills.tabs": "Memory and skills",
  "skills.pendingBadge": "{n} to approve",
  "skills.subtitle":
    "Procedures your AIs worked out and you approved: how to run the tests, how to cut a release… The AIs check them before working it out again.",
  "skills.newButton": "+ New skill",
  "skills.loading": "Loading the skills…",
  "skills.loadError": "Could not read the skills: {error}",
  "skills.protected": "Protected project: its skills stay here and it does not use your skills for all projects.",

  "skills.group.proposed": "Proposed by your AIs · awaiting approval",
  "skills.group.project": "In this project",
  "skills.group.global": "Yours · in every project",
  "skills.empty": "No skills yet.",
  "skills.emptyHint":
    "When an AI finishes a task and has worked out a reusable procedure, it will propose it here. Nothing is used without your approval.",

  "skills.card.steps": "Steps",
  "skills.card.showSteps": "Show the steps",
  "skills.card.proposed": "proposed",
  "skills.card.updated": " · updated {when}",
  "skills.card.fromTask": "from a task",
  "skills.card.global": "every project",
  "skills.card.approve": "Approve",
  "skills.card.remove": "Delete",
  "skills.card.removeConfirm": "Delete it?",
  "skills.update.title": "Update of an approved skill",
  "skills.update.before": "Now",
  "skills.update.after": "Proposed",
  "skills.update.approve": "Approve change",
  "skills.update.added": "new line",
  "skills.update.removed": "removed line",

  "skills.form.name": "Name",
  "skills.form.nameHint": "Lowercase and dashes, e.g. run-e2e-tests. It is its folder name in each CLI.",
  "skills.form.description": "When to use it",
  "skills.form.descriptionHint": "One line: the AIs read it to decide whether they need it.",
  "skills.form.body": "Steps (Markdown)",
  "skills.form.where": "Where",
  "skills.form.project": "This project",
  "skills.form.global": "Every project",
  "skills.form.saveApprove": "Save and approve",
  "skills.new.title": "New skill",
  "skills.new.save": "Save skill",
  "skills.new.bodyPlaceholder": "1. npm run build\n2. npm run test:e2e",

  "skills.where.title": "Where your AIs read them",
  "skills.where.native":
    "This project's skills are written as each CLI's own files: .claude/skills (Claude Code) and .agents/skills (Codex, Gemini CLI, Kimi Code and Antigravity), in the project and in its task folders. Hedy keeps them out of git in .git/info/exclude.",
  "skills.where.tools":
    "All of them, including those for every project, are in Hedy's tools (skill_list and skill_get), for any AI that reads the memory.",
  "skills.where.own": "If you already have a skill of your own with the same name, Hedy leaves it alone.",

  "skills.files.written": "Skill «{name}» ready: your AIs use it inside Hedy ({n} open tasks have it already).",
  "skills.files.writtenNone": "Skill «{name}» ready: your AIs use it inside Hedy.",
  "skills.files.removed": "Skill «{name}» deleted: {n} files removed.",
  "skills.files.removedNone": "Skill «{name}» deleted.",
  "skills.files.skipped": "{path} was left alone: it exists and is not Hedy's. Rename the skill to keep both.",
};

export default { es, en };
