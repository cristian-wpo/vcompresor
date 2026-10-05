#!/usr/bin/env bash
# ¿CUANTO CPU CUESTA EL TROCEADO FINO DEL PLANO? La deuda del commit 8d9c949.
#
# Ese commit se llevo -1,691% de tamaño con la ganancia MEDIDA en bytes y el coste en CPU SIN
# MEDIR, porque el servidor llevaba la tarde entre carga 29 y 56. Se dijo asi en el propio
# mensaje del commit, en vez de callarlo. Ahora la carga ha bajado y se paga la deuda.
#
# 16x mas trozos en el plano son 16x mas llamadas a blake3 y mas entradas de indice. Si cuesta
# poco, el cambio se queda. Si cuesta mucho, va detras de --price o se revierte.
#
# EN SERIE, alternando, dos vueltas, y ABORTA si la maquina no esta quieta. Sobre png-normal,
# que es donde mas se nota (-4,370%).
set -uo pipefail
cd "$(dirname "$0")/.."
CARGA=$(awk '{print $1}' /proc/loadavg)
if (( $(echo "$CARGA > 8" | bc -l) )); then
  echo "ABORTA: carga $CARGA. Esto es una medida de TIEMPO y no se fuerza."; exit 3
fi
VC=$PWD/target/release/vcompresor
IN=/tmp/compresor-f1/corpus/png-normal
W=$(mktemp -d /tmp/tp.XXXXXX)
echo "carga de salida: $CARGA · $(find "$IN" -type f | wc -l) ficheros"
for v in 1 2; do
  for AVG in 262144 16384; do
    if [ "$AVG" = "16384" ]; then ET="plano fino (lo commiteado)"; MIN=4096
    else ET="plano como el resto"; MIN=65536; fi
    rm -f "$W/o.vc"
    VC_CDC_PLANO_AVG=$AVG VC_CDC_PLANO_MIN=$MIN \
      bench/mide.sh -e "F9 coste en CPU del plano fino: $ET (vuelta $v)" \
      -m png-normal -c "plano avg=$AVG" \
      -n "paga la deuda del commit 8d9c949: la ganancia iba medida y el coste no" \
      -- "$VC" compress "$IN" -o "$W/o.vc" --level 9 || echo "  !! fallo"
    echo "   carga ahora: $(awk '{print $1}' /proc/loadavg)"
  done
done
rm -rf "$W"

echo
python3 - <<'PY'
import json, collections
G = collections.defaultdict(list)
for l in open("bench/historia.jsonl"):
    f = json.loads(l)
    if f["etiqueta"].startswith("F9 coste en CPU del plano") and f["rc"] == 0:
        G[f["config"]].append(f)
base, fino = G.get("plano avg=262144"), G.get("plano avg=16384")
if not (base and fino):
    print("  faltan filas"); raise SystemExit


def m(rs, k):
    return sum(r[k] for r in rs) / len(rs)


print("  %-22s %11s %10s %13s %6s" % ("config", "ciclos G", "reloj s", "bytes", "carga"))
for n, rs in (("como el resto", base), ("plano fino", fino)):
    print("  %-22s %11.1f %10.1f %13d %6.1f" %
          (n, m(rs, "ciclos") / 1e9, m(rs, "seg"), int(m(rs, "bytes_out")), m(rs, "carga")))
dc = 100 * (m(fino, "ciclos") - m(base, "ciclos")) / m(base, "ciclos")
dt = 100 * (m(fino, "seg") - m(base, "seg")) / m(base, "seg")
db = 100 * (m(fino, "bytes_out") - m(base, "bytes_out")) / m(base, "bytes_out")
cs = [r["ciclos"] for r in base]
suelo = 100 * (max(cs) - min(cs)) / min(cs) if len(cs) > 1 else None
print()
print(f"  el plano fino cuesta {dc:+.2f}% de ciclos y {dt:+.2f}% de reloj,")
print(f"  y compra {db:+.3f}% de bytes.")
if suelo is not None:
    print(f"  suelo de ruido de esta tanda (la base repetida): {suelo:.2f}%")
    print("  -> " + ("el coste NO es medible: cae por debajo del ruido del propio banco"
                     if abs(dc) < suelo else
                     "el coste SI se distingue del ruido, asi que hay que ponerle precio"))
PY
echo FIN-TIEMPO-PLANO
