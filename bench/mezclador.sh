#!/usr/bin/env bash
# LA REJILLA DE LAS PERILLAS DEL MEZCLADOR
#
# El mezclador tiene cinco perillas y ninguna esta medida en el sitio donde vive hoy: el paso de
# aprendizaje (VC_MIX_LR), el decaimiento de pesos (VC_DECAY), el paso de la cadena ISSE
# (VC_ISSE_LR), cuantas etapas de calibracion se usan (VC_SSE) y si entra la parte no lineal
# (VC_DISP). Sus valores por defecto estan en la tabla de constantes como «A OJO».
#
# BARRIDO POR COORDENADAS, no rejilla completa: se mueve UNA perilla dejando las demas en su
# defecto. 3^5 = 243 compresiones no caben en una suite, y ademas la rejilla completa contesta una
# pregunta que no tenemos («¿interactuan?») antes que la que si («¿esta cada una donde toca?»).
#
# ⚠️ Y LO QUE ESTE GUION HACE Y OTROS NO: AVISA SI EL GANADOR CAE EN EL BORDE DEL BARRIDO. Es el
# error exacto de cdc-tallas el 01-08 —solo probaba tallas mas pequeñas que la vigente, asi que
# solo podia decir «mas abajo es mejor»— y el del DMC, cuyo ganador estaba en el extremo hasta que
# se extendio y la curva se dio la vuelta. Un ganador en el borde NO es un resultado: es un
# barrido que se quedo corto.
#
# ⚠️ DOS TIPOS COMO MINIMO. `PESO_LONG` estaba mal puesta por barrer sobre un corpus sin sql.
#
# Tipo BYTES: determinista.
set -uo pipefail
cd "$(dirname "$0")/.."
. bench/comun.sh

# ⚠️⚠️ DOS TAMAÑOS, Y ESTA ES LA LECCION MAS CARA DEL 02-08.
#
# Este barrido, a 512 KB, dijo que VC_ISSE_LR=20 ganaba −0,244% en codigo y −0,336% en sql, con la
# curva dandose la vuelta en 20-24 —o sea, con todas las señales de un resultado bueno—. Se midio
# en serio, a 8 MiB y en los seis tipos, y sale AL REVES: +0,186% en codigo, +0,443% en logs,
# +0,028% en total. PIERDE en cinco de seis.
#
# No es ruido ni un error: es que la tasa de aprendizaje de la cadena ISSE DEPENDE DEL TAMAÑO. Con
# poco dato, adaptarse rapido compensa; con mucho, el paso largo mete ruido sobre estadistica que
# ya estaba bien estimada. Igual que `PODA_DESDE` reconoce que la poda cambia de signo a los
# 8 MiB.
#
# Y fue la TERCERA vez el mismo dia que el tamaño de la muestra decidia la respuesta: `cuantos`
# infravaloraba los match a la mitad a 512 KB, y `familia` decia «estable» con tres tipos y «no»
# con diez. Asi que el barrido deja de dar UN numero: da dos, y AVISA cuando el ganador cambia
# entre ellos. Un cambio de signo entre tamaños es el resultado — mas que cualquiera de los dos.
MUESTRA=${MUESTRA:-524288}
MUESTRA2=${MUESTRA2:-4194304}
CORPUS=${CORPUS:-/tmp/corpus-hilos}
TIPOS=${TIPOS:-"codigo sql"}
VC=./target/release/vcompresor
TMP=$(mktemp -d /tmp/mezclador.XXXXXX)
trap 'rm -rf "$TMP"' EXIT

cab "MEZCLADOR · la rejilla de las perillas, y si alguna gana en el borde"
[ -x "$VC" ] || { echo "  falta $VC (cargo build --release)"; exit 2; }

