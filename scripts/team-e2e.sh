#!/usr/bin/env bash
# Team memory end to end with the real `hedy` binary (docs/decisions.md §35):
# three people (one with two PCs) on one EMPTY private git repo, any server.
# It uses your own git credentials (SSH agent or credential helper; never a
# URL with a token) and the real OS keyring: every PC leaves at the end.
#
#   cargo build -p hedy-cli && scripts/team-e2e.sh <empty-repo-url> [hedy-binary]
set -euo pipefail

URL=${1:?usage: scripts/team-e2e.sh <empty-repo-url> [hedy-binary]}
HEDY=$(cd "$(dirname "${2:-$(dirname "$0")/../target/debug/hedy}")" && pwd)/$(basename "${2:-hedy}") || exit 1
[ -x "$HEDY" ] || { echo "no está el binario $HEDY (cargo build -p hedy-cli)" >&2; exit 1; }
WORK=$(mktemp -d)
export HEDY_LANGUAGE=es

h() {
  local pc=$1
  shift
  HEDY_DATA_DIR="$WORK/$pc" "$HEDY" "$@"
}

# Team keys in the keyring (other Hedy entries do not count).
keyring() {
  if command -v secret-tool >/dev/null; then
    # secret-tool prints the attributes on stderr.
    secret-tool search --all service hedy 2>&1 | grep -c ':team:' || true
  else
    echo 0
  fi
}
KEYRING_BEFORE=$(keyring)

PCS=(ana luis laptop marta)
cleanup() {
  for pc in "${PCS[@]}"; do
    [ -d "$WORK/$pc" ] || continue
    # Here only: a failed run must not leave keys behind (an admin's leave
    # waits for another admin).
    while h "$pc" team leave --here --yes >/dev/null 2>&1; do :; done
  done
  rm -rf "$WORK"
  if [ "$(keyring)" -gt "$KEYRING_BEFORE" ]; then
    echo "   AVISO: quedaron entradas de equipo en el llavero" >&2
  fi
}
trap cleanup EXIT

step() { printf '\n== %s\n' "$*"; }
ok() { printf '   ok: %s\n' "$*"; }
fail() {
  printf '   FALLA: %s\n' "$*"
  exit 1
}
# The code `hedy team invite` prints (number and four words).
invite() { h "$1" team invite "${@:2}" | grep -oE '^  [0-9]+-[^ ]+' | tr -d ' '; }
notes() { h "$1" team notes | grep ' — ' | sed -E 's/ — .*//; s/^  //' | sort; }

step "Ana crea el equipo"
h ana team create "$URL" --name "Tienda SA" --me Ana --pc Portátil >/dev/null
h ana team | grep -q "Ana · tú  admin" || fail "Ana no es admin"
ok "Ana es su admin"

step "Ana invita a Luis (miembro) y a Marta (admin)"
CODE=$(invite ana)
h luis team join "$URL" "$CODE" --me Luis --pc "PC de Luis" | grep -q "Pedido enviado" || fail "Luis no pidió entrar"
CODE=$(invite ana --admin)
h marta team join "$URL" "$CODE" --me Marta --pc "PC de Marta" >/dev/null
h ana team now >/dev/null
h luis team now | grep -q "Ya estás en el equipo" || fail "Luis no entró"
h marta team now | grep -q "Ya estás en el equipo" || fail "Marta no entró"
h marta team | grep -q "Marta · tú  admin" || fail "Marta no es admin"
ok "los dos entraron, con su rol"

step "Un código equivocado quema la invitación"
CODE=$(invite ana)
WRONG="${CODE%%-*}-abaco-abaco-abaco-abaco"
h laptop team join "$URL" "$WRONG" --me Intruso --pc X >/dev/null
h ana team now >/dev/null
h ana team | grep -q "no funcionó" || fail "Ana no ve la invitación quemada"
h laptop team now >/dev/null 2>&1 && fail "el intruso sigue esperando"
h laptop team leave --yes >/dev/null
ok "quemada, y el intruso no entra"

step "Luis suma su laptop (la firma un admin)"
CODE=$(invite luis --mine)
h laptop team join "$URL" "$CODE" --me Luis --pc Laptop >/dev/null
h luis team now >/dev/null
h ana team now >/dev/null
h laptop team now | grep -q "Ya estás en el equipo" || fail "la laptop no entró"
h ana team now >/dev/null
h ana team | grep -E "^  Luis " | grep -qE "PC de Luis, Laptop|Laptop, PC de Luis" || { h ana team; fail "Luis no tiene dos PCs"; }
ok "Luis tiene dos PCs"

step "Notas, sugerencias y revisión"
h ana team add --everyone --category rule "Precios en USD con dos decimales" | grep -q "Publicada" || fail "Ana no publicó"
h luis team add --everyone "Todo en inglés" | grep -q "sugerencia" || fail "la de Luis no es sugerencia"
h luis team now >/dev/null
h marta team now >/dev/null
ID=$(h marta team review | grep -E '^  [0-9a-f]{6}  ' | head -1 | awk '{print $1}')
[ -n "$ID" ] || fail "Marta no ve la sugerencia"
h marta team review "$ID" --accept >/dev/null
h marta team now >/dev/null
for pc in ana luis laptop marta; do h "$pc" team now >/dev/null; done
for pc in ana luis laptop marta; do
  [ "$(notes "$pc")" = "$(printf 'Precios en USD con dos decimales\nTodo en inglés')" ] || fail "$pc no tiene las dos notas"
done
ok "las cuatro PCs tienen las dos notas"

step "El repo no guarda nada legible"
CLONE="$WORK/clone"
git clone -q "$URL" "$CLONE"
if grep -rqE "Precios|inglés|Tienda SA|Marta" "$CLONE" --exclude-dir=.git; then
  fail "el repo tiene texto legible"
fi
ok "todo sellado"

step "Ana quita a Luis (sus dos PCs)"
h ana team remove Luis --yes >/dev/null
h luis team now >/dev/null 2>&1 && fail "Luis sigue leyendo"
h laptop team now >/dev/null 2>&1 && fail "la laptop sigue leyendo"
h marta team now >/dev/null
[ "$(h marta team | grep -cE '^  (Ana|Marta)')" = 2 ] || fail "Marta no ve solo a Ana y a ella"
[ "$(notes marta)" = "$(printf 'Precios en USD con dos decimales\nTodo en inglés')" ] || fail "la nota de Luis se perdió"
ok "Luis está fuera; su nota sigue con su nombre"

step "Todos salen"
h luis team leave --yes >/dev/null
h laptop team leave --yes >/dev/null
h ana team leave --yes >/dev/null
h marta team now >/dev/null
h ana team now >/dev/null 2>&1 || true
h marta team leave --yes >/dev/null
for pc in ana luis laptop marta; do
  h "$pc" team | grep -q "no está en ningún equipo" || fail "$pc sigue en el equipo"
done
ok "nadie queda"

printf '\nTodo bien.\n'
