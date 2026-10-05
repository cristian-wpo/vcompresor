#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""EL COSTE DE UN FICHERO POR EXISTIR, separado del coste de sus bytes.

Es la medida que decidia si arreglar syscalls era rentable o cosmetica, y contesta que DEPENDE
DEL TAMAÑO — que era justo lo que no se sabia.

    tiempo = 49,8 us por fichero  +  5,393 ns por byte     (carril de TEXTO)

    fichero MEDIANO del arbol (1.972 B)    49,8 + 10,6 = 60,4 us    82% es por fichero
    fichero MEDIO   del arbol (60.530 B)   49,8 + 326  = 376  us    13% es por fichero

CORRIGE UNA COSA QUE DIJE Y ERA FALSA. Por la tarde afirme que seis stat (~12 us) eran «el
0,08% del coste de un fichero» y que arreglarlos era estetica. Divide por 15 ms, que es la
MEDIA de CPU por fichero del arbol — dominada por los ficheros enormes. Para el fichero MEDIANO,
que es la mitad del arbol, el total son 60,4 us y seis stat son el 20%. Me equivoque por un
factor de 250, y en la direccion que hacia parecer inutil un arreglo que si valia.

Es exactamente el error que bench/tallas.tsv existe para evitar —«se midio sobre ficheros de
642 KB de media y el arbol real tiene MEDIANA de 1.972 B»— cometido por quien escribio esa
tabla, el mismo dia.

EL LIMITE DE ESTA MEDIDA, dicho antes de que alguien la extrapole. El modelo predice 16,3 s
para el arbol real y los medidos son ~306 s: falla 19 veces. Motivo: los arboles sinteticos son
todos texto y el coste del arbol real lo dominan los carriles JXL y preflate, que aqui no se
ejercitan. Asi que 49,8 us/fichero vale para EL CARRIL DE TEXTO y no para el arbol entero.
Para el arbol entero hace falta repetir esto con un corpus mezclado.

Y UN FALLO DE DISEÑO PROPIO: el arbol de 2 KiB se capo a 40.000 ficheros (78,1 MiB) mientras
los demas llevaban 130,1 MiB, asi que la premisa «los mismos bytes en todos» no se cumplio del
todo. La recta se ajusta con dos variables y lo absorbe, pero conviene saberlo.
"""
import csv

# talla · ficheros · MiB · seg · Gciclos — medido en el servidor, dos vueltas, carga 6,8
D = [
    ("2 KiB",     40000,  78.1, 2.46,  9.9),
    ("16 KiB",     8327, 130.1, 1.02, 10.6),
    ("128 KiB",    1041, 130.1, 0.76, 10.0),
    ("1 MiB",       131, 130.1, 0.85, 10.7),
    ("8 MiB",        17, 130.1, 0.77, 10.1),
]
A_US, B_NS = 49.8, 5.393
MEDIANA, MEDIA = 1972, 60530

with open("bench/talla-coste.tsv", "w", newline="") as f:
    f.write("# EL COSTE DE UN FICHERO POR EXISTIR, separado del coste de sus bytes.\n")
    f.write("# tiempo = 49,8 us por fichero + 5,393 ns por byte, en el CARRIL DE TEXTO.\n")
    f.write("# OJO: el modelo predice 16,3 s para el arbol real y los medidos son ~306 s. Falla\n")
    f.write("# 19x porque estos arboles son todos texto y el arbol real lo dominan JXL y preflate.\n")
    f.write("# Vale para el carril de texto; para el arbol entero hace falta un corpus mezclado.\n")
    f.write("# Lo escribe bench/talla-tabla.py con los datos de bench/talla-coste.sh.\n")
    w = csv.writer(f, delimiter="\t")
    w.writerow(["talla", "ficheros", "MiB", "seg", "Gciclos", "us_por_fichero", "MB_s"])
    for t, n, mib, s, g in D:
        w.writerow([t, n, f"{mib:.1f}", f"{s:.2f}", f"{g:.1f}",
                    f"{s*1e6/n:.1f}", f"{mib/s:.1f}"])
    w.writerow([])
    w.writerow(["derivado", "valor", "", "", "", "", "nota"])
    w.writerow(["us por fichero", f"{A_US}", "", "", "", "",
                "lo que cuesta un fichero SOLO por existir"])
    w.writerow(["ns por byte", f"{B_NS}", "", "", "", "", "lo que cuesta su contenido"])
    for n, t in (("mediano", MEDIANA), ("medio", MEDIA)):
        by = t * B_NS / 1000
        w.writerow([f"fichero {n} ({t} B)", f"{A_US+by:.1f} us", "", "", "", "",
                    f"el {100*A_US/(A_US+by):.0f}% del coste es por fichero"])

print("bench/talla-coste.tsv escrito\n")
print(f"  {'talla':>10} {'ficheros':>9} {'us/fich':>9} {'MB/s':>8}")
for t, n, mib, s, g in D:
    print(f"  {t:>10} {n:9d} {s*1e6/n:9.1f} {mib/s:8.1f}")
print()
print(f"  tiempo = {A_US} us por fichero + {B_NS} ns por byte  (carril de TEXTO)")
for n, t in (("MEDIANO", MEDIANA), ("MEDIO", MEDIA)):
    by = t * B_NS / 1000
    print(f"    fichero {n:<8} ({t:6d} B): {A_US+by:7.1f} us · "
          f"{100*A_US/(A_US+by):3.0f}% es por fichero")
print()
print("  CORRECCION: dije que seis stat eran el 0,08% del coste de un fichero, dividiendo")
print(f"  por la MEDIA (15 ms). Para el fichero MEDIANO son el {100*12/(A_US+MEDIANA*B_NS/1000):.0f}%. Factor 250 de error.")
