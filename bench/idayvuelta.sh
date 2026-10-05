#!/usr/bin/env bash
# IDA Y VUELTA. La prueba que tenia que existir y no existia.
#
# POR QUE EXISTE, y el motivo es de hoy: el 01-08 se cambiaron DOS defectos del codec —VC_VISTAS
# de 3 a 0 y la forma de LIM— y ninguno de los dos tenia una prueba que dijera si el archivo
# seguia restaurandose. La suite tenia una entrada llamada `compat`, que por el nombre parecia
# esto, y al abrirla resulto ser el test de UNA migracion concreta (F264, «esqueleto crudo» ->
# `esq: Vec<u64>`): depende de tres ficheros .bak en /tmp y de una forma de codigo que ya no
# existe, asi que no puede pasar nunca. Un nombre generico sobre una prueba de un solo uso.
#
# Aqui no se compara contra nada guardado: se comprime, se extrae y se compara CON EL ORIGINAL.
# Eso es lo unico que hay que garantizar siempre, y no caduca cuando cambian los bytes de salida
# —que es justo lo que pasa al tocar LIM o cualquier constante de clase DETERMINISMO—.
#
#   bench/idayvuelta.sh [directorio-corpus]
#
# ⚠️ Es de BYTES: no mira el reloj, asi que puede ir en paralelo con otras de bytes.
set -uo pipefail
cd "$(dirname "$0")/.."
BIN=./target/release/vcompresor
[ -x "$BIN" ] || { echo "falta $BIN"; exit 2; }

CORPUS=${1:-/tmp/corpus-hilos}
MB=${VC_IV_MB:-3}          # cuanto de cada tipo. 3 MB basta para cruzar todos los carriles
TIPOS=${VC_IV_TIPOS:-codigo csvjson logs sql bin pdfoffice png jpg video dup}

echo
echo "  IDA Y VUELTA · $MB MB de cada tipo · defectos del producto (entorno LIMPIO)"
echo "  comprime -> extrae -> compara con el original, byte a byte"
echo

T=$(mktemp -d /tmp/iv.XXXXXX)
trap 'rm -rf "$T"' EXIT
mal=0; bien=0; sin=0

for t in $TIPOS; do
  d="$CORPUS/$t"
  [ -d "$d" ] || { printf "  %-12s (no esta en el corpus)\n" "$t"; sin=$((sin+1)); continue; }
  # una rodaja de dato REAL: ficheros enteros hasta llegar al tamano, no un corte a medias
  python3 - "$d" "$T/$t.bin" "$MB" <<'PY'
import glob, os, sys
d, out, mb = sys.argv[1], sys.argv[2], int(sys.argv[3])
fs = sorted(p for p in glob.glob(f"{d}/**/*", recursive=True) if os.path.isfile(p))
o = bytearray()
for p in fs:
    o += open(p, "rb").read()
    if len(o) >= mb * 1048576: break
open(out, "wb").write(bytes(o[: mb * 1048576]))
PY
  [ -s "$T/$t.bin" ] || { printf "  %-12s (vacio)\n" "$t"; sin=$((sin+1)); continue; }

  for modo in "rapido:" "maximo:--max"; do
    nom=${modo%%:*}; flag=${modo##*:}
    o="$T/$t.$nom.vc"; r="$T/rt.$t.$nom"
    rm -rf "$r"; mkdir -p "$r"
    if ! $BIN compress "$T/$t.bin" -o "$o" --level 9 $flag >/dev/null 2>&1; then
      printf "  %-12s %-7s ⚠️ NO COMPRIME\n" "$t" "$nom"; mal=$((mal+1)); continue
    fi
    if ! $BIN extract "$o" -o "$r" >/dev/null 2>&1; then
      printf "  %-12s %-7s ⚠️ NO EXTRAE\n" "$t" "$nom"; mal=$((mal+1)); continue
    fi
    if cmp -s "$T/$t.bin" "$r/$t.bin"; then
      e=$(stat -c%s "$T/$t.bin" 2>/dev/null || stat -f%z "$T/$t.bin")
      s=$(stat -c%s "$o" 2>/dev/null || stat -f%z "$o")
      printf "  %-12s %-7s OK   %10s -> %10s  (%.4fx)\n" "$t" "$nom" \
             "$(echo $e | sed ':a;s/\B[0-9]\{3\}\>/.&/;ta')" \
             "$(echo $s | sed ':a;s/\B[0-9]\{3\}\>/.&/;ta')" \
             "$(python3 -c "print($e/$s)")"
      bien=$((bien+1))
    else
      printf "  %-12s %-7s 🚨 NO COINCIDE CON EL ORIGINAL\n" "$t" "$nom"; mal=$((mal+1))
    fi
    rm -f "$o"; rm -rf "$r"
  done
done

echo
echo "  $bien bien · $mal mal · $sin sin dato"
# ⚠️ Una prueba de correccion tiene que devolver rc distinto de 0 cuando falla. Si no, la suite
# la cuenta como pasada y el fallo se queda en su log. Es lo que le pasaba a las seis que no
# existian: rc=127 en un fichero que nadie abre.
[ "$mal" -eq 0 ] || exit 1
echo "  FIN idayvuelta OK"
