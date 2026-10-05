#!/usr/bin/env python3
"""Transformada de tuplas SQL: poner en columna lo que el volcado escribe en fila.

sql.10m es 53,8% dígitos decimales metidos en tuplas de INSERT:

    INSERT INTO `ps_order_detail` VALUES (24930,9202,0,0,1,1672,0,0,'G09B Taco...',4,-376,
      0,0,0,12.000000,0.00,0.000000,0.000000),(13178,4458,0,0,1,1682,...),(...)

Un modelo de contextos que lee eso de izquierda a derecha ve, antes de cada número, los
bytes de OTRAS columnas. La columna 15 siempre vale 12.000000 y la 17 siempre 0.000000,
pero el modelo no las ve seguidas: las ve separadas por una descripción de 80 caracteres.

Puestas en columna, cada valor tiene al lado el mismo campo de la tupla anterior.

Se mide en dos escalones, para saber qué aporta cada uno:
  A) sólo transponer
  B) transponer + delta en las columnas que son enteros (los IDs van casi ordenados)

Y la regla que no se negocia: se transforma, se DESHACE, y se compara byte a byte. Si no
vuelve exacto, no se usa — se guarda el original.
"""
import sys, subprocess, os

def partir_campos(cuerpo):
    """Parte '(a,b,'x,y'),(c,d,'z')' en [[a,b,'x,y'],[c,d,'z']] respetando comillas SQL."""
    tuplas, campo, actual = [], bytearray(), []
    i, n = 0, len(cuerpo)
    dentro = False
    prof = 0
    while i < n:
        c = cuerpo[i]
        if dentro:
            campo.append(c)
            if c == 0x5C and i + 1 < n:           # \  escapa el siguiente
                i += 1; campo.append(cuerpo[i])
            elif c == 0x27:                        # '  cierra... salvo que venga otra
                if i + 1 < n and cuerpo[i + 1] == 0x27:
                    i += 1; campo.append(cuerpo[i])
                else:
                    dentro = False
            i += 1; continue
        if c == 0x27:
            dentro = True; campo.append(c); i += 1; continue
        if c == 0x28 and prof == 0:                # (
            prof = 1; campo = bytearray(); actual = []; i += 1; continue
        if c == 0x29 and prof == 1:                # )
            actual.append(bytes(campo)); tuplas.append(actual)
            prof = 0; campo = bytearray(); i += 1; continue
        if c == 0x2C and prof == 1:                # , separando campos
            actual.append(bytes(campo)); campo = bytearray(); i += 1; continue
        if prof == 1:
            campo.append(c)
        i += 1
    return tuplas

def rehacer(tuplas, sep=b"),("):
    return b"(" + sep.join(b",".join(t) for t in tuplas) + b")"

def transponer_sql(datos):
    """Devuelve (cabecera, flujo_columnar) o None si no parece un volcado de tuplas."""
    marca = b" VALUES ("
    salida = bytearray()
    resto = bytearray()
    pos = 0
    bloques = 0
    while True:
        k = datos.find(marca, pos)
        if k < 0:
            resto += datos[pos:]
            break
        # el final del bloque de tuplas es el ';' que cierra la sentencia
        fin = datos.find(b";", k)
        if fin < 0:
            resto += datos[pos:]
            break
        cabecera = datos[pos:k + len(marca) - 1]
        cuerpo = datos[k + len(marca) - 1:fin]
        tuplas = partir_campos(cuerpo)
        if len(tuplas) < 4 or rehacer(tuplas) != cuerpo:
            resto += datos[pos:fin + 1]; pos = fin + 1; continue
        anchos = set(len(t) for t in tuplas)
        if len(anchos) != 1:
            resto += datos[pos:fin + 1]; pos = fin + 1; continue
        ncol = anchos.pop()
        bloques += 1
        resto += cabecera
        resto += b"\x00BLOQUE%d:%d\x00" % (len(tuplas), ncol)
        for c in range(ncol):
            for t in tuplas:
                salida += t[c]; salida.append(0x0A)
        resto += b";"
        pos = fin + 1
    return (bytes(resto), bytes(salida), bloques) if bloques else None

def medir(vc, ruta):
    r = subprocess.run([vc, "c", ruta, ruta + ".vc"], capture_output=True)
    return os.path.getsize(ruta + ".vc")

if __name__ == "__main__":
    vc = sys.argv[1]; ruta = sys.argv[2]
    datos = open(ruta, "rb").read()
    r = transponer_sql(datos)
    if not r:
        print("no se reconocieron bloques de tuplas"); sys.exit(1)
    cab, col, nb = r
    print(f"bloques de tuplas reconocidos: {nb}")
    print(f"  original    {len(datos):>10}")
    print(f"  cabeceras   {len(cab):>10}")
    print(f"  columnar    {len(col):>10}   (suma {len(cab)+len(col)}, "
          f"{100*(len(cab)+len(col))/len(datos):.2f}% del original)")
    open("/tmp/sqlA-cab", "wb").write(cab)
    open("/tmp/sqlA-col", "wb").write(col)
    a = medir(vc, ruta)
    b = medir(vc, "/tmp/sqlA-cab") + medir(vc, "/tmp/sqlA-col")
    print(f"\n  vc-cm sobre el original      {a:>9}")
    print(f"  vc-cm sobre cabecera+columnas {b:>9}   {100*(b-a)/a:+.2f}%")
