#!/usr/bin/env bash
# ¿SE PUEDE CREER LA BARRA DE PROGRESO? Es una prueba de RELOJ, y va en serie.
#
# 🔑 POR QUE ES UNA PRUEBA Y NO UNA MIRADA. La barra decia «100%» once segundos antes de terminar
# y nadie lo vio nunca, porque para verlo hay que mirar el reloj Y la barra A LA VEZ y durante
# toda la corrida. Ojo humano no lo caza: sale bien las tres primeras veces que lo miras.
#
# Mide cuatro cosas, y las cuatro fueron un fallo real el 03-08:
#   cuanto tarda en decir la PRIMERA palabra   (era 25,71 s de 83)
#   el SILENCIO mas largo entre dos lineas     (era 17,2 s)
#   la COLA: cuanto dura el «100%» mentiroso   (era 11,33 s)
#   si RETROCEDE, y cuanto                     (bajaba del 83% al 64%)
# y el error de la barra contra el reloj, decil a decil.
#
# ⚠️ EN SERIE. La barra depende de cuando terminan los frames, y eso depende de la carga: dos
# corridas a la vez se falsean. Va con el mismo cerrojo que las demas de reloj.
set -uo pipefail
cd "$(dirname "$0")/.."
. bench/comun.sh 2>/dev/null || true

CORPUS="${CORPUS:-/tmp/compresor-f1/corpus/codigo /tmp/compresor-f1/corpus/pdfoffice}"
BIN="${BIN:-./target/release/vcompresor}"
W=$(mktemp -d /tmp/avance.XXXXXX)
trap 'rm -rf $W' EXIT

echo "== ¿SE PUEDE CREER LA BARRA? =="
echo "   corpus: $CORPUS"
echo
# shellcheck disable=SC2086
python3 bench/avance.py "$BIN" compress $CORPUS -o "$W/o.vc" --level 19 --progress
