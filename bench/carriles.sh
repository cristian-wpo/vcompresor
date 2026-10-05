#!/usr/bin/env bash
# LA ECONOMIA DE CADA CARRIL: que compra en bytes y que cuesta en CPU, uno por uno.
#
# POR QUE HACE FALTA. D8 lleva semanas diciendo que `--price` optimiza la perilla que no importa:
# sondea para elegir el NIVEL de zstd, que entre el 17 y el 19 mueve un 0,8% del tamaño, y no
# toca los CARRILES. Y el 2026-07-31 D13 lo puso en numeros: la decision de carril mueve un
# 43,6% de bytes y un 99,4% de CPU. Pero para que `--price` pueda elegir carriles hace falta
# saber, POR CARRIL, cuanto compra y cuanto cuesta — y eso no estaba medido para los cinco.
# `bench/ablacion-arbol.sh` solo cubre dos: agrupar y JPEG.
#
# COMO SE MIDE. Se apaga UN carril cada vez y se compara contra «completo». EN SERIE y
# ALTERNANDO, para que una deriva de carga reparta el daño en vez de caer entera sobre una
# configuracion. Todo por bench/mide.sh, que anota carga, commit y si el arbol estaba sucio.
#
# LO QUE ESTE GUION NO HACE, y hay que decirlo: no mide el carril del PLANO -expandir el DEFLATE
# a plano y trocearlo- por separado del de preflate, porque en el codigo son la misma decision.
# Apagar preflate apaga las dos cosas.
#
# Y SE COMPRUEBA LA IDA Y VUELTA de cada archivo. Una configuracion rapida que produce un archivo
# roto no es un ahorro, y midiendo solo segundos parece que si.
set -uo pipefail
cd "$(dirname "$0")/.."

IN=/tmp/compresor-f1/corpus
VC=$PWD/target/release/vcompresor
VUELTAS=${VUELTAS:-2}
NIVEL=${NIVEL:-9}

SUCIO=$(git status --porcelain -- . ':!bench/historia.jsonl' ':!bench/datos.json' ':!bench/panel.html' ':!bench/registros' ':!bench/registros-m3' | wc -l)
[ "$SUCIO" -eq 0 ] || { echo "ABORTA: $SUCIO ficheros de codigo sin commitear"; exit 3; }
CARGA=$(awk '{print $1}' /proc/loadavg)
if (( $(echo "$CARGA > 8" | bc -l) )); then echo "ABORTA: carga $CARGA, esto es tiempo"; exit 3; fi
[ -d "$IN" ] || { echo "ABORTA: no esta $IN"; exit 3; }

W=$(mktemp -d /tmp/car.XXXXXX); trap 'rm -rf "$W"' EXIT
echo "commit $(git rev-parse --short HEAD) · carga $CARGA · nivel $NIVEL · $VUELTAS vueltas"
echo "arbol: $(find "$IN" -type f | wc -l) ficheros"
echo

# nombre · variables de entorno · banderas
# «completo» pide --group EXPLICITO: lo que se mide no puede depender de cual sea el defecto de
# hoy. Ya paso una vez —el defecto cambio y la ablacion de agrupar se comparo consigo misma y
# publico tranquilamente que costaba 0,00%—.
CFGS=(
  "completo|
|--group"
  "sin-preflate|VC_SIN_PREFLATE=1
|--group"
  "sin-jpeg|
|--group --no-jpeg"
  "sin-e8e9|VC_SIN_E8E9=1
|--group"
  "sin-logts|VC_SIN_LOGTS=1
|--group"
  "sin-agrupar|
|--no-group"
)

for v in $(seq 1 "$VUELTAS"); do
  for c in "${CFGS[@]}"; do
    ETI="${c%%|*}"; resto="${c#*|}"
    ENV="${resto%%|*}"; BAN="${resto#*|}"
    ENV=$(echo "$ENV" | tr -d '\n')
    rm -f "$W/o.vc"
    # shellcheck disable=SC2086
    if [ -n "$ENV" ]; then
      env "$ENV" bench/mide.sh -e "F13 carril: $ETI (vuelta $v)" -m arbol2.7g -c "$ETI" \
        -n "economia por carril para --price (D8)" \
        -- "$VC" compress "$IN" -o "$W/o.vc" --level "$NIVEL" $BAN >/dev/null 2>&1
    else
      bench/mide.sh -e "F13 carril: $ETI (vuelta $v)" -m arbol2.7g -c "$ETI" \
        -n "economia por carril para --price (D8)" \
        -- "$VC" compress "$IN" -o "$W/o.vc" --level "$NIVEL" $BAN >/dev/null 2>&1
    fi
    if ! "$VC" verify "$W/o.vc" >/dev/null 2>&1; then
      echo "!! IDA Y VUELTA ROTA en «$ETI» — la medida no vale nada"; exit 4
    fi
    # SI «sin X» SACA LOS MISMOS BYTES QUE «completo», LA BANDERA NO HIZO NADA y la ablacion se
    # esta comparando consigo misma. Paso el 2026-07-31 con `agrupar`: el defecto es agrupar
    # ENCENDIDO, asi que «sin banderas» y «--group» eran la misma configuracion y salio un
    # limpisimo «agrupar compra 0,000%». Un guion correcto, un numero creible y una conclusion
    # inventada. Esto salta en el acto en vez de dejarlo llegar a la tabla.
    B=$(stat -c%s "$W/o.vc")
    if [ "$ETI" = "completo" ]; then BASE=$B
    elif [ "$B" = "${BASE:-}" ]; then
      echo "!! «$ETI» saca EXACTAMENTE los mismos bytes que «completo»: la bandera no hizo nada."
      echo "   La ablacion no mide la pieza, se mide a si misma. NO publicar."; exit 5
    fi
    echo "  vuelta $v · $(printf %-13s "$ETI") $(stat -c%s "$W/o.vc") B   carga $(awk '{print $1}' /proc/loadavg)"
  done
done

echo
echo "Ahora: python3 bench/carriles.py — pone precio a cada carril con estas filas."
echo FIN-CARRILES
