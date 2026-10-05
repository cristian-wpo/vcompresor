#!/usr/bin/env bash
# ¿CUANTOS DE LOS CINCO MATCH HACEN FALTA?
#
# Hay cinco modelos de coincidencia: dos por longitud —match-6 y match-12— y tres DISPERSOS, que
# saltan de 2, 4 y 8 bytes para enganchar registros de enteros. Cada uno cuesta su buffer, su
# busqueda y su ranura en el mezclador. La pregunta es si los cinco pagan.
#
# COMO SE PREGUNTA, en dos pasadas, y el orden importa:
#   1. MARGINAL — se apaga UNO y se mira cuanto sube el archivo. Contesta «¿que aporta este?».
#   2. ACUMULADA — se apagan por orden de menos aportacion. Contesta «¿donde deja de doler?»,
#      que NO es la suma de los marginales: dos modelos que ven lo mismo dan marginal ~0 cada uno
#      y quitar los dos duele. La primera pasada sola engaña, y por eso van las dos.
#
# ⚠️ Archivos NO descomprimibles a proposito (modelo 254). Artefactos de medida.
# Tipo BYTES: determinista.
set -uo pipefail
cd "$(dirname "$0")/.."
. bench/comun.sh

# ⚠️ 4 MiB, NO 512 KB. Los match existen para enganchar repeticiones LEJANAS, asi que medirlos en
# una muestra que cabe entera en la ventana los infravalora por construccion — es el mismo error
# que PODA_DESDE reconoce para los modelos de tabla (a partir de 8 MiB la poda cambia de signo).
# Con 512 KB los cinco juntos salian valiendo +0,4%; el numero no es falso, es de otra pregunta.
MUESTRA=${MUESTRA:-4194304}
CORPUS=${CORPUS:-/tmp/corpus-hilos}
# bin y sql son donde los dispersos tienen algo que hacer (registros de ancho fijo); codigo es el
# control: si tambien ahi aportan, es que enganchan otra cosa.
TIPOS=${TIPOS:-"bin sql codigo"}
VC=./target/release/vcompresor
TMP=$(mktemp -d /tmp/cuantos.XXXXXX)
trap 'rm -rf "$TMP"' EXIT

# indice en NOMBRES -> nombre. 39 (cadena ISSE) NO es un match y no entra.
IDX="37 38 40 41 42"
nom() { case "$1" in 37) echo "match-6";; 38) echo "match-12";; 40) echo "match-disp-2";;
                     41) echo "match-disp-4";; 42) echo "match-disp-8";; esac; }

cab "CUANTOS-MATCH · ¿cuantos de los cinco match hacen falta?"
[ -x "$VC" ] || { echo "  falta $VC (cargo build --release)"; exit 2; }

comprime() {   # $1 = mascara de apagados  $2 = entrada
  local o="$TMP/o.vc"; rm -f "$o"
  env VC_APAGAMASK="$1" "$VC" compress "$2" -o "$o" --level 9 --max >/dev/null 2>&1
  talla "$o"
}

rc=0
for t in $TIPOS; do
  [ -d "$CORPUS/$t" ] || { echo "  falta $CORPUS/$t — se salta"; rc=1; continue; }
  muestra_repartida "$CORPUS/$t" "$MUESTRA" "$TMP/$t.bin" || { rc=1; continue; }
  base=$(comprime 0 "$TMP/$t.bin")
  echo "  $t · muestra $(talla "$TMP/$t.bin") B [${MUESTRA_SELLO:-?}] · con los cinco: $base bytes"

  # ── 1) marginal: apagar uno ──────────────────────────────────────────────────────────────
  : > "$TMP/marg.$t"
  printf "    %-16s %10s %10s\n" "apagando solo" "bytes" "cuesta"
  for i in $IDX; do
    b=$(comprime $(( 1 << i )) "$TMP/$t.bin")
    d=$(( b - base ))
    printf "    %-16s %10s %+10s\n" "$(nom $i)" "$b" "$d"
    echo "$d $i" >> "$TMP/marg.$t"
  done

  # ── 2) acumulada, del que menos aporta hacia arriba ──────────────────────────────────────
  echo
  printf "    %-30s %10s %9s\n" "quitando, de menos util a mas" "bytes" "vs cinco"
  m=0
  quitados=""
  while read -r d i; do
    m=$(( m | (1 << i) ))
    quitados="$quitados $(nom $i)"
    b=$(comprime "$m" "$TMP/$t.bin")
    pct=$(awk -v a="$b" -v c="$base" 'BEGIN{printf "%+.3f%%", 100*(a-c)/c}')
    printf "    %-30s %10s %9s\n" "sin:$quitados" "$b" "$pct"
  done < <(sort -n "$TMP/marg.$t")

  # si quitar los cinco da lo mismo que tenerlos, la muestra no distingue nada
  todos=$(comprime $(( (1<<37)|(1<<38)|(1<<40)|(1<<41)|(1<<42) )) "$TMP/$t.bin")
  distingue_o_aborta "$base" "$todos" "tener los cinco match y no tener ninguno" || rc=1
  echo
done

echo "  LEER ASI: el marginal de un modelo puede ser ~0 y aun asi hacer falta — si otro ve lo"
echo "  mismo, quitar uno no duele y quitar los dos si. Por eso manda la columna ACUMULADA."
echo "FIN cuantos-match rc=$rc"
exit $rc
