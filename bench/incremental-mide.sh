#!/usr/bin/env bash
# ¿CUANTO AHORRA DE VERDAD EL SALTO INCREMENTAL? A/B en serie sobre el arbol de 2,7 GB.
#
# LA COMPARACION ES CONTRA NOSOTROS MISMOS, no contra el commit de ayer: el mismo binario, el
# mismo arbol y la misma ventana de carga, cambiando UNA cosa —VC_SIN_INCREMENTAL— que es
# exactamente el codigo nuevo. Asi no hay que fiarse de que la maquina estuviera igual ayer.
#
# EN SERIE y ALTERNANDO. Y se comprueban los BYTES: si el archivo sale distinto con el salto y
# sin el, el ahorro no vale nada porque no es el mismo trabajo.
set -uo pipefail
cd "$(dirname "$0")/.."
IN=/tmp/compresor-f1/corpus
W=/tmp/incm; rm -rf "$W"; mkdir -p "$W"
M=bench/mide.sh

echo "carga al empezar: $(cat /proc/loadavg)"

# La instantanea de partida. No es la medida: es el terreno.
bench/mide.sh -e "D13 base para el incremental" -m arbol2.7g -- \
  ./target/release/vcompresor compress "$IN" -o "$W/base.vc" --level 9 >/dev/null 2>&1
echo "base: $(stat -c%s "$W/base.vc") B"

for v in 1 2; do
  for MODO in con sin; do
    cp "$W/base.vc" "$W/t.vc"
    if [ "$MODO" = sin ]; then export VC_SIN_INCREMENTAL=1; else unset VC_SIN_INCREMENTAL; fi
    bench/mide.sh -e "D13 2a instantanea: $MODO salto (vuelta $v)" -m arbol2.7g \
      -c "VC_SIN_INCREMENTAL=${VC_SIN_INCREMENTAL:-0}" -- \
      ./target/release/vcompresor add "$W/t.vc" "$IN" --name dia2 > "$W/$MODO.$v.txt" 2>&1
    echo "  vuelta $v · $MODO salto  ->  $(stat -c%s "$W/t.vc") B"
    grep -E "unchanged|time:" "$W/$MODO.$v.txt" | sed 's/^/      /'
    mv "$W/t.vc" "$W/r.$MODO.$v.vc"
  done
done
unset VC_SIN_INCREMENTAL

echo
echo "BYTES (tienen que ser IGUALES: el salto no puede cambiar el archivo, solo el reloj)"
for f in "$W"/r.*.vc; do echo "  $(stat -c%s "$f")  $(basename "$f")"; done

echo
echo "Y el archivo con salto tiene que poder extraerse igual que el otro:"
for MODO in con sin; do
  d="$W/x.$MODO"; mkdir -p "$d"
  ./target/release/vcompresor extract "$W/r.$MODO.1.vc" -o "$d" >/dev/null 2>&1 \
    && echo "  $MODO: extrae" || echo "  $MODO: NO EXTRAE"
done
diff -r "$W/x.con" "$W/x.sin" >/dev/null 2>&1 \
  && echo "  y los dos arboles extraidos son IDENTICOS" \
  || echo "  *** LOS ARBOLES EXTRAIDOS DIFIEREN — el salto esta mal ***"
rm -rf "$W/x.con" "$W/x.sin"
echo "carga al terminar: $(cat /proc/loadavg)"
echo FIN-INCMIDE
