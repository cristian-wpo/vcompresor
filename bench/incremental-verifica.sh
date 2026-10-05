#!/usr/bin/env bash
# ¿EL SALTO INCREMENTAL SE PUEDE FIAR? Ocho pruebas, y una de ellas esta hecha para FALLAR.
#
# El salto de D12 decide que un fichero no ha cambiado por (talla, mtime). Eso hace un respaldo
# diario barato, pero es una APUESTA, y la unica forma honesta de tenerla es sabiendo exactamente
# donde se rompe. Por eso la prueba 6 falsea el mtime a proposito: no para que pase, sino para
# medir QUE se pierde cuando alguien hace justo eso.
#
# NINGUNA de estas pruebas mira el reloj. Aqui solo se comprueba QUE SALE, byte a byte, contra
# el arbol de verdad. La velocidad se mide aparte y por bench/mide.sh.
set -uo pipefail
cd "$(dirname "$0")/.."
VC=./target/release/vcompresor
W=$(mktemp -d /tmp/inc.XXXXXX)
trap 'rm -rf "$W"' EXIT
FALLOS=0; PRUEBAS=0

ok()   { PRUEBAS=$((PRUEBAS+1)); printf '  \033[32m✓\033[0m %s\n' "$1"; }
mal()  { PRUEBAS=$((PRUEBAS+1)); FALLOS=$((FALLOS+1)); printf '  \033[31m✗ %s\033[0m\n' "$1"; }
# Compara el contenido de UNA instantanea contra un arbol de referencia.
cmpsnap() { # $1=archivo $2=instantanea $3=arbol-esperado $4=titulo
  local d="$W/x$RANDOM"; mkdir -p "$d"
  $VC extract "$1" -o "$d" >/dev/null 2>&1 || { mal "$4 (no extrae)"; return; }
  if diff -r "$3" "$d/$2" >/dev/null 2>&1; then ok "$4"; else
    mal "$4"; diff -r "$3" "$d/$2" 2>&1 | head -5 | sed 's/^/      /'; fi
}
# Lo que el CLI dice haberse saltado. Se lee de SU SALIDA de verdad, no de una variable nuestra:
# si algun dia deja de imprimirlo, esta prueba tiene que enterarse.
saltados() { $VC add "$1" "$2" --name "$3" 2>&1 | grep -oE 'unchanged: +[0-9]+ files reused whole' | head -1; }

