#!/usr/bin/env python3
"""Delta en los enteros de las tuplas SQL, SIN mover los bytes de sitio.

La transposición salió un 11% PEOR y ahora se entiende: en un volcado las tuplas enteras se
repiten casi iguales, el match model engancha bloques largos y los predice casi gratis.
Transponer los parte en veinte flujos cortos y mata eso. Además el modelo ya explota las
columnas por otra vía —columna x sep tiene el 46,4% de ocupación, la mejor de las 26— sin
renunciar a la vista por filas.

Así que esto prueba la OTRA idea, la que no toca el orden: sustituir cada entero por su
diferencia con el mismo campo de la tupla anterior, en zigzag decimal. La fila sigue siendo
una fila, el match model sigue viendo tuplas enteras, y los IDs -que en un volcado van casi
ordenados- pasan de cinco dígitos a uno.

    (24930,9202,0,0,1,...)      ->  (24930,9202,0,0,1,...)
    (24931,9203,0,0,1,...)      ->  (~2,~2,~0,~0,~0,...)

Reversible por construcción: sólo se toca un campo si es un entero decimal canónico (sin
ceros a la izquierda, sin signo +), y el resultado se marca con un prefijo que no puede
aparecer en un entero. Y aun así se deshace y se compara byte a byte antes de aceptarlo.
"""
import sys, subprocess, os
sys.path.insert(0, "/tmp")
from sqltuplas import partir_campos, rehacer

MARCA = 0x7E  # '~' no aparece nunca al principio de un entero decimal

def es_entero(b):
    if not b: return False
    s = b[1:] if b[:1] == b"-" else b
    if not s or not s.isdigit(): return False
    if len(s) > 1 and s[:1] == b"0": return False      # 007 no es canónico
    if b[:1] == b"-" and s == b"0": return False        # -0 tampoco
    return True

def zigzag(n):  return (n << 1) ^ (n >> 63) if n >= 0 else ((-n) << 1) - 1

def deszigzag(u): return u >> 1 if not (u & 1) else -((u + 1) >> 1)

def delta_bloque(tuplas):
    ncol = len(tuplas[0])
    # una columna se delta-ea sólo si TODOS sus valores son enteros canónicos
    deltear = [all(es_entero(t[c]) for t in tuplas) for c in range(ncol)]
    salida = []
    for r, t in enumerate(tuplas):
        fila = []
        for c in range(ncol):
            if deltear[c] and r > 0:
                d = int(t[c]) - int(tuplas[r - 1][c])
                fila.append(b"~" + str(zigzag(d)).encode())
            else:
                fila.append(t[c])
        salida.append(fila)
    return salida, deltear

def deshacer_bloque(tuplas):
    ncol = len(tuplas[0])
    salida = []
    for r, t in enumerate(tuplas):
        fila = []
        for c in range(ncol):
            if t[c][:1] == b"~":
                d = deszigzag(int(t[c][1:]))
                fila.append(str(int(salida[r - 1][c]) + d).encode())
            else:
                fila.append(t[c])
        salida.append(fila)
    return salida

def transformar(datos):
    marca = b" VALUES ("
    out = bytearray(); pos = 0; nb = 0; ntup = 0
    while True:
        k = datos.find(marca, pos)
        if k < 0: out += datos[pos:]; break
        fin = datos.find(b";", k)
        if fin < 0: out += datos[pos:]; break
        cab = datos[pos:k + len(marca) - 1]
        cuerpo = datos[k + len(marca) - 1:fin]
        tup = partir_campos(cuerpo)
        if len(tup) < 4 or rehacer(tup) != cuerpo or len(set(len(t) for t in tup)) != 1:
            out += datos[pos:fin + 1]; pos = fin + 1; continue
        nuevas, _ = delta_bloque(tup)
        if deshacer_bloque(nuevas) != tup:          # la regla que no se negocia
            out += datos[pos:fin + 1]; pos = fin + 1; continue
        out += cab; out += rehacer(nuevas); out += b";"
        nb += 1; ntup += len(tup); pos = fin + 1
    return bytes(out), nb, ntup

if __name__ == "__main__":
    vc, ruta = sys.argv[1], sys.argv[2]
    d = open(ruta, "rb").read()
    t, nb, ntup = transformar(d)
    print(f"bloques {nb}, tuplas {ntup}")
    print(f"  original    {len(d):>9}")
    print(f"  con delta   {len(t):>9}   {100*(len(t)-len(d))/len(d):+.2f}% en crudo")
    open("/tmp/sqlD", "wb").write(t)
    subprocess.run([vc, "c", ruta, "/tmp/od1"], capture_output=True)
    subprocess.run([vc, "c", "/tmp/sqlD", "/tmp/od2"], capture_output=True)
    a, b = os.path.getsize("/tmp/od1"), os.path.getsize("/tmp/od2")
    print(f"\n  vc-cm original  {a:>8}")
    print(f"  vc-cm delta     {b:>8}   {100*(b-a)/a:+.2f}%")
