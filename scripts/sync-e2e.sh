#!/usr/bin/env bash
# Personal sync end to end with the real `hedy` binary (docs/decisions.md §34):
# four PCs (data dirs) on one EMPTY private git repo, any server. It uses your
# own git credentials (SSH agent or credential helper; never a URL with a
# token) and the real OS keyring: every PC leaves at the end.
#
#   cargo build -p hedy-cli && scripts/sync-e2e.sh <empty-repo-url> [hedy-binary]
#
# Compaction takes over 100 syncs, so a run against a real server takes a
# few minutes.
set -euo pipefail

URL=${1:?usage: scripts/sync-e2e.sh <empty-repo-url> [hedy-binary]}
HEDY=$(realpath "${2:-target/debug/hedy}")
WORK=$(mktemp -d)
export HEDY_LANGUAGE=es

h() {
  local pc=$1
  shift
  HEDY_DATA_DIR="$WORK/$pc" "$HEDY" "$@"
}

cleanup() {
  for pc in a b c d; do
    [ -d "$WORK/$pc" ] && h "$pc" sync leave --yes >/dev/null 2>&1 || true
  done
  rm -rf "$WORK"
}
trap cleanup EXIT

step() { printf '\n== %s\n' "$*"; }
ok() { printf '   ok: %s\n' "$*"; }
fail() {
  printf '   FALLA: %s\n' "$*"
  exit 1
}
templates() { h "$1" template list | grep -oE 't-[a-z0-9-]+' | sort; }
same_templates() {
  local expected
  expected=$(templates "$1")
  for pc in "${@:2}"; do
    [ "$(templates "$pc")" = "$expected" ] || fail "las plantillas de $pc no son las de $1"
  done
  ok "$* tienen las mismas $(echo "$expected" | grep -c . || true) plantillas"
}

step "Una dirección con token se rechaza"
token_url=$(echo "$URL" | sed -E 's#^https://#https://tok@#')
if [ "$token_url" != "$URL" ]; then
  out=$(h a sync setup "$token_url" --name Casa 2>&1 </dev/null) && fail "aceptó $token_url"
  echo "$out" | grep -q token && ok "rechazada sin clonar nada"
fi

step "Casa configura; palabras equivocadas no dejan nada"
words=$(h a sync setup "$URL" --name Casa </dev/null | grep -oE '[0-9]+\. +[^ ]+' | awk '{print $2}' | tr '\n' ' ')
[ "$(echo "$words" | wc -w)" -eq 12 ] && ok "12 palabras"
echo "uno dos tres" | h b sync join "$URL" --name Oficina >/dev/null 2>&1 && fail "aceptó palabras equivocadas"
[ ! -e "$WORK/b/sync/repo" ] && ok "sin clon tras el intento fallido"

step "Oficina y Portátil se suman; las plantillas viajan"
echo "$words" | h b sync join "$URL" --name Oficina >/dev/null
echo "$words" | h c sync join "$URL" --name Portatil >/dev/null
h b template add t-oficina --text "escrita en la oficina" >/dev/null
h b sync now >/dev/null
h a sync now >/dev/null
h c sync now >/dev/null
same_templates b a c

step "Casa y Oficina sincronizan a la vez"
for i in 1 2 3 4 5; do
  h a template add "t-casa-$i" --text "casa $i" >/dev/null
  h b template add "t-ofi-$i" --text "oficina $i" >/dev/null
  h a sync now >/dev/null &
  h b sync now >/dev/null &
  wait
done
for pc in a b c a b; do h "$pc" sync now >/dev/null; done
same_templates a b c

step "Oficina sale y vuelve: nada se duplica"
h b sync leave --yes >/dev/null
echo "$words" | h b sync join "$URL" --name Oficina >/dev/null
h a sync now >/dev/null
h b sync now >/dev/null
[ "$(h b sync now | grep -oE '[0-9]+ enviados')" = "0 enviados" ] && ok "nada pendiente"
same_templates a b

step "Casa quita a Portátil"
h a sync remove Portatil --yes >/dev/null
out=$(h c sync now 2>&1) && fail "Portátil sigue sincronizando"
echo "$out" | grep -q "quitó" && ok "Portátil sabe que la quitaron"
h a sync | grep -q Portatil && fail "Casa aún lista a Portátil"
ok "Casa ya no la lista"

step "Más de 100 sincronizaciones: Casa compacta"
for i in $(seq 1 105); do
  h a template add "t-lote-$i" --text "lote $i" >/dev/null
  h a sync now >/dev/null
done
clone="$WORK/clone"
git clone --quiet "$URL" "$clone"
casa=$(h a sync | grep "esta PC" | awk '{print $NF}')
batches=$(find "$clone/devices/$casa/changes" -name '*.hedy' 2>/dev/null | wc -l)
snapshots=$(find "$clone/devices/$casa" -maxdepth 1 -name 'snapshot-*' | wc -l)
[ "$batches" -le 100 ] && [ "$snapshots" -eq 1 ] && ok "$batches lotes y 1 snapshot"

step "Oficina (atrasada) y una PC nueva lo reciben todo"
h b sync now >/dev/null
echo "$words" | h d sync join "$URL" --name Nueva >/dev/null
same_templates a b d

step "Todas salen"
for pc in a b c d; do h "$pc" sync leave --yes >/dev/null; done
if command -v secret-tool >/dev/null; then
  left=$(secret-tool search --all service hedy 2>/dev/null | grep -c '^\[' || true)
  [ "$left" -eq 0 ] && ok "el llavero no guarda nada"
fi
printf '\nTodo bien.\n'