# perilla | defecto | valores (el defecto DENTRO y con vecinos a los DOS lados) | que borde es TOPE
#
# La ultima columna evita un falso aviso: hay bordes que no se pueden extender porque son el
# limite del parametro, no el final de mi paciencia. VC_SSE va acotado a 1..5 en el codigo y
# VC_DECAY=0 significa «apagado», no «menos que 1». Un ganador ahi no es un barrido corto.
# ⚠️ DOS CORRECCIONES DEL 03-08, Y LAS DOS SALIERON DE MEDIR, NO DE LEER:
#
#   VC_DISP FUERA: es una perilla INERTE desde la v7. Medido en 512 KiB de codigo —
#   v6 18.084 · v6+VC_DISP=0 18.066 · v7 18.066 · v7+VC_DISP=0 18.066— o sea `v7 == v6 +
#   VC_DISP=0` byte a byte: IDISP/IDISP2 entran en la mascara de apagados y la dispersion se
#   borra a cero. Se gastaban dos compresiones por muestra y por talla en una columna que no
#   podia informar de nada. Y el aviso de «empatan» NO saltaba, porque solo avisa cuando el
#   defecto declarado no esta entre los empatados — y aqui estaba. Un empate silencioso.
#
#   VC_SSE DESDE 1, NO DESDE 3: el 3 era EL BORDE del barrido, asi que lo unico que este
#   guion podia decir de VC_SSE era «mas abajo es mejor». Extendido a mano: en codigo a
#   512 KiB, SSE=1 17.988 · 2 **17.962** · 3 17.967 · 4 17.973 · 5 17.988. El optimo es 2 y
#   la curva SE DA LA VUELTA — la señal que valida un barrido, y que empezar en el borde
#   hacia invisible. Ahora 1 y 5 son topes reales del parametro (va acotado a 1..5), asi que
#   ganar ahi no es quedarse corto y no se avisa.
cat > "$TMP/perillas" <<'EOF'
VC_MIX_LR|12|8 10 11 12 13 14 16|alto
VC_DECAY|0|0 14 16 18|bajo
VC_ISSE_LR|6|3 4 6 8 12 16 20 24 32 48 64|no
VC_SSE|5|1 2 3 4 5|ambos
EOF

comprime() { local o="$TMP/o.vc"; rm -f "$o"
  # shellcheck disable=SC2086
  env $1 "$VC" compress "$2" -o "$o" --level 9 --max >/dev/null 2>&1; talla "$o"; }

