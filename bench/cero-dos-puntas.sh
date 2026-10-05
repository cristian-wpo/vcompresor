#!/usr/bin/env bash
# ¿CUANTO HA MEJORADO EL PRODUCTO ENTRE DOS COMMITS? Los DOS extremos medidos HOY.
#
#   bench/cero-dos-puntas.sh <commit-viejo> [commit-nuevo]     (por defecto el nuevo es HEAD)
#
# POR QUE EXISTE, y es la pregunta que mas veces se ha contestado mal aqui. «¿Cuanto hemos
# mejorado?» se venia respondiendo restando una medida de hoy contra otra de hace horas o dias. Eso
# NO es una comparacion: entre las dos cambio la maquina, la carga, el corpus o las tres. El
# 31-07 esa forma de contarlo produjo tres numeros falsos seguidos —un -0,972% que era de antes de
# otro arreglo, un +609% que comparaba 2.621 MB contra 711 MB, y un -43,33% que contaba la misma
# mejora catorce veces—.
#
# LA FORMA QUE SI VALE: construir LOS DOS commits ahora, con el mismo compilador, y correrlos
# ALTERNANDO en la misma maquina y sobre el mismo corpus. Asi no hay nada viejo en la resta.
#
# ⚠️ LA PRIMERA RONDA SE DESCARTA, y no es superstición: esta medido. La primera pasada paga cache
# de fichero frio y sale entre un 8% y un 20% mas lenta SEA CUAL SEA la variante. Comprobado dos
# veces el 31-07: en el barrido del CDC, invirtiendo el orden, la primera volvia a ser la lenta; y
# aqui mismo, incluir la ronda 1 daba -16,4% donde con la cache caliente son -9,04%. Publicar la
# primera ronda es publicar el doble de lo que hay.
#
# ⚠️ Y EL RSS NO SE AFIRMA salvo que la diferencia pase del 25%. Entre rondas de la MISMA variante
# el pico de memoria se movio de 1.190 a 1.482 MB: un 22% de dispersion. Con ese ruido, un +11%
# aparente no es nada. Se imprime, pero con su dispersion al lado para que no se lea como un dato.
#
# ══════════════════════════════════════════════════════════════════════════════════════════════
# LO QUE SALIO EL 31-07-2026 · 0597502 (07:56) contra 2078652 (21:30)  ·  53 commits
# Mac, corpus-mac (570 MB, 11 tipos, 10.824 ficheros), nivel 9, defecto contra defecto:
#
#                    esta mañana        ahora          delta
#     bytes          314.110.498    300.642.372      -4,288%   <- exacto, identico en 5 rondas
#     reloj              29,05 s        26,43 s      -9,04%    <- rondas 2-5, disp. 3,4% y 1,9%
#     RSS                 1.344 MB      1.330 MB     -1,0%     <- dentro de un ruido del 22%
#
# MENOS BYTES **Y** MENOS TIEMPO A LA VEZ: gana a cualquier precio de disco o de CPU, no hay que
# arbitrar nada. Lo que hay dentro: la ventana igualada al frame, --solid=64 por defecto, la
# juntura de preflate que verificaba dos veces, y el modelo v4 que comprueba el match.
#
# ⚠️ ESTE NUMERO ES DE ESTE CORPUS. Sobre el arbol de codigo del servidor, F14 dio -19,92% de
# reloj: mas del doble. No se extrapola ninguno de los dos — se dice de que corpus es.
# ══════════════════════════════════════════════════════════════════════════════════════════════
set -uo pipefail
cd "$(dirname "$0")/.."
. bench/comun.sh

VIEJO=${1:?uso: cero-dos-puntas.sh <commit-viejo> [commit-nuevo]}
NUEVO=${2:-HEAD}
IN=${IN:?define IN con la carpeta del corpus a medir}
RONDAS=${RONDAS:-5}
W=$(mktemp -d /tmp/cero.XXXXXX); trap 'rm -rf "$W"' EXIT
[ -d "$IN" ] || { echo "no esta el corpus $IN"; exit 3; }

cab "CUANTO HA MEJORADO EL PRODUCTO · $VIEJO -> $NUEVO"
echo "  corpus $(du -sh "$IN" | cut -f1) · $(find "$IN" -type f | wc -l | tr -d ' ') ficheros · $RONDAS rondas alternando"
echo "  $(git rev-list --count "$VIEJO..$NUEVO" 2>/dev/null || echo '?') commits entre los dos"
echo

