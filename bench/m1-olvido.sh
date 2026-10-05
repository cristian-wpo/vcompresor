#!/usr/bin/env bash
# M1: ¿CUANTO CUESTA QUE EL MODELO APRENDA Y OLVIDE EN CADA TRAMO?
#
# vc_cm::comprimir se llama UNA VEZ POR TRAMO y hace Predictor::new cada vez. Con --max los
# tramos son de 32 MiB, asi que sobre un arbol de 2,6 GB el modelo aprende y OLVIDA ochenta
# veces. Un compresor de contextos mejora cuanto mas ha visto: esto no es solo tiempo, es
# RATIO TIRADO A LA BASURA. Es el desperdicio mas caro que queda sin medir en el proyecto.
#
# SE MIDE EN BYTES, asi que la carga de la maquina DA IGUAL. Lo que no da igual es la entrada.
#
# ── POR QUE LA PRIMERA PRUEBA NO VALIO, Y COMO SE EVITA AQUI ─────────────────────────────────
# El primer intento uso el mismo fichero copiado seis veces. El DEDUP lo colapso: 21 MiB unicos,
# que caben en UN tramo con cualquier --solid. Los tres tamaños dieron lo mismo y la lectura
# natural fue «--solid no llega a los carriles CM». No era eso: la entrada no daba para dos
# tramos.
#
# Aqui la entrada se construye de ficheros DISTINTOS, y ademas se COMPRUEBA: si los bytes
# unicos no llegan para llenar mas de un tramo, la medida se descarta en vez de publicarse.
#
#   --solid manda sobre todo (lib.rs:136, `if sol > 0 { return sol << 20; }`), asi que
#   comparar --solid=N contra --solid=GRANDE es comparar «N tramos» contra «uno solo».
set -uo pipefail
cd "$(dirname "$0")/.."
VC=$PWD/target/release/vcompresor
W=$(mktemp -d /tmp/m1.XXXXXX)
IN="$W/in"; mkdir -p "$IN"
OBJETIVO=$((192 * 1024 * 1024))

echo "carga: $(awk '{print $1}' /proc/loadavg) — da igual, esto se mide en BYTES"
echo "montando ~$((OBJETIVO/1024/1024)) MiB de ficheros DISTINTOS (nada repetido)..."
n=0; tot=0
while IFS= read -r f; do
  s=$(stat -c%s "$f" 2>/dev/null) || continue
  [ "$s" -lt 4096 ] && continue
  cp "$f" "$IN/f$(printf %05d $n)" 2>/dev/null || continue
  n=$((n+1)); tot=$((tot+s))
  [ "$tot" -ge "$OBJETIVO" ] && break
done < <(find /tmp/compresor-f1/corpus/codigo /tmp/compresor-f1/corpus/sql \
              /tmp/compresor-f1/corpus/logs -type f 2>/dev/null | sort -u)
echo "  $n ficheros distintos · $tot bytes ($((tot/1024/1024)) MiB)"
echo

printf "  %-14s %8s %14s %9s\n" "tramos de" "cuantos" "bytes" "vs 1 tramo"
declare -A R=()
for MiB in 32 64 192; do
  rm -f "$W/o.vc"
  T=$(( (tot + MiB*1024*1024 - 1) / (MiB*1024*1024) ))
  if ! $VC compress "$IN" -o "$W/o.vc" --max --solid=$MiB >/dev/null 2>&1; then
    echo "  !! fallo con --solid=$MiB"; continue
  fi
  R[$MiB]=$(stat -c%s "$W/o.vc")
  printf "  %-14s %8d %14d" "${MiB} MiB" "$T" "${R[$MiB]}"
  if [ -n "${R[192]:-}" ] && [ "$MiB" != "192" ]; then
    printf " %+8.4f%%" "$(echo "scale=6; 100*(${R[$MiB]}-${R[192]})/${R[192]}" | bc)"
  fi
  echo
done

echo
# LA COMPROBACION QUE FALTO LA PRIMERA VEZ: si los tres dan lo MISMO, la entrada no daba para
# mas de un tramo y no se ha medido nada.
if [ "${R[32]:-0}" = "${R[192]:-1}" ]; then
  echo "  !! LOS TRES DAN LOS MISMOS BYTES."
  echo "     La entrada no llena mas de un tramo —seguramente el dedup la colapso— asi que"
  echo "     esto NO mide el reinicio del modelo. Es justo el fallo de la primera prueba."
  echo "     NO PUBLICAR. Hace falta mas contenido unico."
else
  python3 - "${R[32]:-0}" "${R[64]:-0}" "${R[192]:-0}" "$tot" <<'PY'
import sys
a, b, c, tot = (int(x) for x in sys.argv[1:5])
if not (a and c):
    raise SystemExit
print(f"  Partir en 7 tramos en vez de 2 cuesta {100*(a-c)/c:+.4f}% de tamaño.")
print(f"  Partir en 4 tramos en vez de 2 cuesta {100*(b-c)/c:+.4f}%." if b else "")
print()
print(f"  EXTRAPOLAR CON CUIDADO: sobre {tot/2**20:.0f} MiB son 6 tramos. El arbol real de")
print( "  2,6 GB con tramos de 32 MiB son ~80. El coste NO es lineal en el numero de tramos")
print( "  —cada reinicio pierde lo aprendido hasta ahi, y lo aprendido crece con el tramo—")
print( "  asi que esto da el SIGNO y el orden de magnitud, no la cifra del arbol entero.")
PY
fi
rm -rf "$W"
echo FIN-M1
