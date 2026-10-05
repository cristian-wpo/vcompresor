#!/usr/bin/env bash
# ¿CUAL DE LOS TRES PARAMETROS ES EL QUE GANA?
#
# EL FALLO DEL BARRIDO ANTERIOR, dicho claro: movio min, avg y max A LA VEZ. Con eso se sabe que
# 4/16/64 KiB saca un 1,63% menos en el corpus de codigo, y no se sabe POR QUE. Un barrido que
# mueve tres perillas juntas mide la combinacion, no las perillas.
#
# Y hay una señal de que ahi dentro pasan dos cosas cruzadas: la curva NO es monotona.
#
#     4/ 16/  64   -1,628%     <- gana
#     8/ 32/ 128   -1,422%     <- gana
#    16/ 64/ 256   +0,035%     <- pierde (!)
#    32/128/ 512   +0,114%     <- pierde
#    64/256/1024    0,000%     <- el de hoy
#   128/512/2048   +2,282%     <- pierde mucho
#
# Si fuera un solo efecto «mas fino = mejor», la columna bajaria de arriba abajo sin volver a
# subir. Sube en el medio. Eso solo pasa si hay dos efectos con signos distintos, y hasta
# separarlos cualquier explicacion que demos es un cuento.
#
# AQUI SE MUEVE UNA SOLA PERILLA CADA VEZ, dejando las otras dos en su valor de hoy. Es mas
# lento y es la unica forma de saber cual manda.
#
# Se mide en BYTES: deterministas, valen con la maquina a carga 44. El tiempo NO se mide aqui.
set -uo pipefail
cd "$(dirname "$0")/.."
VC=$PWD/target/release/vcompresor
W=$(mktemp -d /tmp/cdca.XXXXXX)
IN=/tmp/compresor-f1/corpus/codigo
[ -d "$IN" ] || { echo "no esta $IN"; exit 3; }
echo "carga: $(awk '{print $1}' /proc/loadavg) — solo cuentan los BYTES"
echo "corpus: codigo · $(find $IN -type f | wc -l) ficheros"
echo

corre() {  # $1=min $2=avg $3=max $4=que se mueve
  rm -f "$W/o.vc"
  VC_CDC_MIN=$1 VC_CDC_AVG=$2 VC_CDC_MAX=$3 bench/mide.sh \
    -e "F6.1 una perilla cada vez: $4" -m codigo -c "min=$1 avg=$2 max=$3" \
    -n "SOLO BYTES: carga alta. Mueve $4 dejando las otras dos como hoy." \
    -- "$VC" compress "$IN" -o "$W/o.vc" --level 9 >/dev/null 2>&1 \
    && echo "  $4: $(stat -c%s $W/o.vc) B" || echo "  $4: FALLO"
}

echo "--- referencia ---"
corre 65536 262144 1048576 "el de hoy 64/256/1024"
echo
echo "--- solo MIN (avg y max como hoy) ---"
for M in 4096 8192 16384 32768; do corre $M 262144 1048576 "min=$((M/1024))K"; done
echo
echo "--- solo AVG (min y max como hoy) ---"
for A in 16384 32768 65536 131072; do corre 4096 $A 1048576 "avg=$((A/1024))K con min=4K"; done
echo
echo "--- solo MAX (min y avg como hoy) ---"
for X in 65536 131072 262144 524288; do corre 65536 262144 $X "max=$((X/1024))K"; done
rm -rf "$W"

echo
echo "=== A QUE SE DEBE LA GANANCIA ==="
python3 - <<'PY'
import json, collections
R = []
for l in open("bench/historia.jsonl"):
    f = json.loads(l)
    if f["etiqueta"].startswith("F6.1 una perilla") and f["rc"] == 0 and f["bytes_out"]:
        R.append(f)
if not R:
    print("  sin filas"); raise SystemExit
base = next((r for r in R if r["config"] == "min=65536 avg=262144 max=1048576"), None)
if not base:
    print("  sin referencia"); raise SystemExit
b = base["bytes_out"]
grupos = collections.defaultdict(list)
for r in R:
    et = r["etiqueta"].split(": ", 1)[1]
    g = "MIN" if et.startswith("min=") else "AVG" if et.startswith("avg=") else "MAX" if et.startswith("max=") else "ref"
    grupos[g].append((et, r["bytes_out"]))
for g in ("MIN", "AVG", "MAX"):
    if not grupos[g]: continue
    print(f"\n  moviendo solo {g}:")
    for et, by in grupos[g]:
        print(f"    {et:<26} {by:12,d} B   {100*(by-b)/b:+7.3f}%")
print()
print("  El que mueva mas la columna de la derecha es el que manda. Si ninguno solo llega")
print("  al -1,63% de la combinacion, entonces la ganancia es de la INTERACCION y hay que")
print("  decirlo asi en vez de atribuirsela a uno.")
PY
echo FIN-ATRIBUYE
