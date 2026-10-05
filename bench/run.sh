#!/usr/bin/env bash
# Benchmark reproducible de vcompresor. Un comando, corpus PÚBLICO, round-trip verificado.
#
#   bash bench/run.sh
#
# No hay trampa posible: el corpus se descarga de su fuente original, los rivales se miden en
# la MISMA máquina y en la misma ejecución, y cada resultado lleva su comprobación de
# round-trip al lado. Si un round-trip falla, la fila lo dice.
#
# Silesia (211 MB, 12 ficheros) es el corpus de referencia clásico y es DELIBERADAMENTE
# desfavorable para nosotros: son doce ficheros sueltos, así que no hay deduplicación entre
# ficheros que aprovechar, y lleva binarios y bases de datos donde nuestro carril de texto no
# entra. La comparación en datos reales —árboles de miles de ficheros— está en docs/.
set -uo pipefail
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"; export PATH="$HOME/.cargo/bin:$PATH"

HERE=$(cd "$(dirname "$0")/.." && pwd)
WORK=${VC_BENCH_WORK:-/tmp/vc-bench}
CORP=$WORK/silesia
mkdir -p "$WORK"

echo "== vcompresor bench == $(date -u +%FT%TZ)"
echo "   $(uname -srm) · $(nproc 2>/dev/null || echo '?') hilos"
echo

command -v cargo >/dev/null || { echo "hace falta cargo (rustup.rs). Aborta."; exit 1; }
( cd "$HERE" && cargo build --release -q ) || { echo "el build falló"; exit 1; }
VC="$HERE/target/release/vcompresor"

# ---- corpus público ----------------------------------------------------------------------
if [ ! -d "$CORP" ] || [ -z "$(ls -A "$CORP" 2>/dev/null)" ]; then
  echo "descargando Silesia…"
  mkdir -p "$CORP"
  for u in "https://sun.aei.polsl.pl/~sdeor/corpus/silesia.zip" \
           "http://sun.aei.polsl.pl/~sdeor/corpus/silesia.zip"; do
    if curl -sSL --max-time 900 -o "$WORK/silesia.zip" "$u"; then
      unzip -oq "$WORK/silesia.zip" -d "$CORP" && rm -f "$WORK/silesia.zip" && break
    fi
  done
fi
[ -n "$(ls -A "$CORP" 2>/dev/null)" ] || { echo "no pude obtener Silesia; ponlo en $CORP y reintenta"; exit 1; }

IN=$(du -sb "$CORP" | cut -f1)
echo "corpus: $CORP  ($IN bytes, $(find "$CORP" -type f | wc -l) ficheros)"
echo

TSV=$WORK/resultados.tsv
printf 'herramienta\tbytes\tratio%%\tcomp_s\tdescomp_s\tround_trip\n' > "$TSV"

pct(){ awk "BEGIN{printf \"%.2f\", 100*$1/$IN}"; }
cronometra(){ local s=$(date +%s.%N); "$@" >/dev/null 2>&1; local e=$?; date +%s.%N | awk -v s="$s" '{printf "%.1f", $1-s}'; return $e; }
fila(){ printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$1" "$2" "$(pct "$2")" "$3" "$4" "$5" >> "$TSV"
        printf '  %-20s %12s B  %6s%%  %6ss  %6ss  %s\n' "$1" "$2" "$(pct "$2")" "$3" "$4" "$5"; }
hay(){ command -v "$1" >/dev/null; }

# tar sólido para que los rivales que sólo comen un fichero jueguen en igualdad
TAR=$WORK/corpus.tar
tar -cf "$TAR" -C "$CORP" .

echo "midiendo…"

# ---- nosotros ----------------------------------------------------------------------------
for modo in "" "--max"; do
  nombre="vcompresor${modo:+ $modo}"
  out=$WORK/vc${modo:+-max}.vc; dir=$WORK/vc-out${modo:+-max}
  rm -rf "$out" "$dir"
  tc=$(cronometra "$VC" comprimir "$CORP" -o "$out" $modo)
  td=$(cronometra "$VC" descomprimir "$out" -o "$dir")
  # la prueba de verdad no es el hash que nos damos a nosotros mismos: es diff contra el original
  if diff -rq "$CORP" "$dir" >/dev/null 2>&1; then rt="OK"; else rt="FALLO"; fi
  fila "$nombre" "$(stat -c%s "$out")" "$tc" "$td" "$rt"
  rm -rf "$dir"
done

# ---- rivales -----------------------------------------------------------------------------
if hay zstd; then
  tc=$(cronometra bash -c "zstd -19 --long=31 -T0 -q -f '$TAR' -o '$WORK/c.zst'")
  td=$(cronometra bash -c "zstd -d -q -f '$WORK/c.zst' -o '$WORK/d.tar'")
  rt=OK; cmp -s "$TAR" "$WORK/d.tar" || rt=FALLO; rm -f "$WORK/d.tar"
  fila "zstd -19 --long" "$(stat -c%s "$WORK/c.zst")" "$tc" "$td" "$rt"
fi
if hay xz; then
  tc=$(cronometra bash -c "xz -9 -T0 -k -f -c '$TAR' > '$WORK/c.xz'")
  td=$(cronometra bash -c "xz -d -c '$WORK/c.xz' > '$WORK/d.tar'")
  rt=OK; cmp -s "$TAR" "$WORK/d.tar" || rt=FALLO; rm -f "$WORK/d.tar"
  fila "xz -9" "$(stat -c%s "$WORK/c.xz")" "$tc" "$td" "$rt"
fi
if hay 7z; then
  rm -f "$WORK/c.7z"
  tc=$(cronometra 7z a -mx=9 -bso0 -bsp0 "$WORK/c.7z" "$CORP")
  td=$(cronometra 7z x -y -bso0 -bsp0 -o"$WORK/7z-out" "$WORK/c.7z")
  rm -rf "$WORK/7z-out"
  fila "7z -mx9" "$(stat -c%s "$WORK/c.7z")" "$tc" "$td" "n/d"
fi
if hay zpaq; then
  rm -f "$WORK/c.zpaq"
  tc=$(cronometra zpaq a "$WORK/c.zpaq" "$CORP" -m5)
  td=$(cronometra zpaq x "$WORK/c.zpaq" -to "$WORK/zpaq-out")
  rm -rf "$WORK/zpaq-out"
  fila "zpaq -m5" "$(stat -c%s "$WORK/c.zpaq")" "$tc" "$td" "n/d"
else
  echo "  (zpaq no instalado: es el rival que importa. 'apt install zpaq' y reintenta)"
fi

echo
echo "tabla completa: $TSV"
echo
echo "Nota honesta: Silesia son doce ficheros sueltos y sin repeticiones entre ellos, así que"
echo "aquí no se aprovecha la deduplicación ni el reordenado por parecido, que es donde"
echo "vcompresor saca la mayor diferencia en datos reales. Los números sobre árboles de"
echo "miles de ficheros están en docs/, con el corpus descrito y las derrotas incluidas."
