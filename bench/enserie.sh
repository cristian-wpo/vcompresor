#!/usr/bin/env bash
# EN SERIE. El mismo candado que bench/mide.sh, para lo que NO es una medida del banco.
#
# Por que hace falta las dos cosas. bench/mide.sh serializa lo que entra en historia.jsonl, pero
# un BARRIDO exploratorio —doce configuraciones para ver por donde cae la curva— no debe dejar
# doce filas en el libro mayor: ensucia la historia con ruido de busqueda. Antes eso significaba
# «guion suelto en /tmp», y un guion suelto no coge ningun candado. Ahi se colaron las tres
# compresiones simultaneas del 01-08.
#
# Asi que la regla queda en dos, sin hueco entre ellas:
#   - una cifra que vaya a citarse  ->  bench/mide.sh   (candado + fila en el banco)
#   - un barrido para orientarse    ->  bench/enserie.sh (candado, sin fila)
# Lo que NO existe es una tercera via: compresion sin candado.
#
#   bench/enserie.sh bash /tmp/barrido.sh
#   bench/enserie.sh ./target/release/vcompresor compress /tmp/x -o /tmp/y.vc --max
set -uo pipefail
cd "$(dirname "$0")/.."

[ $# -gt 0 ] || { echo "uso: bench/enserie.sh <comando...>" >&2; exit 2; }

if [ -z "${VC_MIDE_EN_SERIE:-}" ]; then
  export VC_MIDE_EN_SERIE=1
  exec flock --verbose bench/.medida.lock "$0" "$@"
fi

exec "$@"
