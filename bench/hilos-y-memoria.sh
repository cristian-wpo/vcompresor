#!/usr/bin/env bash
# ¿ESCALA vc-cm CON LOS HILOS? — y la rejilla, no dos barridos sueltos.
#
# LAS DOS PREGUNTAS, que son la misma:
#   A) el tope de 10 hilos NUNCA SE HA MEDIDO. Lo dice el propio codigo: «es un numero puesto a
#      ojo». Y cada hilo carga SU PROPIO modelo -1,15 GB medidos-, asi que con diez son diez
#      juegos de tablas peleandose por los mismos 32 MiB de L3 (16 por bloque en este Ryzen).
#      Mas hilos pueden hacer que CADA UNO vaya mas lento.
#   B) bench/memoria.tsv barrio VC_MEM de 0 a -3 anotando bytes y RAM y SIN UNA COLUMNA DE
#      TIEMPO, cuando toda la premisa es «tablas mas pequeñas caben en cache y por eso van mas
#      rapido». Midio lo que cuesta y no lo que compra.
#
# POR QUE EN REJILLA Y NO DOS BARRIDOS. La hipotesis es «tablas mas pequeñas -> menos pelea por
# la L3 -> mejor escalado con hilos». Dos barridos sueltos NO PUEDEN VERLA: el de hilos se corre
# con las tablas de hoy y el de memoria con un hilo. Es exactamente el error que P15 acaba de
# enseñar con la ablacion de uno en uno — medir por separado dos cosas que interactuan y sumar.
#
# ⚠️ MIDE TIEMPO: en serie, en una sola maquina, arbol limpio y carga baja. Los bytes de la misma
# pasada valen igual de todas formas, pero el reloj manda el protocolo.
set -uo pipefail
cd "$(dirname "$0")/.."
# 754 MB REPARTIDOS (uno de cada tres ficheros), ~21 tramos de 32 MiB.
# Se recorta el TAMAÑO, no el numero de tramos: con --max los tramos son de 32 MiB y con
# menos de diez no se puede medir el reparto en diez hilos. La entrada completa daba 45
# min POR PASADA a un hilo. La primera celda real tardo 8.704 s -DOS HORAS Y MEDIA-, asi que
# las quince eran ~15 horas de servidor con el cerrojo del reloj cogido todo el rato. Por eso
# ahora es una CRUZ de siete celdas y no una rejilla de quince: ver la cabecera de mas abajo.
# Y va REPARTIDA, no «los primeros N»: ese sesgo ya publico hoy un 97,14% falso.
IN=${IN:-/tmp/corpus-hilos}
VC=$PWD/target/release/vcompresor
W=$(mktemp -d /tmp/hm.XXXXXX); trap 'rm -rf "$W"' EXIT

SUCIO=$(git status --porcelain -- . ':!bench/historia.jsonl' ':!bench/datos.json' ':!bench/panel.html' ':!bench/registros' ':!bench/registros-m3' 2>/dev/null | wc -l)
[ "$SUCIO" -eq 0 ] || { echo "ABORTA: $SUCIO ficheros sin commitear, y esto mide tiempo"; exit 3; }
C=$(awk '{print $1}' /proc/loadavg)
if (( $(echo "$C > 8" | bc -l) )); then echo "ABORTA: carga $C, esto es tiempo"; exit 3; fi

echo "HILOS x MEMORIA en vc-cm · commit $(git rev-parse --short HEAD) · carga $C"
echo "L3 que alcanza un nucleo: 16 MiB · cada hilo carga su propio modelo (1,15 GB medidos)"
echo
# UNA CRUZ, NO UNA REJILLA. Cada celda es "H M".
#   eje de HILOS  con la memoria fija en 0      1, 2, 4, 6, 10
#   eje de MEMORIA con los hilos fijos en 6     0(compartida), -3, -5
# Siete celdas en vez de quince, y las ocho que se van incluian DOS w=1 mas a 2h25 cada una.
# El porque completo -y lo que la cruz NO puede ver- esta en la cabecera de este fichero.
#
# w=1 mem=0 va SALTADA: ya esta medida (8.704 s, 281.773.192 B, commit 48abdfd). Se deja escrita
# aqui, comentada y con su numero, para que se vea que la cruz esta completa y no coja.
#     "1 0"   <- YA MEDIDA: 8.704 s · 281.773.192 B · 2.461 MB · IPC 3,037
for c in "2 0" "4 0" "6 0" "10 0" "6 -3" "6 -5"; do
    set -- $c
    H=$1; M=$2
    rm -f "$W/o.vc"
    VC_MEM=$M VC_WORKERS=$H bench/mide.sh -H vc-cm -e "F18 hilos x memoria: w=$H mem=$M" \
      -m "arbol-repartido-755m" -c "workers=$H VC_MEM=$M" \
      -n "cruz, no rejilla: el eje de hilos a M=0 y el de memoria a H=6 (un hilo por nucleo fisico del Ryzen 3600, sin mezclar el SMT). w=1 mem=0 ya medida en 48abdfd" \
      -- "$VC" compress "$IN" -o "$W/o.vc" --level 9 --max
    B=$(stat -c%s "$W/o.vc" 2>/dev/null || echo 0)
    [ "$B" = 0 ] && { echo "   w=$H mem=$M: NO PRODUJO ARCHIVO"; exit 4; }
    # Los bytes tienen que ser IGUALES para todos los hilos con la misma memoria: si cambian,
    # el reparto en hilos esta cambiando el modelo y la comparacion de tiempo no vale nada.
    # A M=0 la referencia es la celda ya medida: 281.773.192 B.
    echo "   w=$H mem=$M  $B B"
done
echo
echo "Los bytes deben ser IDENTICOS dentro de cada bloque de memoria. Si no, el numero de hilos"
echo "esta cambiando el resultado y entonces no se estan cronometrando dos formas de hacer lo"
echo "mismo: se estan cronometrando dos cosas distintas."
echo FIN-HILOS
