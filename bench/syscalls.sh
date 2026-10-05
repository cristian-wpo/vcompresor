#!/usr/bin/env bash
# CUANTAS VECES LE PREGUNTAMOS AL NUCLEO LO MISMO.
#
# La norma del dueño: que el resultado sea correcto no dice nada de si el camino lo es. El
# archivo que sale de aqui pasa la ida y vuelta byte a byte. La pregunta es otra: cuanto se
# gasta de mas para llegar a el.
#
# Leyendo comprimir_pleno, cada fichero recibe SEIS stat que devuelven la misma estructura:
# recorrer(), PROG_TOTAL_BYTES, PROG_BYTES, `let size`, meta_de() y duenos_de(). Las dos ultimas
# estan escritas una debajo de otra para leer dos mitades del mismo struct. Y hasta tres open:
# agrupar lee 1 MiB y tira los bytes, firma_de abre para leer 8 bytes, y fs::read lo abre otra
# vez entero. Esto NO es una lectura del codigo: se cuenta con strace, que no opina.
#
# POR QUE strace Y NO UN CRONOMETRO. Los syscalls son un CONTEO: no dependen de la carga, no hay
# que repetirlos tres veces y no hay suelo de ruido que discutir. strace hace la ejecucion mucho
# mas lenta y da igual, porque aqui no se mide tiempo. Es la medida mas dura que hay para esta
# pregunta, y por eso va antes que cualquier reloj.
#
# ── DOS FALLOS QUE TUVO LA PRIMERA VERSION, Y LOS DOS DABAN NUMEROS CREIBLES ──────────────────
#
# 1. LEIA LA COLUMNA EQUIVOCADA. La salida de `strace -c` es
#        % time  seconds  usecs/call  calls  errors  syscall
#    y la columna «errors» VA VACIA cuando no hay errores. El awk cogia el campo justo antes del
#    nombre, asi que leia «calls» en las filas sin errores y «errors» en las que si los tienen.
#    Publico «openat: 1 llamada» —que en realidad era 1 ERROR— junto a 3,74 close por fichero,
#    una combinacion imposible que aun asi tenia toda la pinta de un dato. Ahora se parsea por
#    NOMBRE DE COLUMNA, leyendo la cabecera.
#
# 2. COMPARABA UNA CONFIGURACION CONSIGO MISMA. Enfrentaba «--group» contra «por defecto», y el
#    defecto de agrupar acababa de volver a ser encendido: las dos ramas eran la misma. Salieron
#    cifras identicas al syscall, que es justo lo que uno esperaria de una medida buena.
#    Ahora es --group contra --no-group, explicitas las dos, y se COMPRUEBA que difieren.
set -uo pipefail
cd "$(dirname "$0")/.."

VC=$PWD/target/release/vcompresor
command -v strace >/dev/null || { echo "no hay strace"; exit 3; }
W=$(mktemp -d /tmp/sys.XXXXXX)
SAL=bench/syscalls.tsv

{
  echo "# CUANTAS VECES SE TOCA CADA FICHERO. Contado con strace -c -f, que no opina."
  echo "# Se parsea por NOMBRE DE COLUMNA: la columna «errors» de strace va vacia cuando no hay"
  echo "# errores, y leer el campo por posicion mezcla llamadas con errores. Paso."
  echo "# Lo escribe bench/syscalls.sh."
  printf "muestra\tficheros\tconfig\tllamada\tllamadas\tpor_fichero\terrores\n"
} > "$SAL"

lee() {   # $1=fichero de strace  $2=muestra  $3=n  $4=config
  python3 - "$@" <<'PY'
import sys, re
f, muestra, n, cfg = sys.argv[1], sys.argv[2], int(sys.argv[3]), sys.argv[4]
INTERESA = {"openat", "statx", "newfstatat", "read", "pread64", "close", "mmap", "munmap",
            "lseek", "getdents64"}
filas, cab = [], None
for l in open(f):
    if "syscall" in l and "calls" in l:
        cab = l; continue
    if cab is None or l.startswith("-") or l.startswith("total"):
        continue
    p = l.split()
    if len(p) < 5:
        continue
    nom = p[-1]
    if nom not in INTERESA:
        continue
    # sin columna de errores: %time seconds usecs calls syscall  -> 5 campos
    # con columna de errores: %time seconds usecs calls errors syscall -> 6 campos
    llam = int(p[3]); err = int(p[4]) if len(p) >= 6 else 0
    filas.append((nom, llam, err))
filas.sort(key=lambda r: -r[1])
sal = open("bench/syscalls.tsv", "a")
print(f"  --- {cfg} ---")
for nom, llam, err in filas:
    print(f"  {nom:<12} {llam:10d}  {llam/n:7.2f} por fichero" + (f"  ({err} errores)" if err else ""))
    sal.write(f"{muestra}\t{n}\t{cfg}\t{nom}\t{llam}\t{llam/n:.2f}\t{err}\n")
PY
}

# Dos muestras que ejercitan caminos distintos: codigo son ficheros pequeños por el camino
# normal; png pasa ademas por firma_de y por el carril preflate.
for M in codigo png; do
  IN=/tmp/compresor-f1/corpus/$M
  [ -d "$IN" ] || { echo "salto $M (no esta)"; continue; }
  N=$(find "$IN" -type f | wc -l)
  echo "=== $M · $N ficheros ==="
  declare -A BYTES=()
  for C in "--group" "--no-group"; do
    rm -f "$W/o.vc"
    strace -f -c -o "$W/s.txt" "$VC" compress "$IN" -o "$W/o.vc" --level 9 $C >/dev/null 2>&1
    [ -f "$W/o.vc" ] || { echo "  !! no salio archivo con «$C» — medida invalida"; continue; }
    BYTES[$C]=$(stat -c%s "$W/o.vc")
    lee "$W/s.txt" "$M" "$N" "$C"
  done
  # Si las dos ramas sacan los mismos bytes, no son dos ramas: la bandera no hizo nada y la
  # comparacion se esta midiendo a si misma. Paso en la primera version y no fallo nada.
  if [ "${BYTES[--group]:-x}" = "${BYTES[--no-group]:-y}" ]; then
    echo "  !! --group y --no-group sacan LOS MISMOS BYTES: la bandera no hizo nada. NO publicar."
  fi
  echo
done
rm -rf "$W"
echo "escrito $SAL"
echo FIN-SYSCALLS
