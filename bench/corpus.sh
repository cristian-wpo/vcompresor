#!/usr/bin/env bash
# EL CORPUS NO VIVE EN /tmp. Y esto lo comprueba antes de cada tanda.
#
# POR QUE EXISTE. `csvjson.10m` —una de las cinco muestras de `texto5`— SE PERDIO. Vivia en
# /tmp/compresor-f1/samples/ y un reinicio se la llevo. No hay generador en el repo que la
# reconstruya, asi que todas las medidas historicas de texto5 quedaron con una pata que ya no
# se puede volver a pisar: el 17-08 hubo que armar una `csvjson-r.10m` de la misma clase de
# dato y declararla APARTE, porque hacerla pasar por la otra habria contaminado el libro mayor.
#
# Perder una muestra no es perder un fichero: es perder la COMPARABILIDAD de todo lo medido
# contra ella. Y las otras siete seguian en /tmp esperando su turno.
#
# Ahora el corpus vive en $CORPUS (por defecto ~/vcompresor-corpus) y cada fichero lleva su
# sha256 anotado aqui. Este guion:
#   · sin argumentos  — verifica que estan las ocho y que su sha CASA. Falla con rc=1 si no.
#   · --rescata       — copia a $CORPUS lo que encuentre en las rutas viejas de /tmp.
#
# Las cuatro de texto historicas estan PROBADAS, no supuestas: `vc-cm c` sobre ellas reproduce
# los sha256 del control de determinismo de F5 (docs/f6-salto-de-match.md §2).
set -uo pipefail
cd "$(dirname "$0")/.."
CORPUS=${VC_CORPUS:-$HOME/vcompresor-corpus}

# muestra · subcarpeta · sha256 · de donde se rescata si falta
MANIFIESTO="
codigo.10m	texto5	132c06fedd3d6c29abb2642acddea202b0cb4b4d7f914ff97616765206fb1390	/tmp/compresor-f1/samples/codigo.10m
enwik8.10m	texto5	5d7a724fcfbea8f6	/tmp/compresor-f1/samples/enwik8.10m
sql.10m	texto5	a11cd4efaa116b36	/tmp/compresor-f1/samples/sql.10m
logs.10m	texto5	3b5e9832787abc8c	/tmp/x5/logs.10m
csvjson-r.10m	texto5	ec9b5d73a7b6f602	/tmp/f6q6/m/csvjson-r.10m
bin.8m	no-texto3	f2b9edb9892e76fe	/tmp/nt/bin.8m
pdf.8m	no-texto3	41343d3980017359	/tmp/nt/pdf.8m
png.8m	no-texto3	267e871a9ae1c568	/tmp/nt/png.8m
"

if [ "${1:-}" = "--rescata" ]; then
  echo "$MANIFIESTO" | while IFS=$'\t' read -r m sub sha orig; do
    [ -n "${m:-}" ] || continue
    mkdir -p "$CORPUS/$sub"
    [ -f "$CORPUS/$sub/$m" ] && continue
    if [ -f "$orig" ]; then cp "$orig" "$CORPUS/$sub/$m" && echo "  rescatada  $sub/$m"
    else echo "  ⚠️ NO ESTA en ningun sitio: $sub/$m (buscada en $orig)"; fi
  done
fi

FALLOS=0
echo "$MANIFIESTO" | while IFS=$'\t' read -r m sub sha orig; do
  [ -n "${m:-}" ] || continue
  f="$CORPUS/$sub/$m"
  if [ ! -f "$f" ]; then echo "❌ FALTA        $sub/$m"; echo x >> /tmp/.corpus-fallos; continue; fi
  real=$(sha256sum "$f" | cut -d' ' -f1)
  # el manifiesto guarda 16 o 64 caracteres; se compara por prefijo declarado
  if [ "${real:0:${#sha}}" != "$sha" ]; then
    echo "❌ SHA DISTINTO $sub/$m  (esperaba ${sha:0:16}, es ${real:0:16})"; echo x >> /tmp/.corpus-fallos
  else
    echo "✅ $sub/$m  ${real:0:16}"
  fi
done

if [ -s /tmp/.corpus-fallos ]; then
  FALLOS=$(wc -l < /tmp/.corpus-fallos); rm -f /tmp/.corpus-fallos
  echo; echo "CORPUS INCOMPLETO: $FALLOS problema(s). No midas contra el hasta arreglarlo."
  exit 1
fi
rm -f /tmp/.corpus-fallos
echo; echo "CORPUS OK — 8 muestras en $CORPUS, todas con su sha."

# ── LOS GRANDES, que son los que sostienen la referencia y la tabla de rivales ──────────────
#
# No se rescatan: no estan. Se comprueban y se DICE, porque el fallo de no decirlo ya paso —
# `bench/referencia.sh` apunta a /tmp/compresor-f1/corpus y a /tmp/deflate-banco2, y hoy NO
# EXISTE NINGUNO DE LOS DOS: la fila fija que deberia correr en cada cierre no puede correr.
echo
echo "LOS GRANDES (no caben en el rescate; se declara su estado):"
falta_grande=0
for par in \
  "/tmp/compresor-f1/corpus|arbol2.62g — la fila fija de bench/referencia.sh" \
  "/tmp/compresor-f1/corpus/codigo|rival corpus/codigo — contra el que se mide el campo (zpaq/zstd/xz) y la tabla de rivales publicada" \
  "/tmp/deflate-banco2|deflate-banco2 — los 2.522 flujos DEFLATE de la fase 3"
do
  ruta=${par%%|*}; que=${par#*|}
  if [ -e "$ruta" ]; then echo "  ✅ $que"
  else echo "  ❌ PERDIDO  $que"; echo "        (esperado en $ruta)"; falta_grande=$((falta_grande+1)); fi
done
if [ "$falta_grande" -gt 0 ]; then
  echo
  echo "  ⚠️ $falta_grande corpus grande(s) perdido(s). Lo que eso significa, sin adornos:"
  echo "     · bench/referencia.sh NO PUEDE CORRER — la linea de tiempo se queda sin su fila fija."
  echo "     · las medidas contra los rivales NO SON REPRODUCIBLES contra su corpus original."
  echo "     Se rearman con la receta de bench/muestras.tsv, pero seran corpus NUEVOS: hay que"
  echo "     declararlos aparte (como se hizo con csvjson-r) y NO compararlos con lo historico."
  # 🚨 Y SE SALE CON 2, QUE ES EL PUNTO. La primera version de esto imprimia el aviso y devolvia
  # 0; `bench/referencia.sh` lo llamaba, veia rc=0, seguia adelante y escribio en el libro mayor
  # una fila de «el arbol real con los defectos -> 0 B». Un candado que avisa pero deja pasar no
  # es un candado: es un comentario. 1 = faltan muestras · 2 = faltan corpus grandes.
  exit 2
fi
