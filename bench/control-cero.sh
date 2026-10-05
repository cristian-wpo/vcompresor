#!/usr/bin/env bash
# EL CONTROL DE CERO: la misma cosa, tres veces, sin cambiar nada.
#
# POR QUE EXISTE. El panel no podia decir cual es el suelo de ruido de esta maquina, y no por
# falta de medidas -hay 171- sino porque 166 se tomaron con el arbol SIN COMMITEAR. Con el arbol
# sucio el commit no identifica el codigo, y entonces dos vueltas «de lo mismo» pueden ser dos
# programas distintos. Paso de verdad: las dos vueltas de la implementacion rota del salto
# incremental (148 s) y las dos de la buena (17,7 s) figuraban las cuatro bajo el commit
# 6ed723e, y promediarlas daba un «suelo de ruido» del 90,9% que no era ruido.
#
# ASI QUE ESTO SE CORRE CON EL ARBOL LIMPIO Y SE ABORTA SI NO LO ESTA. Es la unica medida del
# banco cuyo valor depende de que no haya nada sin commitear.
#
# Tres vueltas y no dos: con dos puntos la desviacion es un adorno (R3).
# En SERIE, alternando nada: aqui el objetivo es justamente que no cambie nada entre vueltas.
set -uo pipefail
cd "$(dirname "$0")/.."

# Las mismas exclusiones que mide.sh: el libro mayor y el panel los escribe la propia
# maquina de medir, y no son codigo sin commitear.
SUCIO=$(git status --porcelain -- . ':!bench/historia.jsonl' ':!bench/datos.json' ':!bench/panel.html' ':!bench/registros' ':!bench/registros-m3' | wc -l)
if [ "$SUCIO" -ne 0 ]; then
  echo "ABORTA: el arbol tiene $SUCIO ficheros sin commitear."
  echo "Este control mide el ruido de la MAQUINA, y para eso el codigo tiene que estar"
  echo "identificado por su commit. Commitea primero y vuelve."
  git status --short | head -10
  exit 3
fi
C=$(awk '{print $1}' /proc/loadavg)
if (( $(echo "$C > 8" | bc -l) )); then echo "ABORTA: carga $C, esto es tiempo"; exit 3; fi

IN=/tmp/compresor-f1/corpus
W=$(mktemp -d /tmp/c0.XXXXXX)
trap 'rm -rf "$W"' EXIT
echo "commit $(git rev-parse --short HEAD) · arbol limpio · carga $C"
echo

# ── CONTROL A: el archivado completo, que es la operacion representativa ─────────────────────
echo "A · comprimir el arbol entero, tres veces, sin cambiar nada:"
for v in 1 2 3; do
  rm -f "$W/a.vc"
  bench/mide.sh -e "CERO archivar el arbol (vuelta $v)" -m arbol2.7g -n "control de cero, arbol limpio" -- \
    ./target/release/vcompresor compress "$IN" -o "$W/a.vc" --level 9 >/dev/null 2>&1
  echo "   vuelta $v: $(stat -c%s "$W/a.vc") B"
done

# ── CONTROL B: el respaldo diario, que es lo que mas se va a ejecutar ────────────────────────
echo
echo "B · la 2a instantanea de un arbol que no ha cambiado, tres veces:"
cp "$W/a.vc" "$W/base.vc"
for v in 1 2 3; do
  cp "$W/base.vc" "$W/b.vc"
  bench/mide.sh -e "CERO respaldo diario (vuelta $v)" -m arbol2.7g -n "control de cero, arbol limpio" -- \
    ./target/release/vcompresor add "$W/b.vc" "$IN" --name dia2 >/dev/null 2>&1
  echo "   vuelta $v: $(stat -c%s "$W/b.vc") B"
done

echo
echo "Los bytes de las tres vueltas de cada control tienen que ser IDENTICOS. Si no lo son,"
echo "no es el mismo trabajo y lo que se mide no es ruido, es otra cosa."
echo "carga al terminar: $(awk '{print $1}' /proc/loadavg)"
echo FIN-CERO
