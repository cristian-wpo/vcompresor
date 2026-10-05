#!/usr/bin/env bash
# F6.Q3 — las cuatro condiciones sobre el MISMO par de frames.
#
# Contesta el reparto del +1,43% de partir en frames: cuanto del hueco lo devuelve cebar las
# tablas (b), cuanto lo devuelve ademas la ventana de historia (c), y donde esta el techo (d).
# El metodo esta escrito en docs/f6-arranque-en-frio.md §3; esto es su ejecucion.
#
# POR QUE ESTE GUION EXISTE Y NO SE VUELVE A HACER A MANO. La primera tanda (8788e07) corrio F0 y
# (a) sueltos desde la terminal: (b), (c) y (d) no llegaron a correr, la fila de F0 no aparecio en
# el libro mayor -se lanzo sin -S, o sea sin decirle a la bascula cual era la salida- y la unica
# prueba de las dos que si corrieron quedo en /tmp. Un reparto que no se puede re-derivar no es un
# reparto.
#
# REANUDABLE A PROPOSITO. Cada condicion se salta si su salida YA existe con bytes: son ~13
# minutos de maquina cada una y un reintento no puede empezar borrando lo que la anterior midio.
# Para re-medir de verdad, se borra a mano la salida concreta.
#
# Tipo BYTES: determinista. Ningun umbral de ruido se aplica a bytes.
set -uo pipefail
cd "$(dirname "$0")/.."
R=$PWD
W=${W:-/tmp/f6q3}
S=${S:-32}
F=$W/frames-$S
VCCM=$R/target/release/vc-cm

# EL SELLO DEL BINARIO, COMPROBADO ANTES DE GASTAR 40 MINUTOS. (a) se midio con 48a87e59a285; una
# condicion medida con otro binario no se puede restar de (a), y el momento de enterarse es este y
# no cuando ya estan los numeros puestos en el documento.
ESPERADO=${ESPERADO:-48a87e59a285}
[ -x "$VCCM" ] || { echo "falta $VCCM (cargo build --release)"; exit 2; }
SELLO=$(sha256sum "$VCCM" | cut -c1-12)
[ "$SELLO" = "$ESPERADO" ] || { echo "sello $SELLO != $ESPERADO — recompila o ajusta ESPERADO"; exit 3; }

f0=$F/carril0-frame00000.bin
f1=$F/carril0-frame00001.bin
for x in "$f0" "$f1"; do [ -s "$x" ] || { echo "falta $x (vuelca frames con VC_BANCO_FRAMES)"; exit 2; }; done

TSV=$W/condiciones-$S.tsv
[ -f "$TSV" ] || printf 'cond\tsolid\tentrada_B\tsalida_B\tsalida\n' > "$TSV"

# El techo necesita los dos frames como UN bloque. Se arma una sola vez y se reusa.
f01=$W/f01.$S.bin
if [ ! -s "$f01" ]; then cat "$f0" "$f1" > "$f01"; fi

# La ETIQUETA se pasa entera y no se compone aqui: es la clave por la que bench/f6q3-reparto.py
# encuentra la fila en el libro mayor, asi que tiene que salir IDENTICA a la que ya esta escrita.
corre() { # corre <clave> <etiqueta exacta> <entrada> <config>
  local k=$1 eti=$2 ent=$3 cfg=$4 out=$W/o.$1.$S
  if [ -s "$out" ]; then echo "== $k ya medido ($(stat -c%s "$out") B) — se salta"; return 0; fi
  echo "== $k · $(date -u +%H:%M:%SZ) =="
  bench/mide.sh -e "F6.Q3 s$S $eti" -m "frame-s$S" -c "$cfg" -H vc-cm \
    -S "$out" -s "$W/err.$k.$S" -- "$VCCM" c "$ent" "$out"
  local rc=$?
  printf '%s\t%s\t%s\t%s\t%s\n' "$k" "$S" "$(stat -c%s "$ent")" \
    "$( [ -f "$out" ] && stat -c%s "$out" || echo FALLO )" "$out" >> "$TSV"
  return $rc
}

# LAS CINCO, y en este orden. F0 no es una condicion: es el sumando comun de (a), (b) y (c), y sin
# el no hay totales que comparar con (d). La primera tanda lo dejo fuera del libro mayor y por eso
# el reparto no se podia derivar aunque los ficheros estuvieran en /tmp.
# (b) y (c) marcan el archivo con el modelo 254 a proposito: el cebador no viaja dentro, asi que
# un .vccm cebado es ilegible por diseño y no se le pide ida y vuelta. (d) y (a) si son legibles.
                                   corre F0 "F0 solo"                "$f0"  "frio"
                                   corre a  "(a) frio"              "$f1"  "a=frio"
VC_CEBA=$f0                        corre b  "(b) tablas primadas"   "$f1"  "b=VC_CEBA"
VC_CEBA=$f0 VC_CEBA_VENTANA=1      corre c  "(c) primadas+historia" "$f1"  "c=VC_CEBA+VENTANA"
                                   corre d  "(d) techo un bloque"   "$f01" "d=solido"

# IDA Y VUELTA DEL TECHO. Es la unica condicion nueva que produce un archivo que debe poder leerse,
# y sin ella (d) es un tamaño sin garantia de que codifique la entrada.
if [ -s "$W/o.d.$S" ] && [ ! -f "$W/idayvuelta-d.$S.txt" ]; then
  echo "== ida y vuelta (d) · $(date -u +%H:%M:%SZ) =="
  "$VCCM" d "$W/o.d.$S" "$W/rt.d.$S.bin" >/dev/null 2>&1
  if cmp -s "$f01" "$W/rt.d.$S.bin"; then v="IDA Y VUELTA OK"; else v="ROUND-TRIP FAIL"; fi
  printf '%s  %s  %s\n' "$v" "$(sha256sum "$f01" | cut -c1-16)" \
    "$(sha256sum "$W/rt.d.$S.bin" 2>/dev/null | cut -c1-16)" | tee "$W/idayvuelta-d.$S.txt"
  rm -f "$W/rt.d.$S.bin"
fi

echo "== fin · $(date -u +%H:%M:%SZ) =="
cat "$TSV"
