# Prototipos de la fase 0

Lanzan las CLIs oficiales con la carpeta de una cuenta y traducen lo que emiten al
idioma común de eventos de Hedy. No son código de producción.

```fish
cargo build -p hedy-fase0
set P (pwd)/target/debug/hedy-fase0
set -x HEDY_MUESTRAS /tmp/hedy-muestras   # opcional: guarda cada mensaje crudo en <cli>.jsonl
mkdir -p $HEDY_MUESTRAS
```

## Sin login (ya probados)

```fish
$P claude-estado --config-dir /tmp/cuenta-vacia          # logueado=false, salida 1
mkdir -p /tmp/codex-vacio; $P codex --home /tmp/codex-vacio --solo-cuenta
$P gemini --home /tmp/gemini-vacio                        # initialize + métodos de login
$P pty --config-dir /tmp/cuenta-vacia --segundos 8 claude # pantalla de bienvenida
```

## Con una cuenta real (prueba de punta a punta)

1. Login oficial en una carpeta nueva (abre el navegador):

   ```fish
   set C ~/.local/share/hedy/cuentas/claude-personal
   mkdir -p $C; env CLAUDE_CONFIG_DIR=$C claude auth login
   ```

2. Pedido que necesita permiso (pregunta en la terminal: `p`, `e` o `r`):

   ```fish
   mkdir -p /tmp/hedy-prueba; cd /tmp/hedy-prueba; git init -q
   $P claude --config-dir $C --cwd . "crea hola.txt con un saludo y luego lista los archivos"
   ```

Codex y Gemini no están instalados globalmente en esta máquina: pasar `--binario <ruta>`.
Codex (`--home`, con `codex login`) y Gemini (`--home`, con `gemini` → Log in with
Google) funcionan igual. La carpeta de Codex debe existir antes del login.
