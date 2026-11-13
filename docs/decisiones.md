# Decisiones — Fase 0 (validación)

Verificado el 13 de noviembre de 2026 en Linux (CachyOS, fish).

| CLI | Versión probada |
| --- | --- |
| Claude Code | 2.1.306 |
| Codex | 0.169.0 |
| Gemini CLI | 0.68.0 |

Fuentes: documentación oficial actual, `--help` de cada binario, el JSON Schema que
genera `codex app-server` y los prototipos de `prototipos/fase0`. Cuando algo no
está documentado oficialmente, se marca como **no verificado**.

> Las CLIs cambian casi cada semana. Hedy debe guardar la versión de cada binario
> y avisar cuando cambie (`hedy doctor`). Lo que dice este documento vale para
> las versiones de la tabla.

---

## 1. Recomendación: modo mixto

| Proveedor | Chat de Hedy (eventos + permisos) | Pestaña Terminal y `hedy run` |
| --- | --- | --- |
| Claude Code | `claude -p` con stream-json en ambos sentidos | `claude` interactivo en PTY |
| Codex | `codex app-server` (JSON-RPC por stdio) | `codex` interactivo en PTY |
| Gemini ⚠️ | `gemini --acp` (Agent Client Protocol) | `gemini` interactivo en PTY |
| Kimi | HTTP compatible con OpenAI (`reqwest`) | — (o Kimi Code CLI en PTY) |

⚠️ Gemini CLI ya **no acepta cuentas personales de Google** (ver sección 5): solo
sirve con Code Assist Standard/Enterprise o con API key de pago.

**Por qué mixto:**

- El chat propio necesita eventos estructurados y **pedir permiso** antes de cada
  comando o edición. Los tres modos elegidos permiten que Hedy responda a esas
  peticiones; la PTY solo da texto en pantalla, imposible de traducir de forma fiable.
- La PTY sigue siendo necesaria: es donde el usuario hace el **login oficial** la
  primera vez (cada CLI muestra su propio asistente) y es lo que usa `hedy run`.
- Los tres protocolos estructurados son de las propias CLIs oficiales sin modificar.

**Por qué no ACP para todo:** Gemini habla ACP de forma nativa, pero Claude Code y
Codex solo lo hacen mediante adaptadores de terceros (`claude-code-acp`, `codex-acp`)
que añaden una capa más y otro binario. Para esos dos es más directo su protocolo
nativo. Si en el futuro se agregan más CLIs con ACP (Kimi Code, por ejemplo),
el adaptador ACP de Gemini se reutiliza.

---

## 2. Separación de cuentas por carpeta

| CLI | Variable | Dónde queda el login (Linux) | Comprobar sin leer tokens |
| --- | --- | --- | --- |
| Claude Code | `CLAUDE_CONFIG_DIR` | `$CLAUDE_CONFIG_DIR/.credentials.json` (0600) | `claude auth status --json` (salida 0 = logueado, 1 = no) |
| Codex | `CODEX_HOME` (la carpeta debe existir) | `$CODEX_HOME/auth.json` si `cli_auth_credentials_store = "file"` | `codex login status` (salida 0 / 1) o `account/read` del app-server |
| Gemini | `GEMINI_CLI_HOME` (crea `.gemini/` dentro) | `$GEMINI_CLI_HOME/.gemini/oauth_creds.json` | No hay comando. `google_accounts.json` dice qué correo está activo (sin tokens) |

Probado con carpetas vacías: cada CLI escribió solo dentro de su carpeta y no tocó
`~/.claude`, `~/.codex` ni `~/.gemini`.

### Claude Code

- `CLAUDE_CONFIG_DIR` está documentado (code.claude.com/docs/en/env-vars): mueve
  ajustes, historial, plugins, credenciales y también `.claude.json`.
