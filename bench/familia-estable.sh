#!/usr/bin/env bash
# ¿ES ESTABLE LA FAMILIA POR FORMATO? (¿SE PUEDE CACHEAR?)
#
# `elige_familia()` decide con que familia de modelos se comprime probando CUATRO compresiones de
# una sonda de 128 KiB y quedandose con la que menos ocupa. Eso es trabajo real en cada frame. Si
# la familia ganadora fuera SIEMPRE la misma para un formato dado, la sonda sobra: se cachea por
# tipo y se ahorran cuatro compresiones de sonda por frame.
#
# LA PREGUNTA SE PARTE EN DOS, y la segunda es la que de verdad importa:
#   (a) ¿gana siempre la MISMA familia dentro de un formato?      -> ¿se puede cachear?
#   (b) cuando la sonda se equivoca, ¿CUANTO cuesta?              -> ¿importa equivocarse?
# Una sonda que acierta el 70% pero cuyo error vale el 0,02% es una sonda que sobra igual.
#
# ⚠️ Hacen falta VARIAS muestras DISJUNTAS por tipo. Con una sola no se contesta «estable»: se
# contesta «esta». Se cortan de trozos distintos del corpus.
#
# Tipo BYTES: determinista.
set -uo pipefail
cd "$(dirname "$0")/.."
. bench/comun.sh

MUESTRA=${MUESTRA:-262144}
CORPUS=${CORPUS:-/tmp/corpus-hilos}
# ⚠️ LOS DIEZ TIPOS, NO TRES. La primera ejecucion dio familia 3 en los NUEVE cortes de codigo,
# csvjson y sql — y esa es justo la respuesta que NO se puede generalizar desde tres tipos de
# texto. Si tambien sale 3 en jpg, png y video, la sonda —cuatro compresiones de 128 KiB POR
# FRAME— se puede cambiar por una constante.
TIPOS=${TIPOS:-"codigo csvjson sql logs bin pdfoffice png jpg video dup"}
CORTES=${CORTES:-3}
VC=./target/release/vcompresor
TMP=$(mktemp -d /tmp/familia.XXXXXX)
trap 'rm -rf "$TMP"' EXIT

cab "FAMILIA-ESTABLE · ¿gana siempre la misma familia? ¿se puede cachear la sonda?"
[ -x "$VC" ] || { echo "  falta $VC (cargo build --release)"; exit 2; }

comprime() {   # $1 = VC_FAM o 'auto'  $2 = entrada
  # ⚠️ `e` LOCAL Y REINICIADA. Sin el `local`, la llamada con 'auto' se quedaba el VC_FAM de la
  # llamada anterior y medía una familia FORZADA creyendo que medía la sonda — la prueba habría
  # contestado «la sonda acierta siempre», que es justo lo que viene a comprobar.
  local o="$TMP/o.vc" e=""
  rm -f "$o"
  [ "$1" != "auto" ] && e="VC_FAM=$1"
  # shellcheck disable=SC2086
  env $e "$VC" compress "$2" -o "$o" --level 9 --max >/dev/null 2>&1
  talla "$o"
}

rc=0
for t in $TIPOS; do
  [ -d "$CORPUS/$t" ] || { echo "  falta $CORPUS/$t — se salta"; rc=1; continue; }
  # varios cortes DISJUNTOS: uno de cada K empezando en desplazamientos distintos
  find "$CORPUS/$t" -type f 2>/dev/null | sort > "$TMP/lista.$t"
  n=$(wc -l < "$TMP/lista.$t")
  [ "$n" -ge "$CORTES" ] || { echo "  $t: solo $n ficheros, no llegan para $CORTES cortes"; rc=1; continue; }
  echo "  $t"
  printf "    %-8s %9s %9s %9s %9s  %-6s %-6s %s\n" \
         "corte" "fam-0" "fam-1" "fam-2" "fam-3" "mejor" "sonda" "coste del fallo"
  ganadoras=""
  for c in $(seq 1 "$CORTES"); do
    awk -v k="$CORTES" -v r="$c" 'NR%k==r-1' "$TMP/lista.$t" | xargs cat 2>/dev/null \
      | head -c "$MUESTRA" > "$TMP/m.bin"
    [ "$(talla "$TMP/m.bin")" -gt 4096 ] || { echo "    corte $c vacio"; rc=1; continue; }
    mejor=-1; mejorb=0; linea=""
    for f in 0 1 2 3; do
      b=$(comprime "$f" "$TMP/m.bin")
      eval "B$f=\$b"
      linea="$linea $(printf '%9s' "$b")"
      if [ "$mejor" -lt 0 ] || { [ "${b:-0}" -gt 0 ] && [ "$b" -lt "$mejorb" ]; }; then
        mejor=$f; mejorb=$b
      fi
    done
    a=$(comprime auto "$TMP/m.bin")
    # Que familia eligio la sonda: la que da EXACTAMENTE esos bytes. Forzarla recorre el mismo
    # camino, asi que se compara contra lo YA medido arriba — no se vuelve a comprimir.
    sonda="?"
    for f in 0 1 2 3; do
      eval "bf=\$B$f"
      [ "$bf" = "$a" ] && { sonda=$f; break; }
    done
    if [ "$mejorb" -gt 0 ]; then
      coste=$(awk -v a="$a" -v m="$mejorb" 'BEGIN{printf "%+.3f%%", 100*(a-m)/m}')
    else coste="?"; rc=1; fi
    printf "    %-8s%s  %-6s %-6s %s\n" "$c" "$linea" "$mejor" "$sonda" "$coste"
    ganadoras="$ganadoras $mejor"
  done
  u=$(echo $ganadoras | tr ' ' '\n' | sort -u | tr '\n' ' ')
  echo "    familias ganadoras en $t: $u  ->  $([ "$(echo $u | wc -w)" -eq 1 ] && echo 'ESTABLE (cacheable)' || echo 'NO estable')"
  echo
