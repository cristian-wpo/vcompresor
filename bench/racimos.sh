#!/usr/bin/env bash
# ¿SOBRA EL RACIMO DE MODELOS, O BASTA CON UNO?
#
# No es retorica. Los 36 modelos de tabla son la mayor parte de la memoria y del reloj del codec.
# Si tres de ellos hacen el 95% del trabajo, hay una version pequeña del compresor esperando a que
# alguien la mida. Y si no la hay, tambien hace falta saberlo: es la respuesta que JUSTIFICA el
# gasto, y no estaba contestada porque este guion llevaba declarado en la tabla de pruebas desde
# siempre y NUNCA SE ESCRIBIO.
#
# COMO SE PREGUNTA. `VC_PERFIL=1` quita la poda de serie (entran los 36) y `VC_APAGAMASK` apaga los
# que se le digan. La mascara solo SUMA apagados, nunca los devuelve, asi que «quedarse con N» se
# escribe como «apagar los otros 36-N».
#
# ⚠️ ESTOS ARCHIVOS NO SE PUEDEN DESCOMPRIMIR, y es a proposito: la ablacion escribe el modelo 254
# (MODELO_ABLACIONADO) para que un artefacto de medida no pueda confundirse con un respaldo.
#
# Tipo BYTES: determinista. Los mismos datos dan los mismos bytes con la maquina vacia o llena.
set -uo pipefail
cd "$(dirname "$0")/.."
. bench/comun.sh

MUESTRA=${MUESTRA:-512000}
CORPUS=${CORPUS:-/tmp/corpus-hilos}
TIPOS=${TIPOS:-"codigo csvjson"}
VC=./target/release/vcompresor
TMP=$(mktemp -d /tmp/racimos.XXXXXX)
trap 'rm -rf "$TMP"' EXIT

cab "RACIMOS · ¿sobra el racimo de modelos, o basta con uno?"
[ -x "$VC" ] || { echo "  falta $VC (cargo build --release)"; exit 2; }

# nombre | los indices de NOMBRES que SE QUEDAN  ('todos' / 'defecto' son especiales)
cat > "$TMP/grupos" <<'EOF'
los 36 (sin poda)|todos
por defecto (sin duplicados)|defecto
ordenes 0..5 + palabra|0 1 2 3 4 5 6
ordenes 0..3|0 1 2 3
ordenes 0..1|0 1
solo orden-3|3
solo orden-0|0
EOF

# mascara de los que se APAGAN = complemento de los que se quedan, dentro de 0..35
mascara() {
  local keep=" $1 " m=0 i
  for i in $(seq 0 35); do
    case "$keep" in *" $i "*) ;; *) m=$(( m | (1 << i) )) ;; esac
  done
  echo "$m"
}

comprime() {   # $1 = entorno extra   $2 = entrada  -> bytes por stdout
  local o="$TMP/o.vc"; rm -f "$o"
  # shellcheck disable=SC2086
  env $1 "$VC" compress "$2" -o "$o" --level 9 --max >/dev/null 2>&1
  talla "$o"
}

rc=0
for t in $TIPOS; do
  [ -d "$CORPUS/$t" ] || { echo "  falta $CORPUS/$t — se salta"; rc=1; continue; }
  muestra_repartida "$CORPUS/$t" "$MUESTRA" "$TMP/$t.bin" || { rc=1; continue; }
  echo "  $t · muestra de $(talla "$TMP/$t.bin") bytes [${MUESTRA_SELLO:-?}]"
  printf "    %-30s %10s %9s  %s\n" "racimo" "bytes" "vs 36" "modelos"

  base=$(comprime "VC_PERFIL=1 VC_APAGAMASK=0" "$TMP/$t.bin")
  peor=""
  while IFS='|' read -r nom keep; do
    [ -z "$nom" ] && continue
    case "$keep" in
      todos)   e="VC_PERFIL=1 VC_APAGAMASK=0"; n=36 ;;
      defecto) e="VC_PERFIL=0 VC_APAGAMASK=0"; n="los de serie" ;;
      *)       e="VC_PERFIL=1 VC_APAGAMASK=$(mascara "$keep")"; n=$(echo $keep | wc -w | tr -d ' ') ;;
    esac
    b=$(comprime "$e" "$TMP/$t.bin")
    if [ "${b:-0}" -gt 0 ] && [ "${base:-0}" -gt 0 ]; then
      pct=$(awk -v a="$b" -v c="$base" 'BEGIN{printf "%+.2f%%", 100*(a-c)/c}')
    else pct="?"; rc=1; fi
    printf "    %-30s %10s %9s  %s\n" "$nom" "${b:-0}" "$pct" "$n"
    peor=$b
  done < "$TMP/grupos"

  # ⚠️ LO QUE HACE QUE ESTO SEA UNA PRUEBA Y NO UN INFORME. Si «solo orden-0» pesa lo MISMO que
  # «los 36», la mascara no llego o la muestra es demasiado pequeña, y el resultado se leeria como
  # «el racimo no aporta nada» — que es exactamente el error de solid/no-solid del 01-08.
  distingue_o_aborta "$base" "$peor" "los 36 modelos y uno solo" || rc=1
  echo
done

echo "  LEER ASI: «vs 36» es lo que SE PIERDE al quedarse con menos. Si quedarse con cuatro cuesta"
echo "  menos del 1%, el racimo entero es caro y hay una version pequeña del codec esperando."
echo "FIN racimos rc=$rc"
exit $rc