- **Detalle importante:** sin la variable, Claude usa `~/.claude/` **y**
  `~/.claude.json` (fuera de la carpeta). Con la variable, busca
  `$CLAUDE_CONFIG_DIR/.claude.json`. Por eso **no** se debe "adoptar" la cuenta
  existente poniendo `CLAUDE_CONFIG_DIR=~/.claude`: perdería los MCP y la
  configuración de `~/.claude.json`. La cuenta que ya existe se registra en Hedy
  como "carpeta por defecto" y se lanza **sin** la variable.
- En esta máquina la cuenta por defecto (`~/.claude`) es una suscripción **Team**,
  o sea la cuenta del trabajo. Ver "Decisiones abiertas".
- Login en una carpeta nueva: `CLAUDE_CONFIG_DIR=<carpeta> claude auth login`
  (o `/login` dentro de `claude`). Ambos son el flujo oficial.
- `claude auth status --json` devuelve `loggedIn`, `authMethod`, `subscriptionType`,
  `configDirectory` y también correo y organización. Hedy solo guarda los campos
  que no identifican a la persona salvo que el usuario quiera ver el correo.
- Aviso de la documentación: dos inicios de sesión de **Claude Console sin API key**
  no se separan por carpeta. No afecta a cuentas de suscripción.

### Codex

- `CODEX_HOME` está documentado (learn.chatgpt.com/docs/config-file/environment-variables):
  config, auth, logs, sesiones y skills.
- El login puede ir a `auth.json` o al llavero del sistema. Cómo se nombra la entrada
  del llavero por carpeta **no está documentado**, así que Hedy fuerza
  `cli_auth_credentials_store = "file"` en el `config.toml` de cada cuenta. El archivo
  lo escribe y lee Codex; Hedy no lo abre.
- Login: `CODEX_HOME=<carpeta> codex login` (navegador) o `codex login --device-auth`.

### Gemini

- `GEMINI_CLI_HOME` está documentado en la referencia de configuración que viene con
  el paquete (`docs/reference/configuration.md`).
- Por defecto el login de Google queda en `oauth_creds.json` dentro de la carpeta.
- **Riesgo encontrado en el código:** con `GEMINI_FORCE_ENCRYPTED_FILE_STORAGE=true`
  el login va al llavero con servicio `gemini-cli-oauth` y una **clave fija**
  (`main-account`), igual para todas las carpetas. Dos cuentas se pisarían.
  Hedy quita esa variable del entorno.

### Variables que Hedy quita siempre (aplicado en código)

Si alguna de estas variables llega al proceso hijo, la CLI usa esa credencial en vez
del login de la carpeta, o factura por API sin avisar:

- Claude: `ANTHROPIC_API_KEY`, `ANTHROPIC_AUTH_TOKEN`, `CLAUDE_CODE_OAUTH_TOKEN`.
  La documentación dice que en `-p` la API key "se usa siempre que exista".
- Codex: `OPENAI_API_KEY`, `CODEX_API_KEY`, `CODEX_ACCESS_TOKEN`.
- Gemini: `GEMINI_API_KEY`, `GOOGLE_API_KEY`, `GOOGLE_GENAI_USE_VERTEXAI`,
  `GEMINI_FORCE_ENCRYPTED_FILE_STORAGE`.
- Restos de la sesión que lanzó a Hedy: `CLAUDECODE`, `CLAUDE_CODE_*`,
  `CLAUDE_CONFIG_DIR`, `CODEX_HOME` y `GEMINI_CLI_HOME`. Hedy pone después la
  variable de la cuenta elegida.

Ver `entorno_limpio()` en `prototipos/fase0/src/main.rs`. En la fase 1 pasa a
`hedy-core` con tests.

---

## 3. Claude Code

### Modo headless

```
claude -p --input-format stream-json --output-format stream-json --verbose \
       --permission-prompt-tool stdio --permission-mode manual [--model <alias>]
```

- Entrada (stdin, una línea por mensaje):
  `{"type":"user","message":{"role":"user","content":"..."},"parent_tool_use_id":null,"session_id":""}`.
  El proceso queda vivo y acepta más mensajes; cerrar stdin lo termina.
