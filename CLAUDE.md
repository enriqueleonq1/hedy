# Hedy

App de escritorio (Tauri v2 + Svelte 5) y comando `hedy` para trabajar con varias
IAs de programación (Claude Code, Codex, Gemini, Kimi) y varias cuentas por proveedor.
Decisiones verificadas en `docs/decisiones.md`; léelo antes de tocar adaptadores.

## Comandos

- Compilar: `cargo build`
- Tests: `cargo test`
- Lint (sin advertencias): `cargo clippy --all-targets -- -D warnings`
- Formato: `cargo fmt`
- Prototipos de la fase 0: `cargo run -p hedy-fase0 -- --help`

## Estructura

- `crates/hedy-core`: cuentas, proyectos, git, uso, eventos, adaptadores (fase 1+)
- `crates/hedy-cli`: binario `hedy` (fase 1+)
- `crates/hedy-memory`: servidor MCP de memoria (fase 5)
- `app/`: Tauri + Svelte (fase 2+)
- `prototipos/fase0`: prototipos de validación; no es código de producción

## Reglas que no se negocian (se aplican con código, no solo con texto)

1. Hedy nunca lee, copia ni guarda tokens de sesión. Cada cuenta de suscripción vive en
   su carpeta oficial (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`, `GEMINI_CLI_HOME`) y el login
   se hace con el flujo oficial de cada CLI. Hedy solo elige la carpeta al lanzar.
2. Se usan los binarios oficiales sin modificar.
3. Nunca cambiar solo de una cuenta de suscripción a otra al llegar al límite: avisar y
   ofrecer esperar, pasar a otro proveedor o usar una API key de pago.
4. Proyectos protegidos: solo su cuenta asignada; esa cuenta no se ofrece en otros.
5. API keys en el llavero del sistema (crate `keyring`), nunca en archivos.
6. Las notas de memoria nuevas las aprueba el usuario.
7. Archivos locales que Hedy escriba en un repo van a `.git/info/exclude`.
8. Todo local: sin servidor ni telemetría.

Al lanzar una CLI, limpiar del entorno las variables que cambian la cuenta o la
facturación (`ANTHROPIC_API_KEY`, `CLAUDE_CODE_OAUTH_TOKEN`, `OPENAI_API_KEY`,
`CODEX_API_KEY`, `GEMINI_API_KEY`, etc.). Ver `docs/decisiones.md`.

## Convenciones

- Código, comentarios, commits y UI en español.
- Commits pequeños y descriptivos.
- Si la documentación oficial contradice el plan, avisar y proponer alternativa.
