# -*- coding: utf-8 -*-
"""¿QUE CONTENEDORES ABRIMOS, Y CUANTO NOS ESTAMOS DEJANDO SIN ABRIR?

Lo pregunta el dueño: «ya existen formatos y tipos con sus indices y todo lo que necesitamos
listos para simplemente traerlos y comprimirlos sin volverlos a leer».

LO QUE HAY HOY. `deflate::firma()` reconoce TRES formatos —Gzip, Png, Zip— y todo lo demas cae
en el `_ => None`. O sea que un PDF es OPACO: va al carril media, se comprime con zstd nivel 3, y
sus flujos FlateDecode internos -DEFLATE del mismo que vdeflate ya sabe deshacer- no se tocan
nunca. En el banco `corpus/pdfoffice` son 76,5 MB, la entrada mas grande de la tabla de rivales.

ESTA PRUEBA NO OPINA: cuenta. Recorre los PDF de verdad del corpus, encuentra sus flujos
comprimidos y mide QUE FRACCION DE CADA FICHERO son. Si son el 80%, hay un carril entero sin
explotar. Si son el 15%, se cierra y se dice.

⚠️ Y MIDE TAMBIEN LO QUE NO SE PUEDE ABRIR. Un flujo con un filtro que no es FlateDecode -DCTDecode
es un JPEG, JPXDecode es JPEG2000, CCITTFaxDecode es fax- no lo desharia vdeflate. Contarlos todos
juntos daria una cifra optimista, que es como se justifica un carril que luego no rinde.
"""
import os, re, sys, zlib, collections

RAIZ = sys.argv[1] if len(sys.argv) > 1 else "/tmp/compresor-f1/corpus"
FILTRO = re.compile(rb"/Filter\s*(?:/|\[)([^>\]]{0,120})")

def pdfs(raiz, tope=200):
    for d, _, fs in os.walk(raiz):
        for f in sorted(fs):
            if f.lower().endswith(".pdf"):
                yield os.path.join(d, f)
                tope -= 1
                if tope <= 0: return

tot_fich = tot_bytes = 0
flate_bytes = otros_bytes = 0
n_flate = n_otros = n_rotos = 0
filtros = collections.Counter()
por_fichero = []

for p in pdfs(RAIZ):
    try: d = open(p, "rb").read()
    except OSError: continue
    if not d.startswith(b"%PDF"): continue
    tot_fich += 1; tot_bytes += len(d)
    mio_flate = 0
    pos = 0
    while True:
        i = d.find(b"stream", pos)
        if i < 0: break
        # El objeto que precede al `stream` lleva su /Filter. Se mira hacia atras un poco.
        cab = d[max(0, i - 400):i]
        m = FILTRO.search(cab)
        nom = (m.group(1).split(b"/")[0].strip().decode("latin1", "replace")
               if m else "(sin filtro)")
        filtros[nom] += 1
        j = i + 6
        while j < len(d) and d[j] in (13, 10): j += 1
        k = d.find(b"endstream", j)
        if k < 0: break
        cuerpo = d[j:k]
        pos = k + 9
        if "Flate" in nom:
            # Se comprueba que de verdad se descomprime: un /Filter que dice Flate y no
            # descomprime no es un flujo aprovechable, es un fichero raro.
            try:
                zlib.decompress(cuerpo)
                n_flate += 1; flate_bytes += len(cuerpo); mio_flate += len(cuerpo)
            except zlib.error:
                n_rotos += 1
        else:
            n_otros += 1; otros_bytes += len(cuerpo)
    if len(d): por_fichero.append(100.0 * mio_flate / len(d))

if not tot_fich:
    print(f"  no hay PDF en {RAIZ}"); sys.exit(0)

por_fichero.sort()
med = por_fichero[len(por_fichero)//2]
print(f"\n  {tot_fich} PDF · {tot_bytes/2**20:.1f} MiB en total\n")
print(f"  DEFLATE que vdeflate SI podria deshacer   {flate_bytes/2**20:>8.1f} MiB "
      f"({100*flate_bytes/tot_bytes:>5.1f}% del total)  {n_flate} flujos")
print(f"  flujos con OTRO filtro (JPEG, fax, JPX…)  {otros_bytes/2**20:>8.1f} MiB "
      f"({100*otros_bytes/tot_bytes:>5.1f}%)  {n_otros} flujos")
if n_rotos:
    print(f"  ⚠️ {n_rotos} flujos dicen Flate y NO descomprimen: no cuentan.")
print(f"\n  por fichero, la mediana es {med:.1f}% de DEFLATE "
      f"(el peor {por_fichero[0]:.1f}%, el mejor {por_fichero[-1]:.1f}%)")
print("\n  los filtros que aparecen:")
for f, n in filtros.most_common(8):
    print(f"    {n:>5}  {f}")
print()
print("  LA REGLA DE LECTURA: solo cuenta la primera fila. Contar todos los flujos juntos daria")
print("  una cifra optimista —un DCTDecode es un JPEG y ese ya lo trata vc-jpeg, no vdeflate—")
print("  y asi es como se justifica un carril que luego no rinde.")
