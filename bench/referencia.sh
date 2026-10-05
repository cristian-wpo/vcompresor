#!/usr/bin/env bash
# EL BANCO DE REFERENCIA: lo MISMO, con los MISMOS defectos, en cada cierre.
#
# 🔑 POR QUE EXISTE. La linea de tiempo del panel no dibuja hallazgos: dibuja el MISMO experimento
# -misma etiqueta, misma muestra, misma config- medido en DOS commits. Un dia entero de ablaciones
# A/B no le da ni un punto, porque una ablacion son dos configuraciones EN EL MISMO commit.
# Sin una referencia fija que se repita, la curva se queda plana por diseño y parece averiada.
#
# Dos filas y nada mas, que son las dos herramientas que se pagan:
#   archivador . el arbol real de 2,62 GB con los defectos del producto
#   vdeflate   . el banco de 2.522 flujos con los defectos
#
# EN SERIE y con la maquina para ella sola: mide reloj.
set -uo pipefail
cd "$(dirname "$0")/.." || exit 1
W=$(mktemp -d /tmp/ref.XXXXXX)
trap 'rm -rf $W' EXIT
# 🚨 EL CORPUS, ANTES DE MEDIR NADA. Esta referencia apuntaba a /tmp y sus dos corpus grandes
# se perdieron con un reinicio: el guion seguia adelante y dejaba filas a medias en el libro
# mayor en vez de decir que no podia correr. Una referencia que mide lo que quede no es una
# referencia. Con --sigue se fuerza (para medir solo lo que si esta), y se dice en pantalla.
if [ "${1:-}" != "--sigue" ]; then
  if ! bench/corpus.sh >/tmp/.ref-corpus.txt 2>&1; then
    cat /tmp/.ref-corpus.txt
    echo
    echo "REFERENCIA ABORTADA: el corpus no esta completo (arriba, en detalle)."
    echo "Rearmalo, o corre 'bench/referencia.sh --sigue' a sabiendas de que la fila fija faltara."
    exit 1
  fi
  grep -E 'PERDIDO|⚠️' /tmp/.ref-corpus.txt || true
fi


bench/mide.sh -H archivador -e 'REFERENCIA: el arbol real con los defectos' \
  -m arbol2.62g -c 'defectos del producto' \
  -n 'Banco de referencia. Se corre IGUAL en cada cierre para que la linea de tiempo tenga pares.' \
  -- ./target/release/vcompresor compress /tmp/compresor-f1/corpus -o $W/o.vc
bench/mide.sh -H vdeflate -e 'REFERENCIA: la fase 3 con los defectos' \
  -m deflate-banco2 -c 'defectos del producto' \
  -n 'Banco de referencia. Se corre IGUAL en cada cierre para que la linea de tiempo tenga pares.' \
  -- ./target/release/vc-fase3 /tmp/deflate-banco2

# ── LA TERCERA CONFIGURACION: --max, o sea el CM ──────────────────────────────────────────────
#
# 🔑 SIN ESTA FILA, MEDIA MEJORA DEL PRODUCTO NO SE VE. `set_cm(maximo)` (vc-core/src/lib.rs:2121):
# el codec de mezcla de contextos SOLO corre con --max. Todo lo que se toca ahi —la sonda de
# familias, los DUPLICADOS, el racimo de match, las perillas del mezclador— es invisible en las
# dos filas de arriba, que van con los defectos. Un banco de referencia que no cubre una
# configuracion entera no es una referencia: es media.
#
# ⚠️ Y VA SOBRE 64 MiB, NO SOBRE EL ARBOL. El CM comprime a ~0,1 MB/s medido: los 2,62 GB serian
# siete horas y una referencia que nadie corre no sirve de nada. 64 MiB son ~11 min, que es lo que
# cuesta poder responder "¿se movio el CM?" en cada cierre.
#
# ⚠️ La muestra se arma SIEMPRE IGUAL —uno de cada tres, repartido, cortado a 64 MiB— porque el
# panel empareja por (etiqueta, muestra, config) Y COMPRUEBA `bytes_in`: si la entrada cambia mas
# de un 2%, tira el par en vez de publicar una diferencia que no es del codigo.
M=$W/max64.bin
find /tmp/compresor-f1/corpus -type f | LC_ALL=C sort | awk 'NR%3==1' \
  | xargs cat 2>/dev/null | head -c 67108864 > "$M"
if [ -s "$M" ]; then
  bench/mide.sh -H archivador -e 'REFERENCIA: --max (el CM) con los defectos' \
    -m corpus64m -c 'defectos del producto, --max' \
    -n 'Banco de referencia de la tercera configuracion: el CM solo corre con --max, asi que sin esta fila la sonda de familias y los DUPLICADOS no se ven en ninguna medida.' \
    -- ./target/release/vcompresor compress "$M" -o $W/max.vc --max
else
  echo '  ⚠️ no se pudo armar la muestra de 64 MiB: la fila de --max NO se mide (y se dice)'
fi
echo FIN-REFERENCIA