- Salida: `system` (`init`, `compact_boundary`, `api_retry`, `status`, hooks…),
  `assistant`, `user` (resultados de herramientas), `stream_event` (con
  `--include-partial-messages`), `rate_limit_event`, `result`, y
  `control_request` / `control_response`.
- `result` trae `subtype` (`success`, `error_max_turns`, `error_during_execution`,
  `error_max_budget_usd`…), `is_error`, `duration_ms`, `num_turns`, `usage`,
  `modelUsage`, `total_cost_usd` y `permission_denials`. `total_cost_usd` es una
  **estimación del cliente**; la documentación avisa que puede no coincidir con la factura.
- Otros flags útiles: `--resume <id>`, `--session-id <uuid>`, `--fork-session`,
  `--mcp-config` + `--strict-mcp-config`, `--append-system-prompt`,
  `--max-budget-usd`, `--include-hook-events`, `--no-session-persistence`.
- `--permission-mode` acepta `default` (alias `manual`), `acceptEdits`, `plan`,
  `auto`, `dontAsk` y `bypassPermissions`. `plan` sirve para la fase "planificar".

### Permisos: tres vías

1. **Protocolo de control por stdio** (`--permission-prompt-tool stdio`). Claude envía
   `control_request` con `subtype: "can_use_tool"`, `tool_name` e `input`. Hedy
   responde con `control_response` `{"behavior":"allow","updatedInput":…}` o
   `{"behavior":"deny","message":…}`. Es lo que usa el Agent SDK oficial. Probado:
   el handshake `initialize` responde. **Riesgo:** está documentado solo en parte
   (el SDK lo usa, pero no hay especificación pública del formato).
2. **Herramienta MCP de permisos** (`--permission-prompt-tool mcp__hedy__permiso`).
   Documentada oficialmente: la herramienta devuelve un bloque de texto con
   `{"behavior":"allow"}` o `{"behavior":"deny","message":"…"}`. Requiere que Hedy
   sirva un MCP pequeño (ya hará falta uno para la memoria).
3. **Hook `PermissionRequest`**. Documentado y funciona en `-p`.

**Decisión propuesta:** usar la vía 1 y tener la vía 2 implementada como respaldo
detrás de la misma interfaz. Si una actualización rompe la 1, se cambia sin tocar la UI.
"Permitir en este chat" lo guarda Hedy por sesión y responde solo, sin volver a preguntar.

### Uso y límites

- `rate_limit_event` en el stream: `status` (`allowed`, `allowed_warning`, `rejected`),
  `resetsAt` y `utilization`. La documentación no dice a qué ventana (5 h o semanal)
  corresponde `utilization`. **No verificado** con una cuenta real todavía.
- La barra de estado (`statusLine`) recibe `rate_limits.five_hour` y
  `rate_limits.seven_day` con porcentaje y hora de renovación, solo en planes
  Pro/Max/Ultra y tras la primera respuesta. Opción para sesiones PTY: Hedy registra
  su propio comando `statusLine` (vía `--settings`), que solo escribe esos porcentajes.
- No hay métricas de límites por OpenTelemetry.

### ¿El uso programático gasta de otro bolsillo? (pregunta de la fase 0)

Fuente: support.claude.com/en/articles/15036540 ("Use the Claude Agent SDK with your Claude plan").

- **15 de junio de 2026:** Anthropic **pausó** el cambio anunciado: "Claude Agent SDK,
  `claude -p` y el uso desde apps de terceros siguen gastando de los límites de tu
  suscripción".
- **7 de octubre de 2026:** los planes Max, Ultra y Team incluyen créditos mensuales de
  API. Esos créditos solo se usan cuando el SDK o `claude -p` funcionan con una
  **API key** de la organización de Console vinculada. Con login de suscripción,
  `claude -p` sigue gastando de los límites del plan.
- **Conclusión:** hoy `claude -p` con login de suscripción gasta del **mismo bolsillo**
  que el uso interactivo. No hay diferencia de costo entre headless y PTY.
