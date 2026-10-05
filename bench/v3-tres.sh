#!/usr/bin/env bash
# ¿SIGUE SOBRANDO «LOS TRES DISTINTOS» AHORA QUE EL DEFECTO ES v7?
#
# POR QUE HAY QUE VOLVER A MEDIRLO. La decision de quitarlo se tomo sobre v5→v6 y el panel la
# marca REVISAR desde que entro la v7: el fichero donde vive se toco despues de medirla. No es
# burocracia — es la ley de la casa: un negativo caduca con su premisa, y la premisa cambio
# cuando la dispersion salio del defecto.
#
# Y de paso cierra el otro agujero: aquella tanda se quedo en CUATRO de las cinco muestras -falta
# enwik8- y aun asi se aplico por defecto. Esta va sobre las CINCO.
#
# A/B: el defecto de hoy (v7, sin los tres) contra el mismo binario con VC_TRES=1. La perilla
# fuerza la vista encendida sin cambiar nada mas, asi que la unica diferencia entre los dos lados
# es la que se quiere medir.
#
# EN SERIE y con el cerrojo: mide reloj.
set -uo pipefail
cd "$(dirname "$0")/.."
S=/tmp/compresor-f1/samples
W=$(mktemp -d /tmp/v3.XXXXXX)
trap "rm -rf $W" EXIT
for m in codigo.10m csvjson.10m enwik8.10m logs.10m sql.10m; do
  VC_TRES=1 bench/mide.sh -H vc-cm -e "V3 tres sobre v7" -m "$m" -c "con tres (VC_TRES=1)" \
    -n "Re-medida sobre el defecto v7: la decision de quitarlos se tomo sobre v5-v6 y la premisa cambio. Las CINCO muestras, que la vez anterior fueron cuatro." \
    -- ./target/release/vcompresor compress "$S/$m" -o "$W/a.vc" --max
  bench/mide.sh -H vc-cm -e "V3 tres sobre v7" -m "$m" -c "v7 DEFECTO (sin tres)" \
    -n "El otro lado del A/B: el defecto tal y como se instala." \
    -- ./target/release/vcompresor compress "$S/$m" -o "$W/b.vc" --max
done
echo "== V3 terminada =="
