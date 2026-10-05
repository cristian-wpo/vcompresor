#!/usr/bin/env bash
# ¿EL LIMITE ES EL FRAME? La prueba que dejo puesta la pregunta P11.
#
# DE DONDE VIENE. El barrido de huecos encontro que en un arbol de ficheros de verdad se puede
# comprimir un 5,89% MAS gastando 8,8 veces MENOS —estrategia barata con el buscador de largo
# alcance de zstd encendido— y que la ganancia viene de encontrar repeticiones ENTRE FICHEROS
# LEJANOS. Pero eso se midio sobre un bloque de 128 MB, y el archivador no le da a zstd bloques:
# le da FRAMES de 8 MB (texto), 2 MB (media) y 4 MB (otros). La ventana de zstd -19 ya es de 8 MB,
# o sea que el producto NUNCA le da mas de un frame y toda esa ganancia es hoy inalcanzable.
#
# El limite no es la ventana. Es el frame. Y hay una perilla que ya existe para probarlo:
# `--solid N` fija el frame en N MiB para todos los carriles.
#
# LO QUE ESTA PRUEBA PUEDE Y NO PUEDE CONTESTAR. Puede decir cuanto compra un frame mas grande
# con el zstd que tenemos. NO puede probar el LDM, porque el producto no lo expone todavia: si el
# frame grande ya compra por si solo, eso justifica exponerlo; si no compra nada, el hueco del
# LDM se queda donde esta y no hay que tocar el codigo.
#
# EN SERIE y con el arbol limpio, como toda medida de tiempo de esta casa. Y con la ida y vuelta
# comprobada: un frame gigante que produce un archivo roto no es una mejora.
set -uo pipefail
cd "$(dirname "$0")/.."

IN=/tmp/compresor-f1/corpus
VC=$PWD/target/release/vcompresor
NIVEL=${NIVEL:-9}

SUCIO=$(git status --porcelain -- . ':!bench/historia.jsonl' ':!bench/datos.json' ':!bench/panel.html' ':!bench/registros' ':!bench/registros-m3' | wc -l)
[ "$SUCIO" -eq 0 ] || { echo "ABORTA: $SUCIO ficheros de codigo sin commitear"; exit 3; }
C=$(awk '{print $1}' /proc/loadavg)
if (( $(echo "$C > 8" | bc -l) )); then echo "ABORTA: carga $C, esto es tiempo"; exit 3; fi

W=$(mktemp -d /tmp/fr.XXXXXX); trap 'rm -rf "$W"' EXIT
echo "commit $(git rev-parse --short HEAD) · carga $C · nivel $NIVEL"
echo "arbol: $(find "$IN" -type f | wc -l) ficheros"
echo
BASE=""
# 0 = «como esta hoy»: cada carril con su tamaño (8/2/4 MiB). Los demas fijan el frame.
# 0 = como esta hoy. Los demas son MiB, y --solid los limita a 32..256: por debajo de 32 el
# propio CLI dice que no merece la pena, asi que no hay 8 ni 16 que probar.
for S in 0 32 64 128 256; do
  ETI=$( [ "$S" = 0 ] && echo "el defecto de hoy (64)" || echo "solid ${S}MiB" )
  rm -f "$W/o.vc"
  if [ "$S" = 0 ]; then
    bench/mide.sh -H archivador -e "F14 frame: $ETI" -m arbol2.7g -c "$ETI" \
      -n "¿el limite es el frame? (P11, tras el hueco del LDM)" \
      -- "$VC" compress "$IN" -o "$W/o.vc" --level "$NIVEL" >/dev/null 2>&1
  else
    bench/mide.sh -H archivador -e "F14 frame: $ETI" -m arbol2.7g -c "$ETI" \
      -n "¿el limite es el frame? (P11, tras el hueco del LDM)" \
      -- "$VC" compress "$IN" -o "$W/o.vc" --level "$NIVEL" "--solid=$S" >/dev/null 2>&1
  fi
  B=$(stat -c%s "$W/o.vc" 2>/dev/null || echo 0)
  if [ "$B" = 0 ]; then echo "  $ETI: NO PRODUJO ARCHIVO — se para"; exit 4; fi
  if ! "$VC" verify "$W/o.vc" >/dev/null 2>&1; then
    echo "  $ETI: IDA Y VUELTA ROTA — un frame que rompe el archivo no es una mejora"; exit 5
  fi
  if [ -z "$BASE" ]; then BASE=$B; D="—"
  else D=$(python3 -c "print(f'{100*($B-$BASE)/$BASE:+.3f}%')"); fi
  echo "  $(printf %-22s "$ETI") $B B  $D   carga $(awk '{print $1}' /proc/loadavg)"
done
echo
echo "Un frame mas grande compra bytes y cuesta PARALELISMO: menos frames son menos hilos a la"
echo "vez. Por eso hay que mirar el reloj al lado del tamaño, y los dos estan en historia.jsonl."
echo FIN-FRAMES