- Los montos de esos créditos de API **no están verificados** (no aparecen en el artículo).
- Como esto ya cambió dos veces este año, Hedy mostrará la fuente en la pantalla de
  cuentas y lo revisaremos en cada fase.

### Aviso de política

La documentación del Agent SDK dice: "Salvo aprobación previa, Anthropic no permite a
desarrolladores externos ofrecer el login de claude.ai ni sus límites en sus productos".
Hedy es una herramienta **personal y local** que lanza el binario oficial con el login
que hizo el propio usuario, así que no ofrece el login de claude.ai a terceros. Si
alguna vez se distribuye a otras personas, hay que revisarlo antes.

---

## 4. Codex

La documentación pasó de `developers.openai.com/codex` a `learn.chatgpt.com/docs`.

### App server (recomendado para el chat)

- `codex app-server --listen stdio://`: JSON-RPC 2.0, una línea JSON por mensaje.
  OpenAI lo presenta como "la interfaz que usa Codex para clientes completos".
  Lo marca como experimental; sin `capabilities.experimentalApi` el cliente se queda
  en la parte estable.
- Flujo: `initialize` → notificación `initialized` → `thread/start`
  (`cwd`, `approvalPolicy`, `sandbox`, `model`) → `turn/start`
  (`threadId`, `input: [{type:"text", text}]`).
- Notificaciones: `item/started`, `item/completed` (tipos `agentMessage`, `plan`,
  `reasoning`, `commandExecution`, `fileChange`, `mcpToolCall`…),
  `item/agentMessage/delta`, `turn/plan/updated`, `turn/diff/updated`,
  `thread/tokenUsage/updated`, `account/rateLimits/updated`, `turn/completed`, `error`.
- **Permisos:** el servidor pide `item/commandExecution/requestApproval` e
  `item/fileChange/requestApproval`. Las respuestas `accept` / `acceptForSession` /
  `decline` corresponden 1:1 con **Permitir / Permitir en este chat / Rechazar**.
- **Límites:** `account/rateLimits/read` devuelve `primary` y `secondary`, cada uno con
  `usedPercent`, `windowDurationMins` y `resetsAt`. Hedy lee `windowDurationMins` en
  vez de suponer cuál ventana es la de 5 h y cuál la semanal.
- `codex app-server generate-json-schema --out <dir>` genera el esquema exacto de esa
  versión. Hedy fijará la versión soportada y generará los tipos de Rust a partir de él.
- Probado: `initialize` y `account/read` funcionan sin login (`requiresOpenaiAuth: true`);
  `account/rateLimits/read` responde con error "authentication required".

### Alternativa: `codex exec --json`

JSONL con `thread.started`, `turn.started`, `item.*`, `turn.completed` (con `usage`),
`turn.failed` y `error`. Sirve para tareas de una sola vuelta, pero **no permite pedir
permisos** (solo políticas fijas) y no trae límites. `--full-auto` ya no existe.

### Facturación

"Los mensajes locales y los chats en la nube comparten el uso de tu plan." No hay un
bolsillo aparte para `exec` o el app server documentado; todo apunta a que es el mismo.

---

## 5. Gemini CLI

### ⚠️ Gemini CLI ya no sirve con cuentas personales

- Desde el **18 de junio de 2026**, Gemini CLI dejó de atender a cuentas de consumo:
  el plan gratuito, Google AI Pro y Google AI Ultra. Google lo reemplazó por
  **Antigravity CLI** (`agy`). Fuentes:
  developers.googleblog.com/an-important-update-transitioning-gemini-cli-to-antigravity-cli
  (19 de mayo de 2026) y el aviso que muestra cada página de geminicli.com. El binario
  0.68.0 incluye mensajes que remiten a instalar Antigravity CLI.
- **Sigue funcionando** con Gemini Code Assist Standard/Enterprise y con API keys de
  pago de Gemini. Con Vertex AI y con API keys gratuitas, **no verificado** (las
  fuentes se contradicen).
