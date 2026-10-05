#!/usr/bin/env bash
# EL LANZADOR DE PRUEBAS. Una puerta de entrada, un juego por defecto, y varias a la carta.
#
#   bench/pruebas.sh                    el juego por defecto
#   bench/pruebas.sh racimos cebador    solo esas
#   bench/pruebas.sh --lista            que hay y de que tipo es cada una
#   bench/pruebas.sh --bytes            todas las de BYTES (van en paralelo)
#   bench/pruebas.sh --reloj            todas las de TIEMPO (van en serie)
#   bench/pruebas.sh --diseno           las que contestan preguntas de ARQUITECTURA
#
# LA REGLA QUE LO ORGANIZA TODO, y es la unica que hace falta entender:
#   BYTES  -> determinista. Los mismos datos dan los mismos bytes este la maquina vacia o a
#             carga 20. Van EN PARALELO, tantas como quepan.
#   RELOJ  -> va EN SERIE, y solo una por maquina. La misma prueba tarda 51,6 s o 97,1 s segun
#             la carga.
# Confundirlas serializa sin motivo -y eso cuesta horas- o falsea un reloj -y eso cuesta la
# conclusion-.
#
# POR QUE EXISTE ESTO. Cada prueba nueva reinventaba la fontaneria y volvia a pisar la misma
# trampa: `declare -A` que el bash de macOS no tiene, `stat` distinto en cada sistema, `pgrep`
# que se encuentra a si mismo, muestras demasiado pequeñas para distinguir lo que comparan, dos
# cadenas de reloj a la vez. Cinco en un dia, ninguna nueva. Ahora viven en comun.sh y se
# resuelven una vez.
set -uo pipefail
cd "$(dirname "$0")/.."
. bench/comun.sh

# nombre | tipo | grupo | que contesta | comando
# tipo:   B = bytes (paralela)   T = tiempo (serie)
# grupo:  D = defecto (entra en el juego base)   Q = pregunta de diseño (--diseno)
#
# POR QUE LAS SEIS NUEVAS NO SON «POR DEFECTO». No son regresiones: son PREGUNTAS. «¿sobra el
# racimo de modelos?» no se contesta distinto porque alguien toque un bucle, y correrlas en cada
# cambio seria pagar veinte minutos por una respuesta que ya se tiene. El juego por defecto
# comprueba que NO SE HA ROTO NADA; el grupo --diseno pregunta SI ESTA BIEN PENSADO. Son dos cosas
# y confundirlas hace que se deje de correr la primera.
# ⚠️ SEIS DE LAS DOCE APUNTABAN A GUIONES QUE NO EXISTEN, Y CUATRO ERAN «POR DEFECTO».
# ✅ ESCRITAS EL 02-08. Ya no falta ninguna; se quedan aqui las razones porque el fallo se puede
#    repetir en cuanto alguien añada una fila «para luego».
#
# Se vio el 01-08 al correr la suite por primera vez en mucho tiempo: racimos, cuantos, familia,
# cebador, mezclador y alineacion daban `rc=127` —«No such file or directory»— y la suite seguia
# tan tranquila. No las habia borrado nadie: NUNCA SE ESCRIBIERON. La tabla se redacto con el
# juego que se queria tener y esas seis se quedaron de intencion.
#
# Lo grave no es que falten: es que la suite PARECIA cobertura. El rc=127 iba a parar al log de
# cada prueba, que nadie abre si el lanzador no se queja, y el resumen contaba doce lanzadas. Una
# suite que no puede correr es peor que no tener suite, porque ocupa el sitio de una que si.
#
# Ahora hay una comprobacion ANTES de lanzar nada (ver «FALTA EL GUION» mas abajo): si el guion no
# existe, se dice por delante, en la salida principal, y el lanzador acaba con rc distinto de 0.
#
# ⚠️ Y `hilos` deja de ser «por defecto», que es la otra media verdad de esta tabla. Comprime los
# 755 MB del corpus con --max en siete celdas: a w=1 son 8.704 s medidos, o sea que la prueba
# entera son HORAS. Una suite por defecto tiene que poder correrse cuando se cambia algo; esta no
# se corria justamente por eso. Sigue disponible pidiendola por su nombre.
PRUEBAS="
cdc|T|D|¿estan los trozos dimensionados para NUESTRO arbol? (y ¿cuesta reloj trocear mas fino?)|env IN=/tmp/corpus-hilos bash bench/cdc-tallas.sh
contenedores|B|D|¿cuanto de un PDF es DEFLATE que no abrimos?|python3 bench/contenedores.py
zstdbusca|T|D|¿que fraccion del tiempo de zstd es BUSCAR?|bash bench/zstd-busqueda.sh
avance|T|D|¿se puede creer la barra? (primera palabra, silencio mas largo, la cola del 100%, retrocesos)|bash bench/avance.sh
idayvuelta|B|D|¿se restaura byte a byte lo que comprimimos? (10 tipos x rapido y --max)|bash bench/idayvuelta.sh
compat|B||⚠️ NO es compatibilidad general: es el test de UNA migracion (F264) y depende de .bak en /tmp|bash bench/verifica-compatibilidad.sh
incremental|B|D|¿el salto incremental se puede fiar? (ocho pruebas, una hecha para fallar)|bash bench/incremental-verifica.sh
hilos|T||¿escala vc-cm con los hilos? ¿caben las tablas achicandolas? ⚠️ HORAS|bash bench/hilos-y-memoria.sh
fase3|T||las dos cotas de la fase 3 de vdeflate|bash bench/lanza-f3.sh
frames|T||el barrido de tamaños de frame|bash bench/frames.sh
aguja|T||el camino y la aguja|bash bench/aguja.sh
racimos|B|Q|¿sobra el racimo de modelos, o basta con uno?|bash bench/racimos.sh
duplicados|B|Q|¿desde que tamaño conviene podar los DUPLICADOS? (a 512 KB cuesta +1,10%)|bash bench/duplicados-umbral.sh
cuantos|B|Q|¿cuantos de los cinco match hacen falta? (marginal Y acumulada)|bash bench/cuantos-match.sh
familia|B|Q|¿es estable la familia por formato? (¿se puede cachear la sonda?)|bash bench/familia-estable.sh
cebador|B|Q|¿cuanto compra arrancar el modelo en caliente? (cota inferior)|bash bench/cebador.sh
mezclador|B|Q|las cinco perillas del mezclador, y si alguna gana EN EL BORDE|bash bench/mezclador.sh
alineacion|B|Q|¿ven ruido los modelos en dato bit-packed? (con testigos xz/zstd)|bash bench/alineacion.sh
"

