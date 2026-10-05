#!/usr/bin/env bash
# EL CONTROL DEL REPLAY. Sin esto, el replay es una afirmacion; con esto, es una propiedad.
#
# Lo que se afirma es fuerte: que ejecutar la cola del predictor sobre predicciones volcadas da
# EXACTAMENTE los mismos bytes que comprimir de verdad. El argumento esta escrito en `mezcla()` y
# es correcto —ningun submodelo ve `pr`, asi que `st[]` no depende de la cola—, pero un argumento
# correcto sobre un codigo equivocado sigue dando numeros falsos. Ya nos ha pasado dos veces: el
# «oraculo» que resulto ser un bug, y el atajo que elegia configuraciones que nunca se corrieron.
# Los dos eran razonamientos buenos sin control.
#
# TRES PRUEBAS, y las tres tienen que pasar:
#
#   A) EXACTITUD. Comprimir de verdad y replayar la configuracion por defecto tienen que dar el
#      MISMO numero de bytes. Se compara contra el archivo menos los 10 bytes de cabecera (8 de
#      longitud + modelo + familia/perfil), que es lo que sale del codificador.
#
#   B) INDEPENDENCIA DEL REPARTO. El mismo barrido con 1 hilo y con 8 tiene que dar filas
#      identicas. Si no, `reinicia_cola` no esta dejando la cola como la deja el constructor y la
#      primera configuracion de cada lote mide distinto que las demas — un sesgo que no se ve
#      mirando una tabla, porque todas las filas parecen razonables.
#
#   C) EL DEFECTO SE REPRODUCE. La fila del barrido que lleva la configuracion por defecto tiene
#      que coincidir con (A). Es la que ata las dos mitades: sin ella, el replay podria ser
#      autoconsistente y estar desplazado respecto al compresor.
#
# ⚠️ TIPO BYTES: determinista, se puede correr con la maquina llena.
set -uo pipefail
cd "$(dirname "$0")/.."
BIN=${BIN:-./target/release}
MUESTRA=${MUESTRA:-/tmp/compresor-f1/samples/codigo.10m}
TAM=${TAM:-65536}
W=$(mktemp -d /tmp/replayctl.XXXXXX)
trap 'rm -rf "$W"' EXIT
rc=0

[ -x "$BIN/vc-cm" ] || { echo "falta $BIN/vc-cm"; exit 3; }
[ -x "$BIN/vc-replay" ] || { echo "falta $BIN/vc-replay (cargo build --release)"; exit 3; }
[ -f "$MUESTRA" ] || { echo "falta la muestra $MUESTRA"; exit 3; }

head -c "$TAM" "$MUESTRA" > "$W/in.bin"
echo "CONTROL DEL REPLAY · $TAM B de $(basename "$MUESTRA")"

# ── A) exactitud ───────────────────────────────────────────────────────────────────────────────
real=$(VC_VOLCAR="$W/v.st" "$BIN/vc-cm" c "$W/in.bin" "$W/out.vc" 2>&1 | grep -o 'out=[0-9]*' | cut -d= -f2)
[ -n "$real" ] || { echo "  A) FALLO: la compresion no dijo out="; exit 1; }
codec=$((real - 10))    # el archivo lleva 8 B de longitud + 1 de modelo + 1 de familia/perfil
rep=$("$BIN/vc-replay" "$W/v.st" 2>/dev/null | grep -o 'bytes=[0-9]*' | cut -d= -f2)
if [ "$rep" = "$codec" ]; then
  echo "  A) EXACTITUD ✅  real $real B (codec $codec) = replay $rep B"
else
  echo "  A) EXACTITUD ❌  real $real B (codec $codec) != replay $rep B"
  echo "     El replay NO reproduce el compresor. No uses sus numeros para decidir nada."
  rc=1
fi

# ── B) independencia del reparto ───────────────────────────────────────────────────────────────
"$BIN/vc-replay" "$W/v.st" --barrido --jobs 1 2>/dev/null > "$W/j1.tsv"
"$BIN/vc-replay" "$W/v.st" --barrido --jobs 8 2>/dev/null > "$W/j8.tsv"
if cmp -s "$W/j1.tsv" "$W/j8.tsv"; then
  echo "  B) REPARTO   ✅  1 hilo y 8 hilos dan las mismas $(grep -c '^[0-9]' "$W/j1.tsv") filas"
else
  echo "  B) REPARTO   ❌  el resultado DEPENDE de como se repartan las configuraciones:"
  diff "$W/j1.tsv" "$W/j8.tsv" | head -6
  echo "     `reinicia_cola` no deja la cola como la deja el constructor."
  rc=1
fi

# ── C) el defecto del barrido coincide con (A) ─────────────────────────────────────────────────
# El defecto de VC_MIX_LR sale de `tasas_de_modelo`, asi que se DEDUCE de la tabla en vez de
# teclearlo: teclearlo es exactamente como mezclador.sh acabo declarando 10 cuando el binario
# hacia 12, y esa mentira costo leer un +5% como si fuera del barrido.
fila=$(awk -v b="$codec" '$4==b {print $1"/"$2"/"$3}' "$W/j8.tsv" | head -3 | tr '\n' ' ')
if [ -n "$fila" ]; then
  echo "  C) DEFECTO   ✅  el barrido reproduce los $codec B del compresor en: $fila"
else
  echo "  C) DEFECTO   ❌  NINGUNA fila del barrido da los $codec B de la corrida real."
  echo "     El replay puede ser autoconsistente y estar desplazado respecto al compresor."
  rc=1
fi

echo
[ $rc -eq 0 ] && echo "TODO EN VERDE — el replay se puede usar para decidir." \
              || echo "🚨 HAY ROJOS — no uses el replay hasta arreglarlos."
exit $rc
