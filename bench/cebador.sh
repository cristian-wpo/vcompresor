#!/usr/bin/env bash
# ¿CUANTO COMPRA ARRANCAR EL MODELO EN CALIENTE?
#
# Un codec de contexto empieza cada frame sin saber nada: las tablas vacias, los pesos del
# mezclador en su valor inicial, el StateMap sin observaciones. Los primeros cientos de KB se
# pagan a precio de aprendizaje. Un CEBADOR —arrancar con el modelo ya entrenado sobre datos del
# mismo tipo— quitaria ese peaje. Antes de escribirlo hay que saber cuanto vale.
#
# COMO SE MIDE SIN TENER LA FUNCION. No hace falta implementar el cebador para medir lo que
# compraria: basta con darle al codec el prefijo DENTRO del mismo frame y restar.
#
#     a = |comprimir(S)|              S solo, en frio
#     c = |comprimir(P)|              el prefijo solo
#     b = |comprimir(P + S)|          S detras de P, o sea CALIENTE
#     coste de S en caliente = b - c
#
# El cebador de verdad seria aun mejor que esto (no tendria que ESCRIBIR P), asi que (b-c) frente
# a (a) es una COTA INFERIOR de lo que compraria. Si esa cota ya sale pequeña, no hay cebador que
# escribir y la pregunta esta contestada por menos de lo que cuesta implementarlo.
#
# ⚠️ P+S tiene que caber en UN frame o esto no mide lo que dice. Con --max el frame son 32 MiB;
# aqui P y S son 1 MB cada uno.
#
# Tipo BYTES: determinista.
set -uo pipefail
cd "$(dirname "$0")/.."
. bench/comun.sh

TROZO=${TROZO:-1048576}
CORPUS=${CORPUS:-/tmp/corpus-hilos}
TIPOS=${TIPOS:-"codigo csvjson sql"}
VC=./target/release/vcompresor
TMP=$(mktemp -d /tmp/cebador.XXXXXX)
trap 'rm -rf "$TMP"' EXIT

cab "CEBADOR · ¿cuanto compra arrancar el modelo en caliente?"
[ -x "$VC" ] || { echo "  falta $VC (cargo build --release)"; exit 2; }

comprime() { local o="$TMP/o.vc"; rm -f "$o"
  "$VC" compress "$1" -o "$o" --level 9 --max >/dev/null 2>&1; talla "$o"; }

rc=0
printf "  %-10s %10s %10s %10s %10s %9s\n" "tipo" "S frio" "P" "P+S" "S caliente" "compra"
for t in $TIPOS; do
  [ -d "$CORPUS/$t" ] || { echo "  falta $CORPUS/$t — se salta"; rc=1; continue; }
  # ⚠️⚠️ DOS INTENTOS FALLIDOS ANTES DE ESTE, Y EL SEGUNDO ERA PEOR QUE EL PRIMERO.
  #
  #   1º  mitades alternas de la LISTA de ficheros. Disjuntos de verdad, pero en sql —cuatro
  #       ficheros, uno enorme— una mitad no llegaba al tamaño y el tipo se saltaba entero.
  #   2º  dos tramos SEGUIDOS del corpus concatenado. Arreglaba la cobertura y ROMPIA LA MEDIDA:
  #       S pasa a ser la CONTINUACION de P, o sea el mismo volcado de tabla siguiendo, y el
  #       match model engancha de un lado al otro. Salieron −37,95% en codigo y −46,93% en sql,
  #       que no es «lo que compra arrancar en caliente» sino «lo que compra que P y S sean el
  #       mismo fichero». Un numero espectacular midiendo otra cosa.
  #
  # Lo que hace falta es que P y S sean del mismo TIPO y de ficheros DISTINTOS, y que las dos
  # mitades pesen parecido. Se reparte por bytes ACUMULADOS: los ficheros van cayendo en la
  # mitad que menos lleva, asi que ningun fichero gigante desequilibra el reparto.
  # ⚠️ Y EL REPARTO EN awk USABA `ENVIRON["TMP"]` PARA UNA VARIABLE DEL SHELL SIN EXPORTAR, o
  # sea vacia: escribia en «/la» y moria con «Permission denied». Se pasa con -v, que es la
  # forma de meterle una variable a awk. (Fallo en voz alta y se salto los tres tipos en vez de
  # dar un numero: es el modo correcto de fallar, pero era un fallo.)
  # El tamaño sale de un solo `find -printf` en vez de un `wc -c` por fichero.
  find "$CORPUS/$t" -type f -printf '%s\t%p\n' 2>/dev/null | sort -k2 > "$TMP/ls"
  : > "$TMP/la"; : > "$TMP/lb"
  awk -v dir="$TMP" -F'\t' '{ if (a <= b) { print $2 > (dir "/la"); a += $1 }
                               else        { print $2 > (dir "/lb"); b += $1 } }' "$TMP/ls"
  : > "$TMP/P"; : > "$TMP/S"
  xargs cat < "$TMP/la" 2>/dev/null | head -c "$TROZO" > "$TMP/P"
  xargs cat < "$TMP/lb" 2>/dev/null | head -c "$TROZO" > "$TMP/S"
  if [ "$(talla "$TMP/P")" -lt "$TROZO" ] || [ "$(talla "$TMP/S")" -lt "$TROZO" ]; then
    echo "  $t: no hay $TROZO bytes en cada mitad de ficheros — se salta"; rc=1; continue
  fi
  cat "$TMP/P" "$TMP/S" > "$TMP/PS"

  a=$(comprime "$TMP/S")
  c=$(comprime "$TMP/P")
  b=$(comprime "$TMP/PS")
  cal=$(( b - c ))
  if [ "${a:-0}" -gt 0 ]; then
    pct=$(awk -v x="$cal" -v y="$a" 'BEGIN{printf "%+.2f%%", 100*(x-y)/y}')
  else pct="?"; rc=1; fi
  printf "  %-10s %10s %10s %10s %10s %9s\n" "$t" "$a" "$c" "$b" "$cal" "$pct"

  # Si S caliente pesa lo mismo que S frio, o el frame se partio o P y S no se parecen en nada.
  distingue_o_aborta "$a" "$cal" "S en frio y S detras de 1 MB del mismo tipo" || rc=1
done

echo
echo "  LEER ASI: «compra» es cuanto MENOS ocupa S cuando el modelo llega entrenado. Es una COTA"
echo "  INFERIOR —el cebador de verdad no tendria que escribir P— asi que si aqui sale poco, sale"
echo "  poco seguro. Y ojo al reves: si sale mucho, lo que dice es que el peaje de arranque es"
echo "  grande, no que el cebador sea facil (el diccionario tendria que viajar o ser conocido)."
echo "FIN cebador rc=$rc"
exit $rc
