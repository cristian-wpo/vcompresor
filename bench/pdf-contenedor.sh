#!/usr/bin/env bash
# ¿VALE LA PENA ABRIR EL PDF COMO CONTENEDOR? RESPUESTA MEDIDA: NO. Pierde un 6,96%.
#
# DE DONDE VENIA LA IDEA. El censo de contenedores (bench/contenedores.py) dijo que abrimos TRES
# —gzip, png, zip— y que el PDF no es uno de ellos, con un 54,5% de DEFLATE sin tocar en la
# entrada mas grande del banco. Sonaba a dinero tirado: dentro hay DEFLATE y no lo miramos.
#
# LO QUE HAY DE VERDAD, medido sobre corpus-mac/pdfoffice (49 PDF, 42,0 MB):
#     bytes de PDF               42.034.694
#     dentro, FlateDecode        14.786.643    35,18%
#     flujos que inflan bien          1.404
#     flujos que NO inflan               12
# (El 54,5% era de otro PDF, el mas grande del banco. Aqui es 35,18%: el numero depende del PDF,
# asi que citarlo sin decir cual es media verdad.)
#
# Y LO QUE PASA AL ABRIRLOS —esta prueba— es lo contrario de lo que se esperaba:
#     tal cual     29.199.827 B
#     abierto      31.232.182 B     +6,960%
# ABRIRLOS HACE EL ARCHIVO MAS GRANDE. Y esto es la COTA SUPERIOR de lo bueno que puede salir:
# aqui los flujos se inflan y ya esta, SIN pagar las correcciones que preflate necesitaria para
# poder recomponerlos byte a byte. Con ellas seria peor todavia.
#
# POR QUE, y es lo unico que hace util un negativo. La expansion:
#     PDF     42,0 MB -> 1,2 GB     28x
#     PNG      200 MiB -> 1.452 MiB  7,2x   <- aqui el carril SI paga
# Estos PDF son catalogos ESCANEADOS: dentro llevan mapas de bits que DEFLATE ya comprime bien, y
# al inflarlos salen gigabytes de imagen que zstd comprime PEOR de lo que estaban. El carril
# preflate gana cuando debajo del DEFLATE hay TEXTO; aqui debajo hay pixeles.
#
# LA REGLA QUE SALE: «hay DEFLATE dentro» no es motivo suficiente para abrirlo. Lo que decide es
# QUE HAY DEBAJO del DEFLATE, y eso se mide con la expansion. Por encima de ~10x, sospechar.
#
# ⚠️ LO QUE ESTA PRUEBA **NO** DESCARTA: una implementacion de verdad mandaria el plano a su
# PROPIO CARRIL y lo dedupicaria solo contra otros planos, que es lo que hace el carril preflate
# —y la nota de vc-core dice que sin eso «las ganancias se desploman a -0,173% y +0,118%»—. Esto
# infla en el sitio, que es mas bruto. Pero tendria que remontar un +6,96% y una expansion de 28x,
# y eso ya no es «coger dinero tirado»: es un proyecto con la carga de la prueba en contra.
#
# BYTES = determinista.
set -uo pipefail
cd "$(dirname "$0")/.."
. bench/comun.sh

IN=${IN:?define IN con la carpeta de PDF/Office a medir}
VC=$PWD/target/release/vcompresor
W=$(mktemp -d /tmp/pdf.XXXXXX); trap 'rm -rf "$W"' EXIT
mkdir -p "$W/tal" "$W/abierto"
[ -d "$IN" ] || { echo "no esta $IN"; exit 3; }

cab "¿VALE LA PENA ABRIR EL PDF COMO CONTENEDOR?"

i=0
while IFS= read -r -d '' f; do
  i=$((i+1)); cp "$f" "$W/tal/$(printf '%04d' $i).pdf"
done < <(find "$IN" -name "*.pdf" -type f -print0 | sort -z)
[ "$i" -gt 0 ] || { echo "  no hay PDF en $IN"; exit 3; }
echo "  $i PDF · $(du -sh "$W/tal" | cut -f1)"
echo

python3 - "$W/tal" "$W/abierto" <<'PY'
import os, re, sys, zlib
ent, sal = sys.argv[1], sys.argv[2]
nfl = mal = tot = pla = defl = 0
for f in sorted(os.listdir(ent)):
    b = open(os.path.join(ent, f), "rb").read(); tot += len(b)
    out, pos = bytearray(), 0
    for m in re.finditer(rb"/Filter\s*/FlateDecode.*?stream\r?\n", b, re.S):
        ini = m.end(); fin = b.find(b"endstream", ini)
        if fin < 0: continue
        cuerpo = b[ini:fin].rstrip(b"\r\n")
        if len(cuerpo) < 16: continue
        try: plano = zlib.decompress(cuerpo)
        except Exception: mal += 1; continue
        out += b[pos:ini]; out += plano; pos = fin
        nfl += 1; defl += len(cuerpo); pla += len(plano)
    out += b[pos:]
    open(os.path.join(sal, f), "wb").write(bytes(out))
print(f"  bytes de PDF          {tot:>14,}")
print(f"  dentro, FlateDecode   {defl:>14,}   {100*defl/max(tot,1):.2f}%")
print(f"  flujos que inflan     {nfl:>14,}")
print(f"  flujos que NO inflan  {mal:>14,}")
print(f"  el plano ocupa        {pla:>14,}   x{pla/max(defl,1):.1f} sobre el DEFLATE")
print()
print("  ⚠️ LA EXPANSION ES LO QUE DECIDE. El carril preflate paga cuando debajo del DEFLATE hay")
print("     TEXTO -en PNG la expansion es 7,2x-. Por encima de ~10x, debajo suele haber pixeles.")
PY
echo

for m in tal abierto; do
  "$VC" compress "$W/$m" -o "$W/$m.vc" --level 9 >/dev/null 2>&1
done
A=$(talla "$W/tal.vc"); B=$(talla "$W/abierto.vc")
distingue_o_aborta "$A" "$B" "abrir y no abrir el PDF" || exit 1
python3 -c "
a,b=$A,$B
print(f'  {\"tal cual\":<12} {a:>14,} B')
print(f'  {\"abierto\":<12} {b:>14,} B   {100*(b-a)/a:+.3f}%')
print()
if b > a:
    print('  ABRIRLOS HACE EL ARCHIVO MAS GRANDE, y esto es la COTA SUPERIOR: aqui no se pagan las')
    print('  correcciones que preflate necesitaria para recomponerlos byte a byte. Con ellas, peor.')
else:
    print('  Abrirlos compra bytes. Antes de creerselo hay que restar las correcciones de preflate.')"
echo
echo "FIN-PDF"
