#!/usr/bin/env bash
# MEDICIÓN ÚNICA: los cinco corpus que nunca se han enfrentado a paq8px.
#
# El resultado de un rival sobre una entrada dada es una CONSTANTE, así que esto se corre
# UNA VEZ y se cachea en bench/rivales.tsv. Lo único que se re-mide en cada iteración es lo
# nuestro.
#
# El intento anterior se rompió con nombres de fichero con espacios (`while read f`) y no
# encontraba la salida de paq8px. Aquí: -print0 con read -d '' y búsqueda por patrón.
set -u
C=/tmp/compresor-f1/corpus
W=/tmp/medu
VC=/tmp/vcz-sw
PX=/tmp/compresor-f1/bin/tools/paq8px

printf '%-11s %12s %12s %12s %12s\n' corpus entrada nosotros zpaq-m5 paq8px-8
for d in png pdfoffice dup bin jpg; do
  rm -rf "$W"; mkdir -p "$W/x"
  # rebanada de ~20 MB en ficheros ENTEROS, con nombres raros incluidos
  t=0
  while IFS= read -r -d '' f; do
    s=$(stat -c%s "$f" 2>/dev/null) || continue
    t=$((t + s)); [ "$t" -gt 20971520 ] && break
    rel=${f#"$C/$d/"}
    mkdir -p "$W/x/$(dirname "$rel")" 2>/dev/null
    cp "$f" "$W/x/$rel" 2>/dev/null || true
  done < <(find "$C/$d" -type f -print0)

  IN=$(du -sb "$W/x" | cut -f1)

  nice -n 15 "$VC" comprimir "$W/x" -o "$W/a.vc" --max >/dev/null 2>&1
  A=$(stat -c%s "$W/a.vc" 2>/dev/null || echo 0)

  nice -n 15 zpaq a "$W/a.zpaq" "$W/x" -m5 >/dev/null 2>&1
  Z=$(stat -c%s "$W/a.zpaq" 2>/dev/null || echo 0)

  tar -cf "$W/a.tar" -C "$W/x" .
  rm -f "$W"/a.tar.paq8px*
  nice -n 15 "$PX" -8 "$W/a.tar" >/dev/null 2>&1
  P=$(stat -c%s "$W"/a.tar.paq8px* 2>/dev/null | head -1 || echo 0)
  [ -z "$P" ] && P=0

  printf '%-11s %12s %12s %12s %12s\n' "$d" "$IN" "$A" "$Z" "$P"
done
rm -rf "$W"
echo FIN-CENTINELA
