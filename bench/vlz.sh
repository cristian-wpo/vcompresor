#!/usr/bin/env bash
# EL CARRIL DE vlz EN EL BANCO. Para que sus numeros dejen de vivir solo en los mensajes de commit.
#
# POR QUE EXISTE. El 01-08 el dueño pregunto «¿no me sirve el panel para ver que se mueve de lo
# implementado?». La causa estaba medida: 13 de los ultimos 40 commits tienen medidas en
# bench/historia.jsonl y 27 no — y TODOS los de vlz estaban entre los 27. No era que faltara una
# bascula en el Mac: era que vlz-banco imprimia por pantalla y nunca pasaba por bench/mide.sh.
#
# Esto lo arregla por donde habia que arreglarlo: cada punto (tipo x esfuerzo) es UNA linea del
# banco, con su commit, su carga, sus ciclos y su RSS, igual que el resto del producto.
#
#   bench/vlz.sh [corpus] [esfuerzos...]      por defecto: /tmp/corpus-hilos, esfuerzos 1 6 64
#
# ⚠️ EN SERIE Y A PROPOSITO: son medidas de tiempo. Paralelizarlas las falsea.
# ⚠️ LA PRIMERA RONDA NO SE DESCARTA AQUI porque cada punto es una tanda distinta y el banco
#    guarda la carga: lo que se compara en el panel es la MEDIANA de las medidas de un mismo
#    (etiqueta, muestra, config) a lo largo del tiempo, no una contra otra dentro de esta tanda.
#    Para un A/B de dos commits el instrumento es bench/cero-dos-puntas.sh, no este.
set -uo pipefail
cd "$(dirname "$0")/.."

CORPUS=${1:-/tmp/corpus-hilos}
shift 2>/dev/null || true
ESFUERZOS=${*:-1 6 64}
# ⚠️ GUARDIA DE ARBOL LIMPIO. Paso DOS VECES el 01-08 y por lo mismo: editar un fichero del repo
# MIENTRAS el carril corria. mide.sh marca esas medidas `sucio: true` -y hace bien, una cifra de
# un arbol sin commitear no es reproducible- pero para cuando se ve ya se han tirado 20 minutos de
# reloj. La segunda vez fueron 23 de 33 medidas a la basura por tocar bench/cdc-tallas.sh a mitad.
# Se comprueba ANTES de empezar, con las mismas exclusiones que usa mide.sh.
SUCIO=$(git status --porcelain -- .           ':!bench/historia.jsonl' ':!bench/datos.json' ':!bench/panel.html' ':!bench/registros' ':!bench/registros-m3'         2>/dev/null | head -3)
if [ -n "$SUCIO" ]; then
  echo "ABORTA: el arbol esta sucio y las medidas saldrian marcadas como no reproducibles." >&2
  echo "$SUCIO" >&2
  echo "Commitea primero. Y NO TOQUES el repo mientras esto corre: la guardia solo mira al arrancar." >&2
  exit 3
fi

BIN=./target/release/vlz-banco
[ -x "$BIN" ] || { echo "falta $BIN — construyelo: cargo build --release -p vlz --bin vlz-banco" >&2; exit 2; }
[ -d "$CORPUS" ] || { echo "no existe $CORPUS" >&2; exit 2; }

# Los tipos que hay dentro del corpus. Cada uno se mide APARTE: el numero global esconde que la
# longitud con escape gana -4,6% en logs y +1,0% en sql, y esa es justo la informacion que sirve.
TIPOS=$(find "$CORPUS" -mindepth 1 -maxdepth 1 -type d -printf '%f\n' | sort)
[ -n "$TIPOS" ] || TIPOS="."

echo
echo "  CARRIL vlz · corpus $CORPUS · esfuerzos: $ESFUERZOS"
echo "  ⚠️ vlz-banco CONCATENA los ficheros de cada tipo y los ordena por ruta. El montaje cambia"
echo "     los numeros a la mitad, asi que viaja en la etiqueta y en la nota."
echo

for t in $TIPOS; do
  D="$CORPUS/$t"
  [ -d "$D" ] || continue
  N=$(find "$D" -type f | wc -l)
  [ "$N" -gt 0 ] || continue
  for e in $ESFUERZOS; do
    O=$(mktemp /tmp/vlz.XXXXXX.blob)
    bench/mide.sh -H vlz \
      -e "vlz $t esfuerzo $e" \
      -m "$t-concatenado" \
      -c "esfuerzo=$e" \
      -n "vlz-banco CONCATENA los $N ficheros de $t en orden de ruta y comprime el bloque entero: es lo que hace el archivador -hasta 64 MiB por frame- y no lo que se mediria fichero a fichero, que daria el DOBLE de bytes. La ida y vuelta se verifica byte a byte ANTES de escribir nada; si falla, vlz-banco aborta sin imprimir ningun tamaño." \
      -- "$BIN" "$D" "$e" -o "$O"
    rm -f "$O"
  done
done
echo
echo "  FIN-VLZ · las medidas estan en bench/historia.jsonl. Regenera el panel: python3 bench/panel.py"
