#!/usr/bin/env bash
# CAMINO 1 · ¿QUE FRACCION DEL TIEMPO DE ZSTD ES BUSCAR COINCIDENCIAS?
#
# Es la prueba que abre D13, y estaba NOMBRADA en la documentacion de vc-zstd-perfil y sin
# escribir. La abro ahora.
#
# POR QUE ES EL CAMINO MAS GRANDE. La piramide dice que «comprimir» son 133,28 s de los 197,46 s
# de CPU del archivador entero: el 67,5%. Y lo que se comprime es un plano de 6,03 GB que salio de
# expandir 357 MB de DEFLATE — un DEFLATE que YA TRAIA las coincidencias encontradas y que
# tiramos. Si buscar es la mitad del tiempo de zstd, hay un tercio de la CPU del programa entero
# encima de la mesa. Si es el 10%, D13 se cierra como idea cara y se dice.
#
# DOS CAMINOS INDEPENDIENTES AL MISMO NUMERO, y esa es la parte importante del diseño:
#
#   A) EL PERFIL. zstd es C compilado dentro de nuestro binario, asi que sus funciones salen con
#      nombre. Se agrupan por lo que hacen. ⚠️ El reparto de simbolos SE IMPRIME ENTERO: si el
#      criterio de agrupacion no se puede auditar, el numero no vale nada.
#
#   B) EL BARRIDO DE ESTRATEGIAS, que no depende de perf «por si perf no esta o miente». Entre
#      `fast` y `btultra2` lo que cambia es CASI SOLO cuanto se esfuerza el buscador; la entropia
#      -Huffman de literales, FSE de secuencias- es la misma. La diferencia de reloj ACOTA lo que
#      cuesta buscar. No lo mide exacto: lo acota, y eso ya decide.
#
# Si los dos caminos dan cosas muy distintas, el numero no esta listo y se dice.
#
# ⚠️ MIDE TIEMPO: en serie, arbol limpio, carga baja.
set -uo pipefail
cd "$(dirname "$0")/.."
PLANO=${PLANO:-/tmp/plano.bin}
NIVEL=${NIVEL:-19}
BIN=$PWD/target/release/vc-zstd-perfil
[ -f "$PLANO" ] || { echo "no esta el plano en $PLANO — fabricalo con --prepara"; exit 3; }
[ -x "$BIN" ] || { echo "no esta $BIN"; exit 3; }

SUCIO=$(git status --porcelain -- . ':!bench/historia.jsonl' ':!bench/datos.json' ':!bench/panel.html' ':!bench/registros' ':!bench/registros-m3' 2>/dev/null | wc -l)
[ "$SUCIO" -eq 0 ] || { echo "ABORTA: $SUCIO ficheros sin commitear, y esto mide tiempo"; exit 3; }
C=$(awk '{print $1}' /proc/loadavg)
if (( $(echo "$C > 8" | bc -l) )); then echo "ABORTA: carga $C, esto es tiempo"; exit 3; fi

echo "CAMINO 1 · ¿cuanto de zstd es buscar? · commit $(git rev-parse --short HEAD) · carga $C"
echo "plano: $(stat -c%s "$PLANO") B · nivel $NIVEL"
echo

# ── B) el barrido de estrategias, primero: no necesita perf ────────────────────────────────────
echo "═══ CAMINO B · las nueve estrategias (la entropia es la misma; cambia el buscador) ═══"
bench/mide.sh -H archivador -e "F19 zstd: barrido de estrategias" -m plano128 -c "nivel $NIVEL" \
  -n "acota lo que cuesta buscar coincidencias sin depender de perf" \
  -- "$BIN" "$PLANO" "$NIVEL"
echo

# ── A) el perfil, con el reparto de simbolos a la vista ────────────────────────────────────────
echo "═══ CAMINO A · el perfil, agrupando los simbolos de zstd por lo que hacen ═══"
if ! command -v perf >/dev/null 2>&1; then
  echo "  perf no esta: este camino no se puede hacer. El B de arriba sigue valiendo."
  echo FIN-BUSQUEDA; exit 0
fi
P=$(mktemp /tmp/zp.XXXXXX.data)
VC_SOLO_COMPRIME=1 perf record -q -g --call-graph=fp -o "$P" "$BIN" "$PLANO" "$NIVEL" 2>/dev/null
perf report -i "$P" --no-children --sort symbol --percent-limit 0.05 2>/dev/null \
  | grep -E '^\s+[0-9]' | sed 's/^\s*//' > /tmp/zp.txt
rm -f "$P"

