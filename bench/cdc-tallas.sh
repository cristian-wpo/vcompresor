#!/usr/bin/env bash
# ¿ESTAN LOS TROZOS DIMENSIONADOS PARA NUESTRO ARBOL O PARA OTRO?
#
# LOS NUMEROS DE HOY: min 64 KiB, medio 256 KiB, max 1 MiB. Vienen de la literatura de
# deduplicacion, hecha sobre IMAGENES DE MAQUINA VIRTUAL Y RESPALDOS DE DISCO. Nuestro arbol es
# codigo fuente.
#
# MEDIDO sobre el arbol real (43.298 ficheros, 2,44 GiB):
#     por debajo de 64 KiB (el MINIMO)   38.224 ficheros  88,3%    243 MB   9,3% de los bytes
#     por debajo de 1 MiB  (el MAXIMO)   42.995 ficheros  99,3%   1,19 GB  45,3% de los bytes
# El 99,3% de los ficheros es MAS PEQUEÑO QUE EL TROZO MAXIMO, y en el 88,3% el troceador no
# puede cortar NI UNA VEZ porque no llega al minimo. En casi la mitad de los bytes del archivo la
# deduplicacion por debajo del fichero NO PUEDE OCURRIR.
#
# ⚠️ LO QUE ESTA PRUEBA PUEDE Y NO PUEDE DECIR. Trozos mas pequeños dan mas oportunidades de
# deduplicar PERO tambien mas entradas de indice, y el indice se guarda. Puede salir peor. Por eso
# se mide el ARCHIVO ENTERO y no la tasa de dedup: la tasa subiria siempre y no significaria nada.
#
# ══════════════════════════════════════════════════════════════════════════════════════════════
# LO QUE PASO EL 31-07, Y ES LA LECCION MAS CARA DEL DIA
#
# Esta prueba dijo por la mañana: -0,972% con 2/8/32 KiB, monotono y sin girar la curva. Se anoto
# asi y estuvo horas en la lista de pendientes con ese numero. Por la tarde, con el MISMO guion y
# el MISMO corpus, salio -0,1917% y la curva SI giraba (en 4/16/64).
#
# No fallo ninguna de las dos medidas. Lo que cambio en medio fue el CODIGO: `--solid=64` por
# defecto y la ventana de zstd igualada al frame. Comprobado a proposito, corriendo este mismo
# barrido con `--no-solid`:
#     frames de antes   64/256/1024  314.110.498     2/8/32  311.057.188   -0,9720%
#     frames de hoy     64/256/1024  304.401.584     2/8/32  304.043.133   -0,1178%
#                                                   4/16/64  303.818.185   -0,1917%  <- el mejor
# El -0,9720% reaparece EXACTO con los frames viejos. Cuatro decimales.
#
# POR QUE. Los dos mecanismos hacen el mismo trabajo por caminos distintos: el troceado fino busca
# repeticion ENTRE FICHEROS partiendo mas veces, y un frame de 64 MiB con la ventana a juego la
# encuentra sin partir nada, porque le cabe todo en la ventana. Arreglar uno se come lo que el otro
# iba a comprar. El 80%, aqui.
#
# LA REGLA QUE SALE DE ESTO, y va al panel: un numero no viaja solo con COMO se midio (R7), viaja
# tambien con EL ESTADO DEL CODIGO en que se midio. Una medida vieja de una parte que otro cambio
# ha tocado no es «un dato pendiente de aplicar»: es una sobreestimacion en silencio. Se vuelve a
# medir antes de decidir con ella.
# ══════════════════════════════════════════════════════════════════════════════════════════════
#
# EL RELOJ, medido el 31-07 y era la unica objecion viva: NO LA HAY. El barrido entero cae entre
# 25,9 s y 27,6 s sin orden, y lo que parecia un -7,8% de la configuracion de hoy era CACHE FRIO:
# invertido el orden, la primera vuelve a ser la mas lenta sea cual sea. Ocho veces mas cortes -y
# por tanto ocho veces mas hash rodante y mas entradas de indice que hashear- no se nota contra el
# resto del trabajo. La objecion «mas trozos es mas hasheo» esta cerrada: cuesta cero.
#
# ⚠️ LO QUE FALTA antes de tocar el defecto: los dos corpus NO estan de acuerdo en cual es el
# optimo -2/8/32 en el arbol de codigo del servidor, 4/16/64 en el corpus mixto del Mac- y cambiar
# un defecto midiendo en uno solo es justo el error que esta prueba existe para no cometer.
set -uo pipefail
# A LA RAIZ DEL ARBOL, no al directorio del guion. Este vive en bench/, asi que un `cd` al
# suyo dejaba $PWD/target/release/vcompresor apuntando a bench/target/... y las cinco
# configuraciones daban NO PRODUJO ARCHIVO. En el Mac el guion esta en la raiz y por eso alli
# funcionaba: el mismo fichero, dos sitios distintos, un fallo que solo sale en uno.
cd "$(dirname "$0")/.."
. bench/comun.sh
# El corpus del Mac. Se usa el arbol REAL y no una muestra: la pregunta es cuantos
# ficheros caen por debajo del trozo minimo, y eso depende de la DISTRIBUCION de
# tallas. Una muestra podria cambiarla.
IN=${IN:?define IN con la carpeta del corpus a medir}
VC=$PWD/target/release/vcompresor
W=$(mktemp -d /tmp/cdc.XXXXXX); trap 'rm -rf "$W"' EXIT
echo "TROZOS DEL CDC · entrada $(du -sh $IN | cut -f1) · commit $(git rev-parse --short HEAD 2>/dev/null || echo sin-git) · $(uname -s) carga $(carga)"
echo
# EL RELOJ VA AQUI Y NO EN OTRO GUION, y eso convierte esta prueba en una de TIEMPO (tipo T en
# bench/pruebas.sh, o sea EN SERIE y con cerrojo). Los bytes son deterministas y se podian medir
# en paralelo; en cuanto una prueba reporta un reloj, deja de poder. Partirla en dos guiones
# significaria correr el mismo barrido dos veces para sacar dos numeros que la MISMA pasada ya
# produce.
#
# Y el reloj es lo unico que decide esto. Los bytes ya dijeron -0,972% con 2/8/32 KiB, monotono y
# sin girar la curva. Pero trozos ocho veces mas pequeños son ocho veces mas cortes, o sea ocho
# veces mas hash rodante y ocho veces mas entradas de indice que hashear con blake3. Si eso cuesta
# mas de lo que el -0,972% compra a precio de disco, la respuesta correcta es NO tocarlo.
printf "  %-30s %14s %10s %9s %8s\n" "min / medio / max" "bytes" "vs hoy" "reloj" "vs hoy"
BASE=""; TBASE=""
# ⚠️ HASTA EL 01-08 ESTA LISTA SOLO BAJABA. Se barrian cinco tallas TODAS MAS PEQUEÑAS que la de
# hoy, asi que si el optimo estaba POR ENCIMA no habia forma de verlo: el barrido solo podia
# confirmar el ajuste actual, nunca refutarlo por arriba. Es el mismo fallo que el corpus de
# PESO_LONG -medir en la mitad del espacio y concluir sobre el todo-. Ahora sube tambien.
for c in "65536 262144 1048576" "262144 1048576 4194304" "1048576 4194304 16777216" "16384 65536 262144" "8192 32768 131072" "4096 16384 65536" "2048 8192 32768"; do
  set -- $c
  rm -f "$W/o.vc"
  T0=$(python3 -c 'import time;print(time.time())')
  VC_CDC_MIN=$1 VC_CDC_AVG=$2 VC_CDC_MAX=$3 "$VC" compress "$IN" -o "$W/o.vc" --level 9 >/dev/null 2>&1
  T1=$(python3 -c 'import time;print(time.time())')
  B=$(talla "$W/o.vc")
  [ "$B" = 0 ] && { echo "  $1/$2/$3: NO PRODUJO ARCHIVO"; continue; }
  # IDA Y VUELTA: cambiar el troceado cambia el indice entero. Va DESPUES de parar el reloj:
  # cronometrar la verificacion junto con el archivado mezclaria dos cosas distintas.
  "$VC" verify "$W/o.vc" >/dev/null 2>&1 && V="" || V="  ⚠️IDA Y VUELTA ROTA"
  [ -z "$BASE" ] && { BASE=$B; TBASE=$(python3 -c "print($T1-$T0)"); }
  ET="$(($1/1024)) / $(($2/1024)) / $(($3/1024)) KiB"
  [ "$B" = "$BASE" ] && ET="$ET  <- HOY"
  python3 -c "
t=$T1-$T0
print(f'  {\"$ET\":<30} {$B:>14,} {100*($B-$BASE)/$BASE:>9.4f}% {t:>8.1f}s {100*(t-$TBASE)/$TBASE:>+7.1f}%$V')"
done
echo FIN-CDC
