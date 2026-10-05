#!/usr/bin/env bash
# ¿EL TROCEADO FINO GANA EN TODOS LOS TIPOS DE CONTENIDO, O SOLO EN ALGUNOS?
#
# Norma del dueño: «las cosas pueden ser dinamicas — si en un lado es mejor pero en el otro no,
# entonces ver si de verdad se puede aprovechar».
#
# LO QUE YA SABEMOS, Y POR QUE OBLIGA A ESTA PRUEBA:
#     codigo   avg=16K  ->  -1,629%   gana
#     dup      avg=16K  ->  +0,148%   pierde
# Con eso, cualquier valor GLOBAL es la respuesta equivocada por construccion: o se deja la
# ganancia del texto sobre la mesa, o se paga en lo demas. La pregunta no es «cual es el mejor
# avg» sino «¿hay un avg por TIPO que gane en cada uno?».
#
# SE PUEDE APROVECHAR, y no en teoria: el archivador YA enruta por contenido. firma_de mira la
# firma real del fichero —no la extension— y lane_for lo manda a su carril. Si ya sabe que algo
# es texto o es JPEG, puede trocearlo con el avg que le convenga a ESE carril. Los cortes van
# como desplazamientos en el indice, asi que trozos cortados con parametros distintos conviven;
# lo unico que se pierde es dedup ENTRE carriles, y entre un JPEG y un .php no habia ninguno.
#
# LO QUE ESTA PRUEBA NO CONTESTA, dicho antes de empezar: el coste en TIEMPO. 16x mas trozos son
# 16x mas llamadas a blake3 y 16x mas entradas de indice. Los bytes de indice SI estan contados
# —van dentro del archivo que se pesa— pero la CPU no. Eso se mide con la maquina quieta.
set -uo pipefail
cd "$(dirname "$0")/.."
VC=$PWD/target/release/vcompresor
W=$(mktemp -d /tmp/tipo.XXXXXX)
mkdir -p bench/registros/fases
echo "carga: $(awk '{print $1}' /proc/loadavg) — solo cuentan los BYTES, que son deterministas"
echo

for M in codigo dup jpg png png-normal logs sql csvjson pdfoffice; do
  IN=/tmp/compresor-f1/corpus/$M
  [ -d "$IN" ] || continue
  N=$(find "$IN" -type f | wc -l)
  [ "$N" -lt 20 ] && continue          # con menos de 20 ficheros el numero no dice nada
  for C in "65536 262144|hoy" "4096 16384|fino"; do
    P="${C%%|*}"; ET="${C##*|}"
    read -r MN AV <<< "$P"
    rm -f "$W/o.vc"
    E="bench/registros/fases/F8-$M-$ET.txt"
    VC_CDC_MIN=$MN VC_CDC_AVG=$AV bench/mide.sh \
      -e "F8 troceado por tipo: $M $ET" -m "$M" -c "avg=$AV" \
      -n "SOLO BYTES: carga alta. Decide si el avg debe ir por carril." -s "$E" \
      -- "$VC" compress "$IN" -o "$W/o.vc" --level 9 >/dev/null 2>&1 || echo "  !! fallo en $M/$ET"
    # el guardia avisa por stderr y hasta hoy la bascula se lo tragaba; ahora se comprueba
    grep -q "incoherentes" "$E" 2>/dev/null && echo "  !! $M/$ET: el guardia rechazo los parametros, la fila NO vale"
  done
  echo "  $M ($N ficheros) medido"
done
rm -rf "$W"

echo
echo "=== ¿MERECE LA PENA UN avg POR CARRIL? ==="
python3 - <<'PY'
import json, collections
G = collections.defaultdict(dict)
for l in open("bench/historia.jsonl"):
    f = json.loads(l)
    if f["etiqueta"].startswith("F8 troceado por tipo") and f["rc"] == 0 and f["bytes_out"]:
        G[f["muestra"]][f["config"]] = f
if not G:
    print("  sin filas F8"); raise SystemExit

print(f"  {'tipo':<12} {'ficheros':>9} {'hoy (B)':>14} {'fino (B)':>14} {'cambio':>9}  veredicto")
gana = pierde = igual = 0
tot_hoy = tot_fino = 0
for m, c in sorted(G.items()):
    h = c.get("avg=262144"); f_ = c.get("avg=16384")
    if not h or not f_: continue
    d = 100 * (f_["bytes_out"] - h["bytes_out"]) / h["bytes_out"]
    tot_hoy += h["bytes_out"]; tot_fino += f_["bytes_out"]
    if d < -0.05:   v = "FINO gana"; gana += 1
    elif d > 0.05:  v = "HOY gana"; pierde += 1
    else:           v = "da igual"; igual += 1
    print(f"  {m:<12} {h['bytes_in']//max(1,1):>9} {h['bytes_out']:14,d} {f_['bytes_out']:14,d} {d:+8.3f}%  {v}")

print()
print(f"  fino gana en {gana} tipos · hoy gana en {pierde} · da igual en {igual}")
d = 100 * (tot_fino - tot_hoy) / tot_hoy
print(f"  Si se pusiera fino EN TODO: {d:+.3f}% sobre la suma de estos corpus.")
print()
print("  LO QUE DECIDE: si fino gana en unos tipos y pierde en otros, un avg GLOBAL deja")
print("  dinero encima de la mesa haga lo que haga, y la respuesta correcta es por carril.")
print("  Si gana o pierde en TODOS, entonces es un solo numero y no hay nada dinamico que hacer.")
PY
echo FIN-TIPO