rc=0
for t in $TIPOS; do
  [ -d "$CORPUS/$t" ] || { echo "  falta $CORPUS/$t — se salta"; rc=1; continue; }
  for TAM in "$MUESTRA" "$MUESTRA2"; do
  muestra_repartida "$CORPUS/$t" "$TAM" "$TMP/$t.bin" || { rc=1; continue; }
  base=$(comprime "" "$TMP/$t.bin")
  echo "  $t · muestra $(talla "$TMP/$t.bin") B [${MUESTRA_SELLO:-?}] · con los defectos: $base bytes"

  while IFS='|' read -r p def vals topes; do
    [ -z "$p" ] && continue
    linea=""; mejor=""; mejorb=0; primero=""; ultimo=""
    for v in $vals; do
      b=$(comprime "$p=$v" "$TMP/$t.bin")
      eval "B_${p}_$v=\$b"
      [ -z "$primero" ] && primero=$v; ultimo=$v
      d=$(awk -v a="$b" -v c="$base" 'BEGIN{printf "%+.3f", 100*(a-c)/c}')
      mark=$([ "$v" = "$def" ] && echo "*" || echo " ")
      linea="$linea $(printf '%s%s:%s' "$v" "$mark" "$d")"
      if [ -z "$mejor" ] || { [ "${b:-0}" -gt 0 ] && [ "$b" -lt "$mejorb" ]; }; then mejor=$v; mejorb=$b; fi
    done
    # ⚠️ EL DEFECTO DECLARADO NO ES EL DEFECTO. En la primera ejecucion puse «VC_MIX_LR|10» porque
    # lo decia el comentario del codigo, y el codigo hacia 12 desde la v3. La columna mentia y el
    # barrido salvo la medida solo porque compara contra el BINARIO TAL CUAL, no contra lo
    # declarado. Asi que ahora tambien se DEDUCE: el defecto de verdad es el valor cuyos bytes
    # coinciden con la linea base, y si no coincide con el declarado, se dice.
    aviso=""
    bajo_es_tope=$([ "$topes" = "bajo" ] || [ "$topes" = "ambos" ] && echo si || echo no)
    alto_es_tope=$([ "$topes" = "alto" ] || [ "$topes" = "ambos" ] && echo si || echo no)
    if [ "$mejor" = "$primero" ] && [ "$bajo_es_tope" = "no" ]; then
      aviso="  ⚠️ GANA EN EL BORDE BAJO ($mejor): el barrido se quedo corto, hay que bajar mas"; rc=1
    elif [ "$mejor" = "$ultimo" ] && [ "$alto_es_tope" = "no" ]; then
      aviso="  ⚠️ GANA EN EL BORDE ALTO ($mejor): el barrido se quedo corto, hay que subir mas"; rc=1
    fi
    printf "    %-12s %s\n" "$p" "$linea"
    printf "    %-12s mejor=%s (defecto %s)%s\n" "" "$mejor" "$def" "$aviso"
    # ⚠️ EL DIAGNOSTICO VA DETRAS DE SU PROPIA PERILLA. Iba delante, y en el log un aviso
    # aparecia encima del bloque que describe — o sea colgando del anterior: el empate de
    # VC_DECAY (0 y 18) salia debajo de VC_MIX_LR y se leia como suyo.
    # ⚠️ Y SE COGEN TODOS LOS QUE EMPATAN, NO EL PRIMERO. La primera version se quedaba con el
    # primer valor cuyos bytes coincidian con la linea base y anunciaba «el defecto declarado es 5
    # pero el binario hace 3». Era MENTIRA y el propio barrido tenia la explicacion al lado: en
    # codigo, VC_SSE=3 y VC_SSE=5 dan EL MISMO ARCHIVO, porque las etapas 4 y 5 de calibracion
    # aprenden peso cero ahi y dejan de tocar nada. Dos valores que empatan no dicen cual es el
    # defecto; solo dicen que en esta muestra dan igual. En sql, donde 3 cuesta +0,331%, la
    # deteccion acertaba. Asi que se avisa solo si el declarado NO esta entre los que empatan.
    iguales=""
    for v in $vals; do
      eval "bv=\$B_${p}_$v"
      [ "$bv" = "$base" ] && iguales="$iguales $v"
    done
    case " $iguales " in
      *" $def "*) ;;
      "  ") printf "    %-12s ⚠️ ningun valor del barrido reproduce la linea base: el defecto esta FUERA del barrido\n" ""; rc=1 ;;
      *) printf "    %-12s ⚠️ EL DEFECTO DECLARADO ES %s Y EL BINARIO NO LO REPRODUCE (empatan:%s)\n" "" "$def" "$iguales"; rc=1 ;;
    esac
    [ "$(echo $iguales | wc -w | tr -d ' ')" -gt 1 ] && \
      printf "    %-12s (empatan en bytes:%s — en esta muestra esa perilla no distingue)\n" "" "$iguales"
    # el ganador de esta perilla EN ESTE TAMAÑO, para poder comparar tamaños al final
    eval "G_${p}_${TAM}=\$mejor"
  done < "$TMP/perillas"
  echo
  done   # TAM

  # ⚠️ EL AVISO QUE IMPORTA: ¿cambia el ganador al cambiar de tamaño?
  while IFS='|' read -r p def vals topes; do
    [ -z "$p" ] && continue
    eval "g1=\$G_${p}_${MUESTRA}"; eval "g2=\$G_${p}_${MUESTRA2}"
    [ -z "${g1:-}" ] || [ -z "${g2:-}" ] && continue
    if [ "$g1" != "$g2" ]; then
      printf "  ⚠️ %s: gana %s con %s B y %s con %s B — DEPENDE DEL TAMAÑO, no hay un valor bueno\n" \
             "$p" "$g1" "$MUESTRA" "$g2" "$MUESTRA2"
      rc=1
    fi
  done < "$TMP/perillas"
  echo
done

echo "  LEER ASI: cada celda es «valor:% frente al defecto». El * marca el valor de serie. Negativo"
echo "  = comprime mejor. Un ganador en el EXTREMO de la lista no vale: significa que la curva"
echo "  seguia bajando cuando se acabo el barrido, y el guion devuelve rc distinto de 0 por eso."
echo "FIN mezclador rc=$rc"
exit $rc
