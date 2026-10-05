#!/usr/bin/env bash
# ¿CUANTA MEMORIA APRENDIDA HACE FALTA? — y esta vez CON EL RELOJ.
#
# LO QUE FALTABA. bench/memoria.tsv ya barrio VC_MEM de 0 a -3 y anoto bytes y RAM:
#     VC_MEM   bytes       delta      ram_mb
#       0      3.210.419   —          1.159
#      -1      3.217.800   +0,230%      670
#      -2      3.229.955   +0,609%      438
#      -3      3.249.476   +1,217%      291
# O sea que MIDIO LO QUE CUESTA y no midio LO QUE COMPRA. Toda la premisa es «tablas mas
# pequeñas caben en cache y por eso van mas rapido», y no hay ni una columna de tiempo. Un 0,230%
# mas de bytes por la mitad de memoria puede ser un negocio buenisimo o dar igual, y hoy no se
# sabe cual de las dos.
#
# Y SE PARABA EN -3. Para que las tablas quepan en los 16 MiB de L3 que alcanza un nucleo de este
# Ryzen -32 MiB en DOS bloques, y un nucleo solo llega al suyo- hacen falta unos cinco bits mas.
# Extrapolar desde -3 seria justo lo que esta casa no hace.
#
# ⚠️ ESTE MIDE TIEMPO, asi que va EN SERIE y en una sola maquina. No se lanza a la vez que otra
# medida de reloj. Los bytes de la misma pasada valen igual, pero el reloj manda el protocolo.
set -uo pipefail
cd "$(dirname "$0")"
IN=${IN:-/tmp/compresor-f1/corpus}
VC=./target/release/vcompresor
W=$(mktemp -d /tmp/mem.XXXXXX); trap 'rm -rf "$W"' EXIT

SUCIO=$(git status --porcelain -- . ':!bench/historia.jsonl' ':!bench/datos.json' ':!bench/panel.html' ':!bench/registros' ':!bench/registros-m3' 2>/dev/null | wc -l)
[ "$SUCIO" -eq 0 ] || { echo "ABORTA: $SUCIO ficheros sin commitear, y esto mide tiempo"; exit 3; }
C=$(awk '{print $1}' /proc/loadavg)
if (( $(echo "$C > 8" | bc -l) )); then echo "ABORTA: carga $C, esto es tiempo"; exit 3; fi

echo "MEMORIA APRENDIDA vs RELOJ · commit $(git rev-parse --short HEAD) · carga $C"
echo "L3 que alcanza un nucleo: 16 MiB (Ryzen 5 3600, 32 MiB en dos bloques)"
echo
for M in 0 -1 -2 -3 -4 -5 -6; do
  rm -f "$W/o.vc"
  VC_MEM=$M bench/mide.sh -H vc-cm -e "F18 memoria vs reloj: VC_MEM=$M" -m arbol2.7g \
    -c "VC_MEM=$M" \
    -n "el barrido viejo midio bytes y RAM y NO midio el reloj, que es toda la premisa" \
    -- "$VC" compress "$IN" -o "$W/o.vc" --level 9 --max
  B=$(stat -c%s "$W/o.vc" 2>/dev/null || echo 0)
  [ "$B" = 0 ] && { echo "  VC_MEM=$M: NO PRODUJO ARCHIVO"; exit 4; }
  # IDA Y VUELTA: cambiar el tamaño de tabla cambia el modelo, y el descompresor tiene que
  # llegar al mismo. Si no vuelve, el ahorro no existe.
  if "$VC" verify "$W/o.vc" >/dev/null 2>&1; then V="ida y vuelta OK"; else V="ROTA"; fi
  echo "   VC_MEM=$M  $B B  $V"
done
echo FIN-MEMORIA
