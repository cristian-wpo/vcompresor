#!/usr/bin/env python3
"""Transformada columnar reversible: pone juntos los bytes que se parecen.

    nombre,edad,ciudad          nombre  Ana   Luis
    Ana,31,León          ->     edad    31    28
    Luis,28,León                ciudad  León  León

La idea es la misma que ya nos dio la mayor ventaja del proyecto con DEFLATE y JPEG: no
modelar mejor, sino DESHACER LA ESTRUCTURA antes de modelar. Un modelo de contextos predice
el byte siguiente mirando los anteriores; si los anteriores son de otra columna, no dicen
nada. Puestos en columna, cada byte tiene al lado exactamente el que más se le parece.

Biyectiva por construcción, y esto es lo que la hace segura:
  - se parte por '\\n' primero, así que ningún campo puede contener un salto de línea;
  - se parte por el separador, así que ningún campo puede contenerlo;
  - luego cada campo puede terminarse con '\\n' en el flujo de columna SIN ambigüedad;
  - y se guarda cuántos campos tenía cada fila, así que las filas irregulares -y las
    comillas con separadores dentro, que aquí simplemente parten de más- vuelven igual.

No hace falta entender CSV. Sólo hace falta que la vuelta sea exacta, y eso se comprueba.
"""
import sys, collections

def varint(n):
    b = bytearray()
    while True:
        x = n & 0x7F; n >>= 7
        b.append(x | (0x80 if n else 0))
        if not n: return bytes(b)

def leer_varint(d, i):
    n = 0; s = 0
    while True:
        x = d[i]; i += 1
        n |= (x & 0x7F) << s
        if not (x & 0x80): return n, i
        s += 7

def transponer(datos, sep):
    lineas = datos.split(b'\n')
    campos = [l.split(sep) for l in lineas]
    cuentas = [len(c) for c in campos]
    maxc = max(cuentas) if cuentas else 0
    out = bytearray()
    out.append(sep)
    out += varint(len(lineas))
    for c in cuentas: out += varint(c)
    for c in range(maxc):
        for r in range(len(lineas)):
            if cuentas[r] > c:
                out += campos[r][c]
                out.append(0x0A)
    return bytes(out)

def destransponer(d):
    sep = d[0]; i = 1
    nf, i = leer_varint(d, i)
    cuentas = []
    for _ in range(nf):
        c, i = leer_varint(d, i); cuentas.append(c)
    maxc = max(cuentas) if cuentas else 0
    campos = [[None] * cuentas[r] for r in range(nf)]
    cuerpo = d[i:]
    pos = 0
    for c in range(maxc):
        for r in range(nf):
            if cuentas[r] > c:
                fin = cuerpo.index(b'\n', pos)
                campos[r][c] = cuerpo[pos:fin]
                pos = fin + 1
    return b'\n'.join(bytes(sep).join(f) for f in campos)

def mejor_separador(datos):
    """El separador es el byte no alfanumérico que aparece un número PARECIDO de veces en
    cada línea. Lo más frecuente no vale: en un volcado SQL la coma gana siempre y no es
    quien separa registros."""
    muestra = datos[:1 << 20]
    lineas = [l for l in muestra.split(b'\n')[1:-1] if l][:2000]
    if len(lineas) < 20: return None
    mejor, mejor_p = None, 0.0
    for sep in [b',', b'\t', b';', b'|', b' ']:
        cs = [l.count(sep) for l in lineas]
        if not cs: continue
        moda = collections.Counter(cs).most_common(1)[0]
        if moda[0] == 0: continue
        # qué fracción de líneas tiene EXACTAMENTE ese número de separadores
        p = moda[1] / len(cs)
        if p > mejor_p: mejor, mejor_p = sep[0], p
    return (mejor, mejor_p) if mejor_p >= 0.5 else (None, mejor_p)

if __name__ == "__main__":
    datos = open(sys.argv[1], "rb").read()
    sep, conf = mejor_separador(datos)
    if sep is None:
        print(f"{sys.argv[1]}: sin separador claro (mejor confianza {conf:.2f})")
        sys.exit(1)
    t = transponer(datos, sep)
    v = destransponer(t)
    ok = v == datos
    print(f"{sys.argv[1]}: sep={chr(sep)!r} confianza={conf:.2f} "
          f"orig={len(datos)} transp={len(t)} reversible={'SI' if ok else '**NO**'}")
    if not ok: sys.exit(2)
    open(sys.argv[2], "wb").write(t)
