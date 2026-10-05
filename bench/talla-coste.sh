#!/usr/bin/env bash
# ¿CUANTO CUESTA UN FICHERO POR SER UN FICHERO, Y CUANTO POR LOS BYTES QUE TRAE?
#
# ES LA MEDIDA QUE DECIDE DONDE MERECE LA PENA INVERTIR. Hoy se sabe que el arbol real cuesta
# 15 ms de CPU POR FICHERO de media y que su mediana son 1.972 B. Con esos dos numeros no se
# puede saber lo unico que importa: si esos 15 ms se pagan por ABRIR EL FICHERO o por COMPRIMIR
# SUS BYTES. Y de eso depende todo lo demas:
#
#   si manda el coste POR FICHERO  -> quitar syscalls y aperturas es lo mas rentable del proyecto
#   si manda el coste POR BYTE     -> lo de los syscalls es cosmetica y hay que ir al compresor
#
# COMO SE SEPARAN. Se construyen arboles con LOS MISMOS BYTES repartidos en ficheros de tallas
# distintas: 2 KiB, 16 KiB, 128 KiB, 1 MiB y 8 MiB. Si el coste fuera solo de bytes, los cinco
# tardarian lo mismo. Lo que se desvie es el coste por fichero. Con cinco puntos sale una recta:
#
#       tiempo = a * (numero de ficheros) + b * (bytes)
#
# y `a` es exactamente lo que cuesta un fichero por existir, en microsegundos. Ese numero es la
# respuesta, y hasta hoy no lo teniamos.
#
# EL CONTENIDO ES EL MISMO EN LOS CINCO. Se corta un unico flujo de bytes reales del corpus en
# trozos del tamaño que toque. Si cada arbol llevara contenido distinto, la diferencia de tiempo
# podria ser de compresibilidad y no de talla, y no habria forma de distinguirlo.
#
# EN SERIE y con la maquina quieta, porque esto es tiempo. Aborta si la carga pasa de 8.
set -uo pipefail
cd "$(dirname "$0")/.."

VC=$PWD/target/release/vcompresor
FUENTE=/tmp/compresor-f1/corpus/codigo      # bytes reales, no ceros ni aleatorio
TOTAL=$((512 * 1024 * 1024))                # los mismos bytes en todos los arboles
VUELTAS=${VUELTAS:-2}

CARGA=$(awk '{print $1}' /proc/loadavg)
(( $(echo "$CARGA > 8" | bc -l) )) && { echo "ABORTA: carga $CARGA, esto es una medida de tiempo"; exit 3; }
[ -d "$FUENTE" ] || { echo "ABORTA: no esta $FUENTE"; exit 3; }

W=$(mktemp -d /tmp/talla.XXXXXX)
echo "carga de salida: $CARGA · $((TOTAL/1024/1024)) MiB por arbol · $VUELTAS vueltas"
echo

# Un solo flujo de bytes reales, del que se cortan todos los arboles.
echo "preparando el flujo comun ($((TOTAL/1024/1024)) MiB de codigo real)..."
find "$FUENTE" -type f -print0 | xargs -0 cat 2>/dev/null | head -c $TOTAL > "$W/flujo.bin"
REAL=$(stat -c%s "$W/flujo.bin")
echo "  flujo: $REAL bytes"
echo

for TALLA in 2048 16384 131072 1048576 8388608; do
  D="$W/t$TALLA"; mkdir -p "$D"
  # split reparte el flujo en ficheros de TALLA bytes exactos. Los nombres van en subcarpetas
  # de 1.000 para no hacer un directorio de 262.144 entradas, que mediria otra cosa.
  ( cd "$D" && split -b "$TALLA" -a 6 -d "$W/flujo.bin" f_ )
  N=$(ls "$D" | wc -l)
  if [ "$N" -gt 40000 ]; then
    echo "$((TALLA/1024)) KiB: $N ficheros — demasiados, se recorta a 40.000"
    ls "$D" | tail -n +40001 | while read -r f; do rm -f "$D/$f"; done
    N=$(ls "$D" | wc -l)
  fi
  BYTES=$(du -sb "$D" | cut -f1)
  echo "$((TALLA/1024)) KiB · $N ficheros · $BYTES bytes"
done
echo

for v in $(seq 1 "$VUELTAS"); do
  for TALLA in 2048 16384 131072 1048576 8388608; do
    D="$W/t$TALLA"
    N=$(ls "$D" | wc -l)
    rm -f "$W/o.vc"
    bench/mide.sh -e "F5 coste por talla: $((TALLA/1024)) KiB (vuelta $v)" \
      -m "talla$((TALLA/1024))k" -c "$N ficheros de $TALLA B" \
      -n "mismos bytes en todos los arboles; separa el coste POR FICHERO del coste POR BYTE" \
      -- "$VC" compress "$D" -o "$W/o.vc" --level 9 || { echo "!! fallo"; break 2; }
  done
done
rm -rf "$W"

echo
echo "=== LA RECTA: cuanto cuesta un fichero por existir ==="
python3 - <<'PY'
import json, collections
G = collections.defaultdict(list)
for l in open("bench/historia.jsonl"):
    f = json.loads(l)
    if f["etiqueta"].startswith("F5 coste por talla") and f["rc"] == 0:
        G[f["muestra"]].append(f)
if not G:
    print("  sin filas F5"); raise SystemExit

pts = []
print(f"  {'talla':>10} {'ficheros':>9} {'MiB':>8} {'seg':>8} {'Gciclos':>9} {'us/fich':>9} {'MB/s':>8}")
for m, rs in sorted(G.items(), key=lambda kv: kv[1][0]["bytes_in"] / max(len(kv[1]), 1)):
    n = int(rs[0]["config"].split()[0])
    seg = sum(r["seg"] for r in rs) / len(rs)
    cic = sum(r["ciclos"] for r in rs) / len(rs)
    by = sum(r["bytes_in"] for r in rs) / len(rs)
    pts.append((n, by, seg, cic))
    print(f"  {m:>10} {n:9d} {by/2**20:8.1f} {seg:8.2f} {cic/1e9:9.1f} "
          f"{seg*1e6/n:9.1f} {by/2**20/seg:8.1f}")

# minimos cuadrados sobre  seg = a*n + b*bytes
import statistics
if len(pts) >= 2:
    sn = sum(p[0]*p[0] for p in pts); sb = sum(p[1]*p[1] for p in pts)
    snb = sum(p[0]*p[1] for p in pts)
    sy_n = sum(p[2]*p[0] for p in pts); sy_b = sum(p[2]*p[1] for p in pts)
    det = sn*sb - snb*snb
    if det:
        a = (sy_n*sb - sy_b*snb) / det
        b = (sy_b*sn - sy_n*snb) / det
        print()
        print(f"  tiempo = {a*1e6:.1f} us por fichero  +  {b*1e9:.3f} ns por byte")
        print(f"  o sea: {b*2**20*1e3:.1f} ms por MiB de contenido")
        print()
        # el arbol real: 43.298 ficheros, 2.620.837.053 bytes
        tf, tb = 43298 * a, 2620837053 * b
        print(f"  Sobre el ARBOL REAL (43.298 ficheros, 2,6 GB) eso predice:")
        print(f"    {tf:7.1f} s de coste POR FICHERO   ({100*tf/(tf+tb):4.1f}%)")
        print(f"    {tb:7.1f} s de coste POR BYTE      ({100*tb/(tf+tb):4.1f}%)")
        print(f"    {tf+tb:7.1f} s en total, contra los ~306 s medidos de verdad")
PY
echo FIN-TALLA
