# -*- coding: utf-8 -*-
"""EL DETECTOR, MEDIDO ANTES DE CONSTRUIRLO (P16 del camino).

La pregunta: ¿hay en el arbol real de 14.433 ficheros ALGO donde transponer por ancho de registro
gane? Si la respuesta es CERO, la idea se cierra con un numero en vez de con una opinion. Si es un
3%, hay carril que escribir y se sabe cuanto compra.

LA SONDA es zstd -3 sobre una muestra de 256 KB. Es barata a proposito -un detector real no puede
gastar mas- y SE VALIDA CONTRA EL CONTROL POSITIVO antes de usarla: si la sonda no ve la ganancia
en el array de enteros, es ciega y su cero no significa nada.
"""
import subprocess, sys, os, glob, collections

MUESTRA = 256 * 1024

def zst(b):
    return len(subprocess.run(["zstd","-3","-c","-T1"], input=b, capture_output=True).stdout)

def transpone(b, n):
    m = (len(b) // n) * n
    o = bytearray(len(b)); k = 0
    for j in range(n):
        o[k:k+m//n] = b[j:m:n]; k += m//n
    o[k:] = b[m:]
    return bytes(o)

def prueba(b):
    base = zst(b)
    mejor, w = base, 0
    for n in (2, 4, 8, 16):
        c = zst(transpone(b, n))
        if c < mejor: mejor, w = c, n
    return base, mejor, w

if sys.argv[1] == "valida":
    for f in sorted(glob.glob("/tmp/entrelaza/control/*.bin")):
        b = open(f,"rb").read()[:MUESTRA]
        base, mejor, w = prueba(b)
        print(f"  {os.path.basename(f):<16} {base:>9} -> {mejor:>9}  w={w}  {100*(mejor-base)/base:+7.2f}%")
    sys.exit(0)

raiz = sys.argv[1]
fs = [p for p in glob.glob(os.path.join(raiz,"**","*"), recursive=True)
      if os.path.isfile(p) and os.path.getsize(p) >= 64*1024]
fs.sort()
paso = max(1, len(fs)//int(sys.argv[2])) if len(sys.argv)>2 else 1
fs = fs[::paso]
gana = collections.Counter(); ahorro = 0; total = 0; n = 0
for p in fs:
    try: b = open(p,"rb").read(MUESTRA)
    except Exception: continue
    if len(b) < 64*1024: continue
    base, mejor, w = prueba(b)
    n += 1; total += base; ahorro += base - mejor
    gana[w] += 1
print(f"\n  {n} ficheros de >=64 KB muestreados de {raiz}\n")
for w in sorted(gana):
    et = "NINGUNO gana (se queda el original)" if w==0 else f"gana ancho {w}"
    print(f"    {gana[w]:>6}  {100*gana[w]/max(n,1):>5.1f}%   {et}")
g = n - gana[0]
print(f"\n  transponer gana en {g} de {n} ({100*g/max(n,1):.2f}%)")
print(f"  y sobre el TOTAL de las muestras ahorraria {100*ahorro/max(total,1):.3f}%")