python3 - <<'PY'
import re, sys
# EL CRITERIO DE AGRUPACION, ESCRITO Y A LA VISTA. Un reparto de simbolos que no se puede
# auditar es una opinion con formato de porcentaje.
GRUPOS = [
 ("BUSCAR coincidencias", [
   "FindBestMatch", "insertBt", "updateTree", "ZSTD_count", "insertAndFindFirstIndex",
   "compressBlock_lazy", "compressBlock_btultra", "compressBlock_btopt", "compressBlock_opt",
   "compressBlock_greedy", "compressBlock_fast", "compressBlock_dfast", "ZSTD_ldm",
   "fillHashTable", "fillDoubleHashTable", "reduceIndex", "ZSTD_row_",
   # AÑADIDOS EL 01-08 al ver que el 43% «sin clasificar» era en su mayor parte esto:
   # `btGetAllMatches` es LA funcion del analizador optimo -btopt/btultra, o sea el nivel 19
   # con el que se mide- y por si sola era el 33,71% del perfil. La tabla tenia `insertBt` y
   # `ZSTD_count` pero no ella, asi que el simbolo MAS GRANDE de todo el perfil caia fuera de
   # su grupo y el numero que este experimento existe para medir salia bajo.
   "btGetAllMatches",
   # ⚠️ `litLengthPrice` (2,95%) vive en zstd_opt.c y calcula lo que COSTARIA cada candidato
   # para que el analizador optimo elija. Es la decision de QUE coincidencia usar, no la
   # codificacion de la elegida, y la pregunta del experimento es cuanto cuesta DECIDIR. Va
   # aqui, y queda dicho para que se pueda discutir.
   "litLengthPrice", "PriceTable", "rescaleFreqs"]),
 ("ENTROPIA (Huffman/FSE)", [
   "HUF_", "FSE_", "encodeSequences", "buildCTable", "compressLiterals", "compressSequences",
   "compressSuperBlock", "writeBlock", "buildSeq",
   # `HIST_` faltaba aunque `HUF_` y `FSE_` estaban: los histogramas son el paso previo a
   # construir las tablas. Igual `seqToCodes`, `updateStats` y `buildEntropyStatistics`.
   "HIST_", "seqToCodes", "updateStats", "buildEntropyStatistics"]),
 # GRUPO NUEVO, y va aparte a proposito: decidir DONDE cortar el bloque no es ni buscar ni
 # codificar. Meterlo con calzador en cualquiera de los dos habria inflado justo el numero que
 # este experimento quiere medir.
 ("partir bloques", ["recordFingerprint", "splitBlock", "SplitBlock"]),
 ("copiar / mover", ["memcpy", "memmove", "memset", "wildcopy", "copyBlock", "storeSeq"]),
]
filas = []
for l in open("/tmp/zp.txt"):
    m = re.match(r"([\d.]+)%\s+(?:\S+\s+)?\[?[.k]?\]?\s*(\S.*)$", l.strip())
    if not m: continue
    filas.append((float(m.group(1)), m.group(2).strip()))
if not filas:
    print("  el perfil salio vacio — perf no pudo resolver simbolos"); sys.exit(0)
tot = sum(p for p, _ in filas)
acum = {g: 0.0 for g, _ in GRUPOS}; acum["otros / sin clasificar"] = 0.0
detalle = {g: [] for g, _ in GRUPOS}; detalle["otros / sin clasificar"] = []
for p, s in filas:
    for g, claves in GRUPOS:
        if any(k in s for k in claves):
            acum[g] += p; detalle[g].append((p, s)); break
    else:
        acum["otros / sin clasificar"] += p; detalle["otros / sin clasificar"].append((p, s))
print(f"\n  el perfil cubre {tot:.1f}% de las muestras (el resto queda bajo el umbral)\n")
for g in list(acum):
    if acum[g] <= 0: continue
    print(f"  {g:<26} {acum[g]:>6.1f}%   ({100*acum[g]/tot:>5.1f}% de lo cubierto)")
print("\n  ── el reparto, simbolo a simbolo, para que se pueda discutir ──")
for g in list(detalle):
    if not detalle[g]: continue
    print(f"\n  {g}:")
    for p, s in sorted(detalle[g], reverse=True)[:12]:
        print(f"    {p:>5.2f}%  {s[:74]}")
sc = acum["otros / sin clasificar"]
if sc > 0.15 * tot:
    print(f"\n  ⚠️ {100*sc/tot:.0f}% sin clasificar: el reparto NO es de fiar todavia. Mirar la")
    print("     lista de arriba y meter los que falten en su grupo antes de citar el numero.")
PY
echo
echo "Si el perfil y el barrido de estrategias no cuentan lo mismo, el numero no esta listo."
echo FIN-BUSQUEDA
