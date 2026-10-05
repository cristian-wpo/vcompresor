# -*- coding: utf-8 -*-
"""EL FILTRO `shuffle`: transponer DENTRO del fichero por ancho de registro.

Es la version de la idea del dueño que SI usa la industria (Blosc, HDF5, Parquet): si el dato
son registros de N bytes -un array de enteros de 4, de flotantes de 8-, poner juntos todos los
bytes 0, luego todos los bytes 1, etc. deja los bytes ALTOS -que casi no cambian- en una racha
larga de ceros, y los bajos -que son ruido- aparte.

Contra entrelazar FICHEROS, esto tiene la premisa que aquello no tenia: el registro N garantiza
que la posicion i%N es siempre el mismo campo. Entre ficheros de texto no hay tal alineacion, y
por eso salio +71% a +140%.
"""
import sys, os
d, out = sys.argv[1], sys.argv[2]
b = open(d, "rb").read()
os.makedirs(out, exist_ok=True)
open(f"{out}/w00-original.bin", "wb").write(b)
for n in (2, 4, 8, 16):
    o = bytearray(len(b))
    m = (len(b) // n) * n
    k = 0
    for j in range(n):
        for i in range(j, m, n):
            o[k] = b[i]; k += 1
    o[k:] = b[m:]
    open(f"{out}/w{n:02d}-shuffle.bin", "wb").write(bytes(o))
print(f"  {os.path.basename(d)} · {len(b):,} B".replace(",", "."))
