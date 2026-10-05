#!/usr/bin/env bash
# EL CANDADO DE LA FASE 3. CAMINO P14: vc-fase3.rs cambio DOCE veces el 02-08 y no tiene ni un
# test. Lo unico que la protegia eran los controles de cada medida —que son buenos— pero se
# corren A MANO y solo cuando a alguien se le ocurre.
#
# Esto no sustituye a una suite: es lo minimo que evita publicar una cifra despues de haber roto
# algo. Corre sobre UNA DE CADA DIEZ del banco (252 flujos, ~6 s) y compara cuatro numeros contra
# los esperados. Si cambian, no dice «peor» ni «mejor»: dice QUE CAMBIO, y el que lo mira decide.
#
#   uso:  bash bench/regresion-fase3.sh [--fijar]
#
# `--fijar` reescribe los esperados con lo que salga AHORA. Se usa despues de un cambio que se ha
# medido y aceptado a proposito, nunca para «que pase».
set -u
cd "$(dirname "$0")/.." || exit 1

ESPERADOS="bench/regresion-fase3.esperado"
SUB=/tmp/banco-regresion-fase3
BANCO="${1:-/tmp/deflate-banco2}"
[ "$BANCO" = "--fijar" ] && BANCO=/tmp/deflate-banco2

if [ ! -d "$BANCO" ]; then echo "no hay banco en $BANCO"; exit 2; fi

# El subconjunto es DETERMINISTA y se deriva del banco: una de cada diez por orden alfabetico.
# Asi no hay que guardar copias ni depende de que /tmp sobreviva a un reinicio.
rm -rf "$SUB"; mkdir -p "$SUB"
i=0
for f in "$BANCO"/*.raw; do
  i=$((i + 1))
  [ $((i % 10)) -eq 1 ] && ln -s "$f" "$SUB/$(basename "$f")"
done
N=$(ls "$SUB" | wc -l)

SAL=$(./target/release/vc-fase3 "$SUB" 2>/dev/null)
AC=$(echo "$SAL"  | grep -o 'acierto [0-9.]*%' | head -1 | grep -o '[0-9.]*')
FA=$(echo "$SAL"  | grep -o 'fallos [0-9]*'    | head -1 | grep -o '[0-9]*')
CO=$(echo "$SAL"  | awk '/ESTRUCTURA DE preflate/,/preflate real/' | grep 'TOTAL' | grep -o '[0-9.]*')
# ⚠️ En el modo por defecto (UNA configuracion) el techo es IGUAL a los fallos: «ninguna
# configuracion lo explica» y «la unica fallo» son la misma frase. Se conserva porque cazaria un
# cambio en la logica del techo, pero NO se lea como un techo de verdad — ese sale con las nueve.
TE=$(echo "$SAL"  | grep -A 3 'EL TECHO' | grep 'NINGUNA' | awk '{print $(NF-1)}')
AHORA="flujos=$N acierto=$AC fallos=$FA coste=$CO techo=$TE"

if [ "${2:-${1:-}}" = "--fijar" ] || [ "${1:-}" = "--fijar" ]; then
  echo "$AHORA" > "$ESPERADOS"
  echo "FIJADO: $AHORA"
  exit 0
fi

if [ ! -f "$ESPERADOS" ]; then
  echo "no hay esperados. Corre:  bash bench/regresion-fase3.sh --fijar"
  echo "  ahora mismo saldria: $AHORA"
  exit 2
fi

ANTES=$(cat "$ESPERADOS")
if [ "$AHORA" = "$ANTES" ]; then
  echo "✅ la fase 3 no se ha movido"
  echo "   $AHORA"
  exit 0
fi
echo "⚠️ LA FASE 3 HA CAMBIADO. No es un fallo por si mismo: es que hay que mirarlo."
echo "   antes: $ANTES"
echo "   ahora: $AHORA"
echo
echo "   Si el cambio es a proposito y esta medido:  bash bench/regresion-fase3.sh --fijar"
exit 1
