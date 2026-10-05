#!/usr/bin/env bash
# EL CARRIL DE vc-cm EN EL BANCO. La deuda que el camino ha marcado SEIS VECES seguidas.
#
# POR QUE EXISTE. El 01-08 se corrieron seis experimentos sobre el modelo —entrelazar, la curva de
# juntos, el barajado, la memoria igualada contra paq8px, las vistas del estado y la memoria de
# ranura— y NINGUNO paso por bench/mide.sh. Sus numeros viven en ficheros de texto y en mensajes
# de commit: no estan en historia.jsonl, no llevan carga ni ciclos ni RSS, y el panel no los ve.
# Seis veces ya no es un descuido: es que el carril del banco se escribia AL FINAL y tiene que
# escribirse AL PRINCIPIO.
#
#   bench/vccm.sh "<etiqueta>" "<VAR=val VAR=val>" [muestras...]
#
# Ejemplo:
#   bench/vccm.sh "ranura con memoria de byte" "VC_VISTAS=0" /tmp/corpus-hilos/codigo
#   bench/vccm.sh "sin memoria de byte" "VC_VISTAS=0 VC_RACHA=0 VC_TRES=0" /tmp/corpus-hilos/codigo
#
# ⚠️ EN SERIE Y CON EL ARBOL LIMPIO, como el carril de vlz: son medidas de tiempo y una cifra de
# un arbol sin commitear no es reproducible. La guardia esta abajo y aborta.
set -uo pipefail
cd "$(dirname "$0")/.."

ETI=${1:?uso: bench/vccm.sh "<etiqueta>" "<VARS>" [muestras...]}
VARS=${2:-}
shift 2 2>/dev/null || shift 1
MUESTRAS=${*:-/tmp/corpus-hilos/codigo}

BIN=./target/release/vcompresor
[ -x "$BIN" ] || { echo "falta $BIN" >&2; exit 2; }

# LA MISMA GUARDIA QUE bench/vlz.sh, y por el mismo motivo: el 01-08 se tiraron 23 de 33 medidas
# por editar el repo mientras el carril corria. Solo mira AL ARRANCAR.
SUCIO=$(git status --porcelain -- . \
          ':!bench/historia.jsonl' ':!bench/datos.json' ':!bench/panel.html' ':!bench/registros' ':!bench/registros-m3' \
        2>/dev/null | head -3)
if [ -n "$SUCIO" ]; then
  echo "ABORTA: el arbol esta sucio y las medidas saldrian como no reproducibles." >&2
  echo "$SUCIO" >&2
  exit 3
fi

echo
echo "  CARRIL vc-cm · $ETI"
echo "  config: ${VARS:-(por defecto)}"
echo

for m in $MUESTRAS; do
  [ -e "$m" ] || { echo "  no existe $m" >&2; continue; }
  n=$(basename "$m")
  O=$(mktemp /tmp/vccm.XXXXXX.vc)
  # `env` con la lista de variables: asi la config viaja EN LA LINEA DEL BANCO y no en la cabeza
  # de quien la corrio, que es medio motivo de que este carril haga falta.
  env $VARS bench/mide.sh -H vc-cm \
    -e "$ETI · $n" \
    -m "$n" \
    -c "${VARS:-defecto}" \
    -n "Experimento de MODELO sobre vc-cm con --max (o sea el codec de maximo ratio, no el carril rapido). ⚠️ Sin --max el archivador usa zstd y la medida NO seria del mezclador: paso el 01-08 y se publico una curva del carril rapido llamandola «el mezclador» durante una hora. La comprobacion barata es que VC_MEM mueva los bytes; si no los mueve, no esta corriendo vc-cm." \
    -- "$BIN" compress "$m" -o "$O" --level 9 --max
  rm -f "$O"
done
echo
echo "  FIN-VCCM · en bench/historia.jsonl. Regenera el panel: python3 bench/panel.py"