# ── los dos binarios, del MISMO git y el MISMO compilador ─────────────────────────────────────
for lado in viejo nuevo; do
  ref=$([ "$lado" = viejo ] && echo "$VIEJO" || echo "$NUEVO")
  rm -rf "$W/$lado"; mkdir -p "$W/$lado"
  git archive --format=tar "$ref" | tar x -C "$W/$lado" || { echo "no se pudo sacar $ref"; exit 4; }
  ( cd "$W/$lado" && cargo build --release -q 2>&1 | grep -E "^error" -A3 ) && { echo "no compila $ref"; exit 4; }
  echo "  $lado ($ref)  binario $(shasum -a 256 "$W/$lado/target/release/vcompresor" 2>/dev/null | cut -c1-16 || sha256sum "$W/$lado/target/release/vcompresor" | cut -c1-16)"
done
# Dos binarios IGUALES no son un A/B: seria comparar algo consigo mismo, que ya paso una vez hoy.
HV=$(shasum -a 256 "$W/viejo/target/release/vcompresor" 2>/dev/null || sha256sum "$W/viejo/target/release/vcompresor")
HN=$(shasum -a 256 "$W/nuevo/target/release/vcompresor" 2>/dev/null || sha256sum "$W/nuevo/target/release/vcompresor")
[ "${HV%% *}" = "${HN%% *}" ] && { echo "  ABORTA: los dos binarios son IDENTICOS. No hay nada que comparar."; exit 5; }
echo

# ── alternando, en serie ──────────────────────────────────────────────────────────────────────
: > "$W/med.txt"
printf "  %-6s %-7s %10s %14s %9s\n" ronda version reloj bytes "RSS MB"
for r in $(seq 1 "$RONDAS"); do
  for lado in viejo nuevo; do
    rm -f "$W/o.vc"
    T0=$(python3 -c 'import time;print(time.time())')
    if [ "$(uname -s)" = Darwin ]; then /usr/bin/time -l "$W/$lado/target/release/vcompresor" compress "$IN" -o "$W/o.vc" --level 9 >/dev/null 2>"$W/t.txt"
    else /usr/bin/time -v -o "$W/t.txt" "$W/$lado/target/release/vcompresor" compress "$IN" -o "$W/o.vc" --level 9 >/dev/null 2>&1; fi
    T1=$(python3 -c 'import time;print(time.time())')
    B=$(talla "$W/o.vc")
    RSS=$(awk '/maximum resident/{print int($1/1048576)} /Maximum resident/{print int($6/1024)}' "$W/t.txt" | head -1)
    S=$(python3 -c "print(f'{$T1-$T0:.2f}')")
    echo "$r $lado $S $B ${RSS:-0}" >> "$W/med.txt"
    printf "  %-6s %-7s %9ss %14s %9s\n" "$r" "$lado" "$S" "$B" "${RSS:-0}"
  done
done
echo

# ── el veredicto, SIN la ronda 1 ──────────────────────────────────────────────────────────────
python3 - "$W/med.txt" <<'PY'
import sys, statistics as st
f = [l.split() for l in open(sys.argv[1]) if l.strip()]
def col(lado, i, desde=2):
    return [float(x[i]) for x in f if x[1] == lado and int(x[0]) >= desde]
print("  EL VEREDICTO · la ronda 1 NO cuenta: paga cache de fichero frio y sale entre 8% y 20%")
print("  mas lenta sea cual sea la variante. Incluirla ya publico un -16,4% donde habia -9,04%.")
print()
r1 = {l: col(l, 2, 1)[0] for l in ("viejo", "nuevo")}
print(f"    (ronda 1, descartada:  viejo {r1['viejo']:.1f}s   nuevo {r1['nuevo']:.1f}s)")
print()
for i, nom, exacto in ((3, "bytes", True), (2, "reloj", False), (4, "RSS", False)):
    a, b = col("viejo", i), col("nuevo", i)
    if not a or not b: continue
    ma, mb = st.mean(a), st.mean(b)
    if ma <= 0: continue
    d = 100 * (mb - ma) / ma
    if exacto:
        ok = len(set(a)) == 1 and len(set(b)) == 1
        marca = "  EXACTO, identico en todas las rondas" if ok else "  ⚠️ los bytes CAMBIAN entre rondas: algo no es determinista"
        print(f"    {nom:<7} {ma:>15,.0f} -> {mb:>15,.0f}   {d:>+8.3f}%{marca}")
    else:
        da = 100*(max(a)-min(a))/ma if ma else 0
        db = 100*(max(b)-min(b))/mb if mb else 0
        aviso = ""
        if max(da, db) > abs(d):
            aviso = f"  ⚠️ NO SE AFIRMA: la dispersion ({max(da,db):.1f}%) es mayor que la diferencia"
        print(f"    {nom:<7} {ma:>15,.2f} -> {mb:>15,.2f}   {d:>+8.3f}%   disp. {da:.1f}% y {db:.1f}%{aviso}")
PY
echo
echo "FIN-CERO"
