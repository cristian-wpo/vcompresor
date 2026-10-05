#!/usr/bin/env bash
# EL COSTE DE CADA NIVEL, MEDIDO DE NUEVO. Los siete, sobre el mismo arbol y en serie.
#
# POR QUE HAY QUE REHACERLO. bench/dinero.py decide que nivel sale mas barato sumando alquiler
# de disco y CPU, y para la parte de CPU usaba velocidades ESCRITAS A MANO de una tanda de
# 445-548 s por vuelta. Desde entonces el carril preflate corre en paralelo y la misma vuelta
# tarda ~170 s. O sea que la tabla que decide el ajuste que se vende por defecto estaba
# calculada con una CPU 2,6 veces mas cara de lo que es. Era el mismo defecto que P10, en la
# tabla que mas dinero mueve.
#
# EN SERIE, un nivel detras de otro, y con el arbol limpio para que el commit identifique el
# codigo. Cada vuelta deja su linea en el libro mayor; dinero.py las LEE de ahi y ya no lleva
# ninguna cifra dentro.
set -uo pipefail
cd "$(dirname "$0")/.."
SUCIO=$(git status --porcelain -- . ':!bench/historia.jsonl' ':!bench/datos.json' ':!bench/panel.html' ':!bench/registros' ':!bench/registros-m3' | wc -l)
[ "$SUCIO" -eq 0 ] || { echo "ABORTA: $SUCIO ficheros de codigo sin commitear"; exit 3; }
C=$(awk '{print $1}' /proc/loadavg)
if (( $(echo "$C > 8" | bc -l) )); then echo "ABORTA: carga $C"; exit 3; fi

IN=/tmp/compresor-f1/corpus
W=$(mktemp -d /tmp/niv.XXXXXX); trap 'rm -rf "$W"' EXIT
echo "commit $(git rev-parse --short HEAD) · carga $C · $(du -sb "$IN" | cut -f1) B de entrada"
echo
for N in 1 3 9 12 15 17 19; do
  rm -f "$W/n.vc"
  bench/mide.sh -e "NIVEL $N" -m arbol2.7g -c "--level $N" -n "coste por nivel para dinero.py" -- \
    ./target/release/vcompresor compress "$IN" -o "$W/n.vc" --level "$N" >/dev/null 2>&1
  echo "  nivel $(printf %2d "$N"): $(stat -c%s "$W/n.vc") B   carga $(awk '{print $1}' /proc/loadavg)"
done
echo
echo "Ahora: python3 bench/dinero.py — las lee del libro mayor, no de aqui."
echo FIN-NIVELES
