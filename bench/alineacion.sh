#!/usr/bin/env bash
# ¿VEN RUIDO LOS MODELOS EN DATO BIT-PACKED?
#
# Todo el codec razona en BYTES: los contextos son «el byte anterior», «los dos anteriores», «el
# byte a distancia 4». Pero hay formatos enteros —muestras de audio de 12 bits, indices de
# 12 bits, mapas de bits de sensores, cabeceras de protocolo— donde los campos NO empiezan en
# frontera de byte. Ahi el mismo valor cae cada vez en una posicion distinta dentro del byte, y
# un contexto de bytes ve ruido donde hay estructura perfecta.
#
# COMO SE PREGUNTA, con dato sintetico y la ENTROPIA CONTROLADA. Se generan los MISMOS valores
# dos veces:
#     A · empaquetados a 12 bits, sin frontera de byte     (bit-packed)
#     B · los mismos valores en 16 bits alineados          (con 4 bits de relleno a cero)
# B es MAS GRANDE en crudo —lleva relleno— asi que si aun asi comprime mejor que A, la diferencia
# no es del dato: es de que nuestros modelos solo saben mirar en frontera de byte.
#
# ⚠️ Y VA CON TESTIGOS. Se comprime lo mismo con xz y zstd si estan: si a ellos les pasa igual, el
# problema es de la familia entera de compresores de contexto por bytes y no nuestro; si a
# nosotros nos pasa MAS, es nuestro. Sin el testigo el numero no se puede interpretar.
#
# Tipo BYTES: determinista (el generador va con semilla fija).
set -uo pipefail
cd "$(dirname "$0")/.."
. bench/comun.sh

N=${N:-400000}          # cuantos valores
VC=./target/release/vcompresor
TMP=$(mktemp -d /tmp/alineacion.XXXXXX)
trap 'rm -rf "$TMP"' EXIT

cab "ALINEACION · ¿ven ruido los modelos en dato bit-packed?"
[ -x "$VC" ] || { echo "  falta $VC (cargo build --release)"; exit 2; }

python3 - "$N" "$TMP" <<'PY'
import sys, random
n = int(sys.argv[1]); d = sys.argv[2]
# Una señal con estructura de verdad: paseo aleatorio suave, o sea muy predecible a partir del
# valor anterior. Semilla fija -> el mismo fichero en cualquier maquina y en cualquier ejecucion.
random.seed(20260802)
v, vals = 2048, []
for _ in range(n):
    v = max(0, min(4095, v + random.randint(-12, 12)))
    vals.append(v)

# A · 12 bits pegados, sin frontera de byte
acc = bits = 0
a = bytearray()
for x in vals:
    acc = (acc << 12) | x; bits += 12
    while bits >= 8:
        bits -= 8; a.append((acc >> bits) & 0xFF)
if bits: a.append((acc << (8 - bits)) & 0xFF)

# B · los mismos valores, 16 bits LE alineados (4 bits de relleno)
b = bytearray()
for x in vals:
    b.append(x & 0xFF); b.append((x >> 8) & 0xFF)

open(d + "/A.bin", "wb").write(bytes(a))
open(d + "/B.bin", "wb").write(bytes(b))
PY

ta=$(talla "$TMP/A.bin"); tb=$(talla "$TMP/B.bin")
echo "  $N valores de 12 bits · A empaquetado $ta B · B alineado a 16 bits $tb B (+$(( 100*(tb-ta)/ta ))% de relleno)"
echo

mide() {   # $1 = herramienta  $2 = fichero -> bytes
  case "$1" in
    vc)   local o="$TMP/o.vc"; rm -f "$o"
          "$VC" compress "$2" -o "$o" --level 9 --max >/dev/null 2>&1; talla "$o" ;;
    xz)   xz -9e -c "$2" 2>/dev/null | wc -c ;;
    zstd) zstd -19 -c "$2" 2>/dev/null | wc -c ;;
  esac
}

rc=0
printf "  %-8s %12s %12s %12s %s\n" "" "A packed" "B alineado" "B/A" "que dice"
for h in vc xz zstd; do
  command -v "$h" >/dev/null 2>&1 || [ "$h" = vc ] || { printf "  %-8s %s\n" "$h" "(no instalado)"; continue; }
  ca=$(mide "$h" "$TMP/A.bin"); cb=$(mide "$h" "$TMP/B.bin")
  [ "${ca:-0}" -gt 0 ] && [ "${cb:-0}" -gt 0 ] || { echo "  $h: sin dato"; rc=1; continue; }
  r=$(awk -v a="$ca" -v b="$cb" 'BEGIN{printf "%.3f", b/a}')
  dice=$(awk -v a="$ca" -v b="$cb" 'BEGIN{print (b<a) ? "el ALINEADO gana pese al relleno -> ciego al bit-packing" : "el packed gana: ve la estructura"}')
  printf "  %-8s %12s %12s %12s %s\n" "$h" "$ca" "$cb" "$r" "$dice"
done

# Un fichero de 12 bits empaquetado y el mismo alineado NO pueden dar lo mismo. Si dan lo mismo,
# el generador no ha escrito lo que dice o la muestra es ridicula.
distingue_o_aborta "$ta" "$tb" "el crudo empaquetado y el crudo alineado" || rc=1

echo
echo "  LEER ASI: B lleva 33% MAS bytes crudos (4 bits de relleno por valor) y contiene EXACTAMENTE"
echo "  la misma informacion. Si aun asi comprime mas pequeño, lo que sobra en A no es dato: es lo"
echo "  que nuestros modelos no saben leer por no estar alineado. Y la fila de xz/zstd dice si eso"
echo "  es un problema de la familia entera o nuestro."
echo "FIN alineacion rc=$rc"
exit $rc