fila() { echo "$PRUEBAS" | grep "^$1|"; }
campo() { echo "$1" | cut -d'|' -f"$2"; }

if [ "${1:-}" = "--lista" ]; then
  cab "LAS PRUEBAS"
  printf "  %-14s %-8s %s\n" "nombre" "tipo" "que contesta"
  echo "  $(printf '%.0s-' $(seq 78))"
  echo "$PRUEBAS" | while IFS='|' read -r n t d q c; do
    [ -z "$n" ] && continue
    tt=$([ "$t" = "B" ] && echo "bytes" || echo "RELOJ")
    fa=""
    for w in $c; do case "$w" in bench/*) [ -e "$w" ] || fa="  ⚠️ FALTA $w" ;; esac; done
    et=""
    [ "$d" = "D" ] && et="   [por defecto]"
    [ "$d" = "Q" ] && et="   [--diseno]"
    printf "  %-14s %-8s %s%s%s\n" "$n" "$tt" "$q" "$et" "$fa"
  done
  echo
  echo "  bytes = deterministas, van EN PARALELO.  RELOJ = en serie, una por maquina."
  exit 0
fi

# ── que se corre ──────────────────────────────────────────────────────────────────────────────
case "${1:-}" in
  --bytes)  SEL=$(echo "$PRUEBAS" | awk -F'|' '$2=="B"{print $1}') ;;
  --reloj)  SEL=$(echo "$PRUEBAS" | awk -F'|' '$2=="T"{print $1}') ;;
  --diseno) SEL=$(echo "$PRUEBAS" | awk -F'|' '$3=="Q"{print $1}') ;;
  "")      SEL=$(echo "$PRUEBAS" | awk -F'|' '$3=="D"{print $1}') ;;
  *)       SEL="$*" ;;
esac
SEL=$(echo $SEL)

cab "LANZADOR DE PRUEBAS"
mkdir -p bench/registros/pruebas

# ⚠️ FALTA EL GUION: se comprueba ANTES de lanzar, y se dice AQUI.
#
# El 01-08 la suite corrio con seis pruebas cuyo guion no existe. Cada una devolvia `rc=127` a su
# propio log —«No such file or directory»— y el lanzador contaba doce lanzadas tan tranquilo.
# Nadie abre doce logs si el resumen no se queja. Una suite que no puede correr es PEOR que no
# tener suite, porque ocupa su sitio y pasa por cobertura.
#
# Se saca la ultima palabra del comando que sea un camino a bench/ y se mira si esta. Es una
# heuristica, pero coge el caso que importa: la prueba declarada y nunca escrita.
FALTAN=""
for n in $SEL; do
  f=$(fila "$n")
  [ -z "$f" ] && { echo "  no existe la prueba «$n» (mira --lista)"; exit 2; }
  for w in $(campo "$f" 5); do
    case "$w" in bench/*) [ -e "$w" ] || FALTAN="$FALTAN $n:$w" ;; esac
  done
done
if [ -n "$FALTAN" ]; then
  echo
  echo "  ⚠️ FALTA EL GUION DE $(echo $FALTAN | wc -w | tr -d ' ') PRUEBA(S) — no se lanza ninguna:"
  for x in $FALTAN; do printf "      %s\n" "$x"; done
  echo
  echo "  O se escriben, o se quitan de la tabla. Declarada-y-sin-escribir no vale:"
  echo "  la suite las contaria como lanzadas y su rc=127 se quedaria en su propio log."
  exit 3
fi

BY="" RE=""
for n in $SEL; do
  f=$(fila "$n")
  [ "$(campo "$f" 2)" = "B" ] && BY="$BY $n" || RE="$RE $n"
done

# ⚠️ LAS DE BYTES PRIMERO Y LAS DE RELOJ DESPUES. NO A LA VEZ.
#
# Esto estaba mal y estaba mal EN LA CARA de la regla: la cabecera de este mismo fichero dice, en
# mayusculas, «RELOJ -> va EN SERIE, y solo una por maquina. La misma prueba tarda 51,6 s o 97,1 s
# segun la carga». Y luego el codigo lanzaba las de bytes en paralelo Y la cadena de reloj al
# mismo tiempo. El juego por defecto son tres de bytes —una de ellas `idayvuelta`, que son veinte
# compresiones— machacando la maquina justo mientras `cdc` y `zstdbusca` cronometran.
#
# El cerrojo `toma_reloj` no lo cubria: impide DOS CADENAS DE RELOJ, no que alguien mas gaste la
# CPU. Contra lo que protege de verdad es contra relanzar la suite; contra la propia suite, nada.
#
# Cuesta reloj de pared —el total pasa de max(bytes, reloj) a bytes+reloj— y es el precio correcto:
# una medida de tiempo falseada no es una medida barata, es ninguna.

# ── primero las de BYTES: deterministas, todas a la vez ───────────────────────────────────────
if [ -n "$BY" ]; then
  N=$(echo $BY | wc -w | tr -d ' ')
  echo "  $N de BYTES, en paralelo: $BY"
  for n in $BY; do
    c=$(campo "$(fila "$n")" 5)
    ( $c > "bench/registros/pruebas/$n.log" 2>&1; echo "FIN $n rc=$?" >> "bench/registros/pruebas/$n.log" ) &
  done
  echo "    -> bench/registros/pruebas/<nombre>.log"
  [ -n "$RE" ] && echo "    (las de RELOJ esperan a que estas acaben: cronometrar con la maquina llena no vale)"
  wait
fi

# ── y despues las de RELOJ: una detras de otra, con cerrojo y con la maquina ya vacia ─────────
if [ -n "$RE" ]; then
  echo "  $(echo $RE | wc -w | tr -d ' ') de RELOJ, en serie: $RE · carga ahora $(carga)"
  ( toma_reloj || exit 0
    for n in $RE; do
      arbol_limpio || { echo "  $n: saltada, arbol sucio"; continue; }
      c=$(campo "$(fila "$n")" 5)
      echo "  [reloj] $n · $(date -u +%H:%M:%SZ) · carga $(carga)"
      $c > "bench/registros/pruebas/$n.log" 2>&1
      echo "FIN $n rc=$?" >> "bench/registros/pruebas/$n.log"
    done
    echo "FIN-RELOJ" >> bench/registros/pruebas/_reloj.log ) &
fi

echo
echo "  lanzadas. Para ver como van:  tail -f bench/registros/pruebas/*.log"
echo "  Y para saber cuales han acabado:  grep -l FIN bench/registros/pruebas/*.log"
wait
echo
echo "TODAS TERMINADAS · $(date -u +%H:%M:%SZ)"
for n in $SEL; do
  l="bench/registros/pruebas/$n.log"
  [ -f "$l" ] && printf "  %-14s %s\n" "$n" "$(grep '^FIN' "$l" | tail -1)"
done