- **Antigravity CLI** tiene `agy acp` (ACP también), pero guarda el login en el
  **llavero del sistema** y no hay ninguna variable documentada para cambiar su
  carpeta. Con eso no se pueden tener dos cuentas separadas por carpeta. Por ahora
  solo sirve para **una cuenta personal** por máquina.
- Gemini CLI se sigue manteniendo (0.68.0 del 10 de noviembre; 0.69 en preview).

### Lo verificado de Gemini CLI

- Paquete oficial `@google/gemini-cli` (0.68.0). Login: "Log in with Google"
  (`oauth-personal`), API key de Gemini o Vertex AI.
- **ACP** (`gemini --acp`; `--experimental-acp` está obsoleto): JSON-RPC por stdio.
  Métodos `initialize` (devuelve `authMethods`), `authenticate`, `session/new`,
  `session/load`, `session/prompt`, `session/cancel` y `session/set_mode`. El agente
  envía notificaciones `session/update` (`agent_message_chunk`, `tool_call`,
  `tool_call_update`, `plan`…) y pide permiso con `session/request_permission`, con
  opciones `allow_once` / `allow_always` / `reject_once`. Corresponden 1:1 con las
  tarjetas de permiso de Hedy.
- Probado: `initialize` responde con la lista de métodos de login; `session/new` sin
  login devuelve "Gemini API key is missing or not configured".
- **Headless** (`gemini -p … -o stream-json`): eventos `init`, `message`, `tool_use`,
  `tool_result`, `error` y `result` (con estadísticas por modelo). No permite pedir
  permisos: solo `--approval-mode` (`default`, `auto_edit`, `yolo`, `plan`). Sirve
  para `hedy ask`.
- En PTY, la primera vez muestra "Waiting for authentication…" (puede abrir el
  navegador para el login de Google) y la pregunta de confiar en la carpeta.

- Eventos de stream-json (código fuente `packages/core/src/output/types.ts`): todos
  llevan `type` y `timestamp`; `result.stats` incluye `total_tokens`, `input_tokens`,
  `output_tokens`, `cached`, `duration_ms`, `tool_calls` y desglose por modelo.
  Códigos de salida: 0, 1 (error), 42 (entrada inválida), 53 (límite de turnos).
- **Cuota:** hasta 1.500–2.000 pedidos al día según el plan (Code Assist Standard /
  Enterprise). No hay forma de leer la cuota restante desde un programa; `/stats model`
  solo funciona en modo interactivo. Hedy contará los pedidos y lo mostrará como
  **estimación**.
- MCP: `mcpServers` en `settings.json` o `gemini mcp add --scope user|project`.
  El archivo de contexto se puede renombrar con `context.fileName` (por ejemplo, a
  `AGENTS.md`).
- **Términos:** usar el servicio de Code Assist con el login de Gemini CLI desde software
  de terceros (es decir, sin pasar por la CLI) está prohibido y puede causar
  suspensión. Lanzar la CLI oficial sin modificar, como hace Hedy, no está prohibido,
  pero tampoco autorizado de forma explícita.

### Gemini por API (proveedor tipo Kimi)

- Endpoint compatible con OpenAI:
  `https://generativelanguage.googleapis.com/v1beta/openai/` con
  `Authorization: Bearer <GEMINI_API_KEY>`. Soporta `tools`, `stream` y
  `stream_options.include_usage`. Google aún lo marca como beta.
- Modelos para programar: `gemini-4-carbon` ("para ingeniería de software compleja"),
  `gemini-4-hydrogen` ("programación diaria y flujos con agentes") y
  `gemini-3.8-flash`. La familia 2.5 se retira el 1 de diciembre de 2026.
- Así Gemini se usa igual que Kimi: una API key en el llavero y el adaptador HTTP
  común. **Es la forma más simple y segura de agregar Gemini con una cuenta personal.**

---

## 6. Kimi

