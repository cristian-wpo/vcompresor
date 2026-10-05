#!/usr/bin/env bash
# ¿DESDE QUE TAMAÑO DE BLOQUE CONVIENE PODAR LOS `DUPLICADOS`?
#
# LO QUE PASA HOY. `apagados_de_perfil(0)` —el perfil por defecto— apaga los seis modelos que la
# cadena ISSE ya refina, SIEMPRE, sin mirar el tamaño del bloque. Su hermana `PODA_V2` si mira
# (`PODA_DESDE` = 8 MiB, crates/vc-cm/src/lib.rs:460) porque se midio que por debajo del cruce los
# podados «aun aciertan». A los DUPLICADOS nadie les hizo esa pregunta.
#
# Y la respuesta parcial ya estaba en el banco, en `racimos.log`: sobre 512.000 B la poda por
# defecto cuesta **+1,10% en codigo** y **+0,76% en csvjson**. O sea que la justificacion escrita
# al lado del codigo —«mejor ratio (-0,15% en el total) y -10% de tiempo»— es cierta al tamaño en
# el que se midio y FALSA por debajo. Esto no es afinar una perilla: es un umbral que falta.
#
# ⚠️ LO QUE ESTA PRUEBA **NO** CONTESTA, y hay que decirlo antes de leerla: la poda tambien compra
# RELOJ y RAM. Con `RELOJ=1` se hace una segunda pasada EN SERIE que cronometra; sin eso, aqui
# solo hay bytes. Publicar un umbral con la mitad de la cuenta seria elegir por el eje que se
# midio, no por el que decide.
#
# EL CONTROL, dicho antes de mirar el resultado: la curva tiene que ser MONOTONA y cruzar UNA vez
# —caro en pequeño, gratis o mejor en grande—. Si zigzaguea, no hay umbral que sacar: significa
# que la muestra es demasiado pequeña para distinguir, y entonces la respuesta es «mas muestra»,
# no un numero. Es la misma trampa que ya mordio tres veces en este banco.
#
#   uso:  bash bench/duplicados-umbral.sh            (bytes, se puede paralelizar)
#         RELOJ=1 bash bench/duplicados-umbral.sh    (⚠️ EN SERIE: ademas cronometra)
set -uo pipefail
cd "$(dirname "$0")/.." || exit 1
. bench/comun.sh

CORPUS=${CORPUS:-/tmp/corpus-hilos}
TIPOS=${TIPOS:-"codigo sql logs csvjson bin"}
# Hasta 8 MiB, que es donde `PODA_DESDE` ya decide: por encima las dos podas coinciden y la
# pregunta se acaba.
TALLAS=${TALLAS:-"131072 262144 524288 1048576 2097152 4194304 8388608"}
VCM=./target/release/vc-cm
RELOJ=${RELOJ:-0}
rc=0

[ -x "$VCM" ] || { echo "no esta $VCM (compila: cargo build --release)"; exit 3; }
TMP=$(mktemp -d /tmp/dupumbral.XXXXXX)
trap 'rm -rf "$TMP"' EXIT

cab "DUPLICADOS · ¿desde que tamaño conviene podarlos?"
echo "  poda ON  = perfil 0 (el defecto de hoy): los seis DUPLICADOS fuera"
echo "  poda OFF = VC_PERFIL=1: los 36 modelos, como la v1"
[ "$RELOJ" = "1" ] && echo "  ⚠️ RELOJ=1 · esta corrida cronometra: NO lanzar nada mas en la maquina"
echo

for t in $TIPOS; do
  [ -d "$CORPUS/$t" ] || { echo "  falta $CORPUS/$t — se salta"; rc=1; continue; }
  echo "  $t"
  printf "    %10s %12s %12s %9s" "muestra" "poda ON" "poda OFF" "cuesta"
  [ "$RELOJ" = "1" ] && printf " %9s %9s %8s" "s ON" "s OFF" "reloj"
  echo
  signo_previo=""
  cruces=0
  for m in $TALLAS; do
    muestra_repartida "$CORPUS/$t" "$m" "$TMP/m.bin" || { rc=1; continue; }
    real=$(talla "$TMP/m.bin")
    # ⚠️ Si la muestra sale mas pequeña que la pedida, el corpus se acabo: esa fila y las
    # siguientes son LA MISMA muestra con otro nombre, y compararlas dibujaria una curva falsa.
    if [ "$real" -lt $((m * 9 / 10)) ]; then
      printf "    %10s  (el corpus solo da %s B: se corta aqui)\n" "$m" "$real"
      break
    fi
    t0=$(date +%s.%N)
    on=$(VC_PERFIL=0 "$VCM" c "$TMP/m.bin" "$TMP/o.cm" 2>&1 | sed -n 's/.*out=\([0-9]*\).*/\1/p')
    t1=$(date +%s.%N)
    off=$(VC_PERFIL=1 "$VCM" c "$TMP/m.bin" "$TMP/o.cm" 2>&1 | sed -n 's/.*out=\([0-9]*\).*/\1/p')
    t2=$(date +%s.%N)
    [ -n "$on" ] && [ -n "$off" ] || { echo "    $m: vc-cm no devolvio bytes"; rc=1; continue; }
    d=$(awk -v a="$on" -v b="$off" 'BEGIN{printf "%+.3f", 100*(a-b)/b}')
    printf "    %10s %12s %12s %8s%%" "$real" "$on" "$off" "$d"
    if [ "$RELOJ" = "1" ]; then
      son=$(awk -v a="$t0" -v b="$t1" 'BEGIN{printf "%.2f", b-a}')
      sof=$(awk -v a="$t1" -v b="$t2" 'BEGIN{printf "%.2f", b-a}')
      dr=$(awk -v a="$son" -v b="$sof" 'BEGIN{if(b>0) printf "%+.1f%%", 100*(a-b)/b; else printf "?"}')
      printf " %9s %9s %8s" "$son" "$sof" "$dr"
    fi
    echo
    # El cruce: cuantas veces cambia el signo de «cuesta». Uno es un umbral; mas de uno, ruido.
    s=$(awk -v x="$d" 'BEGIN{print (x+0>0)?"+":"-"}')
    [ -n "$signo_previo" ] && [ "$s" != "$signo_previo" ] && cruces=$((cruces + 1))
    signo_previo=$s
  done
  case "$cruces" in
    0) echo "    -> NO cruza en este rango: la poda cae del mismo lado en todos los tamaños." ;;
    1) echo "    -> cruza UNA vez: ahi esta el umbral de este tipo." ;;
    *) echo "    -> ⚠️ cruza $cruces veces: eso NO es un umbral, es ruido. Mas muestra antes de decidir."; rc=1 ;;
  esac
  echo
done

echo "  LEER ASI: «cuesta» es lo que la poda por defecto AÑADE frente a no podar. Positivo ="
echo "  la poda hace el archivo mas grande. El umbral es el primer tamaño donde deja de costar."
echo "  ⚠️ Y no se decide solo con esta columna: por debajo del umbral la poda sigue comprando"
echo "  reloj y RAM, asi que ahi no es «un fallo que quitar» sino una opcion del menu. Lo que si"
echo "  es un fallo es que hoy se aplique SIEMPRE con la excusa de que mejora el ratio."
echo "FIN duplicados-umbral rc=$rc"
exit $rc
