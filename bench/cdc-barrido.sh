#!/usr/bin/env bash
# BARRER LA PERILLA QUE NUNCA SE HA TOCADO: los parametros del troceado por contenido.
#
# CDC_MIN=64K, CDC_AVG=256K, CDC_MAX=1M deciden DONDE se corta, y con eso todo el dedup. No hay
# ni una fila en bench/historia.jsonl que los haya barrido: se pusieron una vez y ahi siguen.
#
# LO QUE HACE LA PREGUNTA URGENTE. El 88,3% de los ficheros del arbol real esta por DEBAJO de
# CDC_MIN (bench/tallas.tsv). Para nueve de cada diez ficheros el troceador no rueda ni un byte:
# son un trozo entero, y el dedup solo puede funcionar a fichero completo. Trozos mas pequeños
# darian dedup mas fino; tambien mas entradas de indice. Nadie ha medido el cambio.
#
# ── POR QUE ESTO SE PUEDE MEDIR HOY, CON LA MAQUINA A CARGA 54 ───────────────────────────────
# Porque se mide en BYTES, y los bytes que salen son DETERMINISTAS: la misma entrada con los
# mismos parametros da el mismo archivo, corra la maquina sola o con media flota encima. El
# tiempo NO se puede medir hoy —el suelo de ruido del banco es 2,05% con la maquina quieta y a
# carga 54 el mismo trabajo gasta un 38% mas de ciclos— asi que el tiempo se mide otro dia y
# aqui se ignora aunque mide.sh lo anote.
#
# Emparejar el trabajo con el estado de la maquina es parte del metodo: hay preguntas que se
# pueden contestar con la maquina llena y preguntas que no.
set -uo pipefail
cd "$(dirname "$0")/.."
VC=$PWD/target/release/vcompresor
W=$(mktemp -d /tmp/cdc.XXXXXX)

echo "carga: $(awk '{print $1}' /proc/loadavg) — da igual: aqui solo cuentan los BYTES"
echo

# min · avg · max · por que esta combinacion en la lista
COMBOS=(
  "65536 262144 1048576|el de hoy, nunca medido"
  "4096 16384 65536|16x mas fino: el 88,3% de los ficheros dejaria de ser un solo trozo"
  "8192 32768 131072|8x mas fino"
  "16384 65536 262144|4x mas fino"
  "32768 131072 524288|2x mas fino"
  "131072 524288 2097152|2x mas grueso: menos indice, dedup mas basto"
)

for M in dup codigo; do
  IN=/tmp/compresor-f1/corpus/$M
  [ -d "$IN" ] || { echo "salto $M"; continue; }
  echo "=== $M · $(find $IN -type f | wc -l) ficheros · $(du -sh $IN | cut -f1) ==="
  for C in "${COMBOS[@]}"; do
    P="${C%%|*}"; POR="${C##*|}"
    read -r MN AV MX <<< "$P"
    rm -f "$W/o.vc"
    VC_CDC_MIN=$MN VC_CDC_AVG=$AV VC_CDC_MAX=$MX bench/mide.sh \
      -e "F6 barrido del troceado: $((MN/1024))/$((AV/1024))/$((MX/1024)) KiB" \
      -m "$M" -c "min=$MN avg=$AV max=$MX" \
      -n "SOLO VALEN LOS BYTES: tomado a carga alta, el tiempo de esta fila no es comparable. $POR" \
      -- "$VC" compress "$IN" -o "$W/o.vc" --level 9 || echo "  !! fallo con $P"
  done
  echo
done
rm -rf "$W"

echo "=== LO QUE DICEN LOS BYTES ==="
python3 - <<'PY'
import json, collections
G = collections.defaultdict(dict)
POR = {}
for l in open("bench/historia.jsonl"):
    f = json.loads(l)
    if f["etiqueta"].startswith("F6 barrido del troceado") and f["rc"] == 0 and f["bytes_out"]:
        G[f["muestra"]][f["config"]] = f
        POR[f["config"]] = f["nota"].split(". ", 1)[-1]
if not G:
    print("  sin filas F6"); raise SystemExit

for m, cfgs in G.items():
    base = next((v for k, v in cfgs.items() if "min=65536" in k), None)
    print(f"\n  {m}:")
    print(f"    {'min/avg/max':>18} {'bytes':>13} {'ratio':>7} {'vs el de hoy':>13}   por que")
    orden = sorted(cfgs.items(), key=lambda kv: int(kv[0].split("min=")[1].split()[0]))
    for k, v in orden:
        mn = int(k.split("min=")[1].split()[0]) // 1024
        av = int(k.split("avg=")[1].split()[0]) // 1024
        mx = int(k.split("max=")[1].split()[0]) // 1024
        r = v["bytes_in"] / v["bytes_out"]
        d = 100 * (v["bytes_out"] - base["bytes_out"]) / base["bytes_out"] if base else 0
        marca = "  <- el de hoy" if base and v["bytes_out"] == base["bytes_out"] and "min=65536" in k else ""
        print(f"    {mn:5d}/{av:5d}/{mx:5d} {v['bytes_out']:13,d} {r:7.3f} {d:+12.3f}%   {POR.get(k,'')[:40]}{marca}")

print()
print("  Los bytes son exactos. El TIEMPO de estas filas NO vale: se tomaron a carga alta.")
print("  Si alguna combinacion gana en bytes, hay que medirle el tiempo con la maquina quieta")
print("  antes de decidir nada: trozos mas finos comprimen mejor y cuestan mas indice y mas CPU.")
PY
echo FIN-CDC