- La documentación pasó a `platform.kimi.ai` (antes `platform.moonshot.ai`).
- API compatible con OpenAI: `POST https://api.moonshot.ai/v1/chat/completions`
  (China: `api.moonshot.cn`). Soporta `tools`, `stream` y
  `stream_options.include_usage` (devuelve el uso total en el último trozo).
- **Modelos actuales:** `kimi-k3.1-pro` (el más capaz, 1M de contexto),
  `kimi-k3.1-pro-highspeed` y `kimi-k3`. La familia `kimi-k2.*` está obsoleta o se
  retira en diciembre de 2026. Los modelos K3 siempre "piensan" y usan
  `reasoning_effort`.
- También existe un endpoint compatible con Anthropic (`https://api.moonshot.ai/anthropic`).
- **Kimi Code** (suscripción) tiene endpoints y claves propios
  (`https://api.kimi.ai/coding/v1`) y una CLI oficial nueva, `MoonshotAI/kimi-code`,
  que habla ACP (`kimi acp`). La antigua `kimi-cli` está archivada.

---

## 7. PTY (`portable-pty`)

- Funciona con las tres CLIs. En la primera ejecución, cada una muestra su asistente:
  Claude pide elegir tema y luego login, Codex ofrece "Sign in with ChatGPT / Device
  Code / API key", y Gemini pide login y confianza en la carpeta.
- Hedy **no automatiza** esos asistentes: el usuario hace el login en la pestaña
  Terminal. Así se cumple la regla 1.
- `portable-pty` arranca en `$HOME` si no se le indica la carpeta: hay que pasar
  siempre `cwd`.
- Para leer el estado de una sesión PTY no se analiza la pantalla. Se usan los hooks
  y la `statusLine` de Claude, que escriben a un socket de Hedy.

---

## 8. Cambios respecto al documento original

1. **Gemini** se agrega como proveedor (pedido durante la fase 0). Gemini CLI solo
   sirve con cuentas Code Assist Standard/Enterprise o API key de pago. Para una cuenta
   personal, lo práctico es la API de Gemini por HTTP, como Kimi.
2. **Kimi**: los nombres de modelo cambiaron a `kimi-k3.x`. Hay dos formas de usarlo:
   API (pago por uso) o Kimi Code (suscripción, con CLI oficial y ACP).
3. **Codex**: el modo recomendado es el app server, no `exec`, porque `exec` no permite
   pedir permisos.
4. **Claude**: los permisos del chat dependen de un protocolo documentado solo en
   parte. Mitigación: respaldo con la herramienta MCP de permisos.
5. **Política de Anthropic** sobre el login de claude.ai en productos de terceros:
   sin efecto mientras Hedy sea de uso personal.

## 9. Decisiones abiertas

1. **Cuenta por defecto de Claude:** `~/.claude` tiene la cuenta **Team** (trabajo).
   Propuesta: dejarla como "Claude trabajo" (sin variable, protegida y asignada solo a
   Homming) y hacer login de la personal en `~/.local/share/hedy/cuentas/claude-personal`.
   La alternativa es mover el trabajo a una carpeta de Hedy y dejar la personal por
   defecto. Esto afecta a qué cuenta usa `claude` cuando se lanza fuera de Hedy.
2. **Ubicación de las carpetas de cuenta:** propuesta `~/.local/share/hedy/cuentas/<id>/`
   (XDG). La configuración de Hedy va en `~/.config/hedy/`.
3. **Kimi:** ¿API de pago por uso, suscripción Kimi Code, o ambas?
4. **Gemini:** ¿qué cuenta de Google tienes? Las opciones son:
   - **API key de Gemini:** adaptador HTTP; recomendado.
   - **Code Assist Standard/Enterprise:** Gemini CLI con ACP, varias cuentas por carpeta.
   - **AI Pro/Ultra personal:** Antigravity CLI, una sola cuenta por máquina.
5. **Prueba de punta a punta con login real:** pendiente (ver README de los prototipos).
