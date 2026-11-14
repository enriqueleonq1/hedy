// Ajustes / Settings.

const es = {
  "settings.appearance": "Apariencia",
  "settings.theme": "Tema",
  "settings.theme.system": "Según el sistema",
  "settings.theme.light": "Claro",
  "settings.theme.dark": "Oscuro",
  "settings.name": "Tu nombre",
  "settings.nameHint": "Para saludarte en la bienvenida.",
  "settings.colors": "Colores de cuenta",
  "settings.noAccounts": "Aún no hay cuentas.",
  "settings.mascotStates": "Estados de la mascota",
  "settings.tryColor": "Probar color",
  "settings.tryColorLabel": "Color de prueba de la mascota",
  "settings.cliPaths": "Rutas de las CLIs",
  "settings.cliPathsHint": "Déjalo vacío si el comando está en tu PATH.",
  "settings.apiUrlHint": "Déjalo vacío para usar la dirección oficial. Kimi Code o la región de China tienen las suyas.",
  "settings.apiUrl": "{name}: dirección",
  "settings.priceHint":
    "Precio para estimar el costo de las tareas por API: entrada/salida en USD por millón de tokens, como lo publica el proveedor (por ejemplo, 0.60/2.50). Sin precio, Hedy solo cuenta tokens.",
  "settings.price": "{name}: precio",
  "settings.pricePlaceholder": "entrada/salida",
  "settings.data": "Datos",
  "settings.dataDir": "Hedy guarda todo en tu equipo:",
  "settings.dataHintBefore": "Para usar otra carpeta, abre Hedy con la variable",
  "settings.dataHintAfter":
    ". Los logins de suscripción viven en la carpeta oficial de cada cuenta; las API keys, en el llavero del sistema.",
  "settings.codeMap.hintBefore":
    "Un índice local del código (funciones, llamadas, rutas) que todas tus IAs pueden consultar para gastar menos tokens leyendo archivos. Usa",
  "settings.codeMap.hintAfter": " (MIT, sin telemetría); el índice se guarda en tu PC.",
  "settings.codeMap.installedAt": "Instalado en",
  "settings.codeMap.register": "Registrar el mapa del código en las tareas",
  "settings.codeMap.missing": "Configurado como {path}, pero no lo encuentro.",
  "settings.codeMap.notInstalled": "No está instalado.",
  "settings.codeMap.installTitle": "Instalar mapa del código",
  "settings.codeMap.install": "Instalar",
  "settings.codeMap.note":
    "Se instala con el script oficial, que verifica la descarga, y sin tocar la configuración de tus CLIs: Hedy lo registra en cada tarea, limitado a la carpeta del proyecto. Al terminar, vuelve a esta pantalla para activarlo.",
};

const en: Record<keyof typeof es, string> = {
  "settings.appearance": "Appearance",
  "settings.theme": "Theme",
  "settings.theme.system": "System default",
  "settings.theme.light": "Light",
  "settings.theme.dark": "Dark",
  "settings.name": "Your name",
  "settings.nameHint": "Used to greet you on the home screen.",
  "settings.colors": "Account colors",
  "settings.noAccounts": "No accounts yet.",
  "settings.mascotStates": "Mascot states",
  "settings.tryColor": "Try a color",
  "settings.tryColorLabel": "Mascot preview color",
  "settings.cliPaths": "CLI paths",
  "settings.cliPathsHint": "Leave empty if the command is on your PATH.",
  "settings.apiUrlHint": "Leave empty to use the official URL. Kimi Code and the China region have their own.",
  "settings.apiUrl": "{name}: URL",
  "settings.priceHint":
    "Price used to estimate the cost of API tasks: input/output in USD per million tokens, as the provider publishes it (for example, 0.60/2.50). Without a price, Hedy only counts tokens.",
  "settings.price": "{name}: price",
  "settings.pricePlaceholder": "input/output",
  "settings.data": "Data",
  "settings.dataDir": "Hedy keeps everything on your computer:",
  "settings.dataHintBefore": "To use another folder, start Hedy with the variable",
  "settings.dataHintAfter":
    ". Subscription logins live in each account's official folder; API keys, in the system keychain.",
  "settings.codeMap.hintBefore":
    "A local index of your code (functions, calls, routes) that all your AIs can query to spend fewer tokens reading files. It uses",
  "settings.codeMap.hintAfter": " (MIT, no telemetry); the index stays on your PC.",
  "settings.codeMap.installedAt": "Installed at",
  "settings.codeMap.register": "Register the code map in tasks",
  "settings.codeMap.missing": "Set to {path}, but I can't find it.",
  "settings.codeMap.notInstalled": "Not installed.",
  "settings.codeMap.installTitle": "Install code map",
  "settings.codeMap.install": "Install",
  "settings.codeMap.note":
    "It installs with the official script, which verifies the download, without touching your CLIs' settings: Hedy registers it in each task, limited to the project folder. When it's done, come back here to turn it on.",
};

export default { es, en };