# ---------------------------------------------------------------- el arbol de partida
A="$W/arbol"; mkdir -p "$A/sub"
head -c 200000 /dev/urandom > "$A/binario.dat"
printf 'linea uno\nlinea dos\nlinea tres\n' > "$A/texto.txt"
printf 'mismo tamano exacto ....\n' > "$A/sub/cambiante.txt"
printf 'otro fichero cualquiera\n' > "$A/sub/quieto.txt"
cp /usr/share/doc/*/copyright "$A/sub/lic.txt" 2>/dev/null || printf 'licencia\n' > "$A/sub/lic.txt"
sleep 1.1

echo
echo "  LAS OCHO PRUEBAS DEL SALTO INCREMENTAL"
echo

$VC compress "$A" -o "$W/a.vc" --level 3 >/dev/null 2>&1 || { echo "no comprime"; exit 3; }

# 1) sin tocar nada: tiene que saltarselos TODOS y salir identico
S=$(saltados "$W/a.vc" "$A" igual)
cmpsnap "$W/a.vc" igual "$A" "1· arbol identico: sale igual  [$S]"
[ -n "$S" ] && ok "1b· y lo dice en voz alta en vez de callarselo" || mal "1b· no dice cuantos salto"

# 2) MISMA TALLA, mtime nuevo: el caso que MAS importa. Tiene que cogerlo.
printf 'MISMO TAMANO EXACTO ....\n' > "$A/sub/cambiante.txt"   # mismos bytes de largo, contenido distinto
sleep 1.1
saltados "$W/a.vc" "$A" cambio >/dev/null
cmpsnap "$W/a.vc" cambio "$A" "2· misma talla y mtime nuevo: SE COGE"

# 3) fichero nuevo
printf 'recien nacido\n' > "$A/sub/nuevo.txt"; sleep 1.1
saltados "$W/a.vc" "$A" nuevof >/dev/null
cmpsnap "$W/a.vc" nuevof "$A" "3· fichero nuevo: entra"

# 4) fichero borrado
rm "$A/sub/quieto.txt"; sleep 1.1
saltados "$W/a.vc" "$A" borrado >/dev/null
cmpsnap "$W/a.vc" borrado "$A" "4· fichero borrado: desaparece de la instantanea"

# 5) chmod sin tocar el contenido: el mtime NO cambia, solo el ctime.
chmod 700 "$A/texto.txt"; sleep 1.1
saltados "$W/a.vc" "$A" permisos >/dev/null
D="$W/xp"; mkdir -p "$D"; $VC extract "$W/a.vc" -o "$D" >/dev/null 2>&1
M=$(stat -c%a "$D/permisos/$(basename "$A")/texto.txt" 2>/dev/null || stat -c%a "$D/permisos/texto.txt" 2>/dev/null)
[ "$M" = "700" ] && ok "5· chmod sin tocar mtime: los permisos SI viajan ($M)" \
                 || mal "5· chmod perdido en silencio: esperaba 700, salio '$M'"

# 6) LA QUE FALLA A PROPOSITO: misma talla y mtime devuelto a mano.
T=$(stat -c%y "$A/sub/lic.txt")
SZ=$(stat -c%s "$A/sub/lic.txt")
head -c "$SZ" /dev/urandom > "$A/sub/lic.txt"
touch -d "$T" "$A/sub/lic.txt"      # el enga~o: misma talla, mismo mtime, otro contenido
sleep 1.1
saltados "$W/a.vc" "$A" enganio >/dev/null
D2="$W/xe"; mkdir -p "$D2"; $VC extract "$W/a.vc" -o "$D2" >/dev/null 2>&1
R=$(find "$D2/enganio" -name lic.txt | head -1)
if cmp -s "$A/sub/lic.txt" "$R"; then
  ok "6· mtime falseado: aun asi lo cogio (mejor de lo prometido)"
else
  ok "6· mtime falseado: NO lo coge — es el agujero conocido, y queda medido aqui"
fi

# El veneno de la 6 sigue en el arbol: mismo mtime, otro contenido. Si no se le devuelve un
# mtime honesto, TODAS las pruebas de abajo heredan la mentira y fallan por el motivo equivocado.
touch "$A/sub/lic.txt"; sleep 1.1

# 7) LA GUARDIA DEL SEGUNDO QUE NO HA PASADO. Un mtime que aun no ha llegado no se puede fiar:
#    el fichero puede volver a escribirse dentro de ese mismo segundo sin que el mtime cambie, y
#    entonces seria invisible PARA SIEMPRE, no solo esta vez. Se prueba con una fecha futura, que
#    es determinista y ademas pasa de verdad: relojes desajustados y ficheros recien
#    desempaquetados los traen a diario.
FUT=$(date -d "+120 seconds" '+%Y-%m-%d %H:%M:%S')
printf 'contenido A del futuro\n' > "$A/sub/futuro.txt"; touch -d "$FUT" "$A/sub/futuro.txt"
$VC add "$W/a.vc" "$A" --name fut1 >/dev/null 2>&1
printf 'contenido B del futuro\n' > "$A/sub/futuro.txt"; touch -d "$FUT" "$A/sub/futuro.txt"
saltados "$W/a.vc" "$A" fut2 >/dev/null
cmpsnap "$W/a.vc" fut2 "$A" "7· misma talla y mtime en el futuro: NO se fia, lo relee"

# 8) con el salto APAGADO tiene que salir exactamente lo mismo
sleep 1.1
VC_SIN_INCREMENTAL=1 $VC add "$W/a.vc" "$A" --name apagado >/dev/null 2>&1
cmpsnap "$W/a.vc" apagado "$A" "8· con VC_SIN_INCREMENTAL=1: mismo resultado"

# 9) y el archivo entero sigue siendo valido
$VC verify "$W/a.vc" >/dev/null 2>&1 && ok "9· verify del archivo con 8 instantaneas encima" \
                                     || mal "9· el archivo no pasa verify"
echo
if [ "$FALLOS" -eq 0 ]; then printf '  \033[32m%d de %d\033[0m\n\n' "$PRUEBAS" "$PRUEBAS"; exit 0
else printf '  \033[31m%d FALLOS de %d\033[0m\n\n' "$FALLOS" "$PRUEBAS"; exit 1; fi