done

echo "  LEER ASI: si «mejor» es la misma en los tres cortes de un tipo, la sonda se puede cachear"
echo "  por formato. Y si la columna «coste del fallo» es del orden de centesimas, se puede quitar"
echo "  aunque NO sea estable: acertar no vale lo que cuesta preguntarlo."

# ── LO QUE FALTABA: ¿CUANTO CUESTA PREGUNTAR? ─────────────────────────────────────────────────
#
# Toda la prueba de arriba mide una mitad —lo que cuesta EQUIVOCARSE— y ninguna mide la otra, que
# es lo que cuesta PREGUNTAR. Sin las dos no hay decision: una sonda que se equivoca por 0,005%
# sobra si cuesta un 20% de reloj, y se queda si cuesta un 0,1%.
#
# Y el coste NO es una constante: la sonda son cuatro compresiones de hasta 128 KiB POR TRAMO, o
# sea que su peso relativo depende del tamaño del tramo. Por eso se mide a varios tamaños en vez
# de a uno y se extrapola.
#
# ⚠️ EN SERIE. Esto cronometra: RELOJ=1 y nada mas corriendo en la maquina.
if [ "${RELOJ:-0}" = "1" ]; then
  echo
  echo "  ═══ ¿CUANTO CUESTA PREGUNTAR? (RELOJ, en serie) ═══"
  echo
  printf "    %-10s %10s %10s %10s %10s %9s %9s\n" \
         "tipo" "bloque" "s sonda" "s fija(3)" "reloj" "B sonda" "B fija"
  # ⚠️ SIN COMILLAS DENTRO DEL `${...:-...}`. Con ellas el valor por defecto se expande como UNA
  # sola palabra y el bucle da una vuelta con «262144 1048576 8388608» entero metido en $m: la
  # primera version imprimio tres veces «la muestra salio vacia» y ni una fila.
  for t in ${TIPOS_RELOJ:-codigo sql bin}; do
    [ -d "$CORPUS/$t" ] || continue
    for m in ${BLOQUES:-262144 1048576 8388608}; do
      muestra_repartida "$CORPUS/$t" "$m" "$TMP/r.bin" || continue
      real=$(talla "$TMP/r.bin")
      [ "$real" -lt $((m * 9 / 10)) ] && break
      t0=$(date +%s.%N); bs=$(comprime auto "$TMP/r.bin"); t1=$(date +%s.%N)
      bf=$(comprime 3 "$TMP/r.bin"); t2=$(date +%s.%N)
      ss=$(awk -v a="$t0" -v b="$t1" 'BEGIN{printf "%.2f", b-a}')
      sf=$(awk -v a="$t1" -v b="$t2" 'BEGIN{printf "%.2f", b-a}')
      dr=$(awk -v a="$ss" -v b="$sf" 'BEGIN{if(b>0) printf "%+.1f%%", 100*(a-b)/b; else print "?"}')
      printf "    %-10s %10s %10s %10s %10s %9s %9s\n" "$t" "$real" "$ss" "$sf" "$dr" "$bs" "$bf"
    done
  done
  echo
  echo "    «reloj» es lo que la SONDA añade sobre fijar la familia. Positivo = preguntar cuesta."
  echo "    ⚠️ Si «B sonda» y «B fija» coinciden, en ese bloque la sonda ya elegia la 3: entonces"
  echo "       la fila mide el coste de preguntar SIN comprar nada, que es el caso limpio."
fi
echo "FIN familia-estable rc=$rc"
exit $rc
