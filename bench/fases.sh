#!/usr/bin/env bash
# LA MEDIDA DIRECTA: cuanto cuesta cada fase, EN UNA SOLA PASADA y sin restar nada.
#
# Sale de P15 —«¿es esta la mejor prueba posible, o solo una valida?»— y de un error concreto
# de hoy que conviene tener delante:
#
#   Para saber lo que cuesta la fase «agrupar» se gastaron SEIS compresiones del arbol entero,
#   media hora de maquina, restando dos totales de 305 segundos. Salio 0,78%, que caia por
#   debajo del suelo de ruido de 2,05%: media hora para no poder afirmar nada.
#
#   VC_FASES lleva la cuenta de CADA FASE por dentro y lo dice al acabar. UNA pasada, sin
#   restar, sin ruido de dos totales que se cancelan mal. Estaba en el codigo esa misma mañana.
#
# LA JERARQUIA, que es lo que P15 pide mirar antes de escribir una prueba:
#   1. medir DIRECTAMENTE la cantidad          <- esto
#   2. restar dos medidas directas             <- los ruidos se suman
#   3. restar dos TOTALES donde lo tuyo es una parte pequeña   <- el ruido te ahoga
#
# Casi siempre se escribe la 3 porque es la que sale sola. Casi siempre existe una 1 que nadie
# busco. Este guion es la 1 para todo lo que sea «cuanto cuesta esta fase».
#
# NOTA HONESTA sobre lo que esto NO sustituye: apagar una pieza cambia el trabajo de las que
# vienen despues (sin agrupar, los ficheros llegan en otro orden y el compresor ve otra cosa).
# El desglose por fases dice lo que cuesta la fase; la ablacion dice lo que cuesta la DECISION.
# Son dos preguntas y hacen falta las dos. Lo que estaba mal era usar la ablacion —cara y
# ruidosa— para contestar la primera.
set -uo pipefail
cd "$(dirname "$0")/.."
VC=$PWD/target/release/vcompresor
IN=${1:-/tmp/compresor-f1/corpus}
CARGA=$(awk '{print $1}' /proc/loadavg)
(( $(echo "$CARGA > 8" | bc -l) )) && { echo "ABORTA: carga $CARGA — el desglose es de TIEMPO"; exit 3; }
[ -d "$IN" ] || { echo "no esta $IN"; exit 3; }

W=$(mktemp -d /tmp/fs.XXXXXX)
REG=bench/registros/fases; mkdir -p "$REG"
echo "carga: $CARGA · $(find "$IN" -type f | wc -l) ficheros · UNA sola pasada"
echo

VC_FASES=1 bench/mide.sh -e "F10 desglose por fases (medida directa)" \
  -m "$(basename "$IN")" -c "todo encendido" \
  -n "P15: la fase se mide DIRECTAMENTE en una pasada, no restando dos totales" \
  -s "$REG/F10-directo.txt" \
  -- "$VC" compress "$IN" -o "$W/o.vc" --level 9
rm -rf "$W"

echo
echo "=== LO QUE CUESTA CADA FASE, SIN RESTAR NADA ==="
cat "$REG/F10-directo.txt"
echo
echo "  Compara: para «agrupar» esto son minutos y una pasada. La ablacion eran seis"
echo "  compresiones del arbol entero y aun asi el numero quedaba bajo el ruido."
echo FIN-FASES
