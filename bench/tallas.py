# -*- coding: utf-8 -*-
"""LA TALLA DE LOS FICHEROS: por que el banco no podia ver nada de esto.

El corpus del banco con el que se midieron las velocidades tiene 656 ficheros de 642 KB de
media. El arbol real tiene 43.298 con una MEDIANA de 1.972 bytes. No son dos tamaños del mismo
problema: son dos problemas.

    corpus del banco    656 ficheros    642.374 B de media   -> todo son ficheros grandes
    arbol real       43.298 ficheros      1.972 B de MEDIANA -> la mitad son diminutos

Y de ahi sale la cifra que de verdad manda, que no es MB/s:
    43.298 ficheros / 445 s      =  97 ficheros por segundo
    653 segundos-CPU / 43.298    =  15 ms de CPU POR FICHERO

Quince milisegundos de CPU para un fichero de 2 KB. Eso no lo explican ni leer, ni trocear, ni
hashear: son microsegundos. Tiene que haber trabajo pesado que se paga una vez por fichero.

LA SOSPECHA PRINCIPAL, y va como sospecha porque no esta medida todavia: el corpus tiene 12.569
JPEG. bench/f2.3-jpeg.tsv midio el carril libjxl a 6,4 MB/s sobre 2000 JPEG de 200 MB, o sea
15,6 ms por JPEG. Doce mil quinientos por quince milisegundos son ~196 segundos-CPU, el 30% del
total. La ablacion --no-jpeg da el numero exacto y esta encolada.
"""
import csv
F = [
("corpus del banco (nivel-zstd.tsv)", 656, 421397452, 642374, "-", "todo ficheros grandes"),
("arbol real (F3.1)", 43298, 2620837053, 60530, 1972, "la mitad por debajo de 2 KB"),
]
D = [
("ficheros por debajo de CDC_MIN (64 KB)", 38225, "88.3% de los ficheros", 243089138, "9.3% de los bytes",
 "un solo trozo: el troceador no rueda ni un byte, y aun asi se paga todo el coste por fichero"),
("ficheros por encima", 5073, "11.7% de los ficheros", 2377747915, "90.7% de los bytes",
 "estos si se trocean; aqui la deduplicacion tiene algo que hacer"),
]
with open("bench/tallas.tsv", "w", newline="") as f:
    f.write("# LA TALLA DE LOS FICHEROS. Por que el banco no podia ver la perdida de velocidad:\n")
    f.write("# midio sobre 656 ficheros de 642 KB de media y el arbol real tiene 43.298 con\n")
    f.write("# MEDIANA de 1.972 B. No son dos tamaños del mismo problema, son dos problemas.\n")
    f.write("# Lo escribe bench/tallas.py.\n")
    w = csv.writer(f, delimiter="\t")
    w.writerow(["arbol", "ficheros", "bytes", "media_B", "mediana_B", "nota"])
    for r in F: w.writerow(r)
    w.writerow([])
    w.writerow(["reparto", "ficheros", "pct_ficheros", "bytes", "pct_bytes", "nota"])
    for r in D: w.writerow(r)
    w.writerow([])
    w.writerow(["derivado", "valor", "", "", "", "nota"])
    w.writerow(["ficheros por segundo", "97", "", "", "", "43.298 / 445 s — LA UNIDAD QUE MANDA, no MB/s"])
    w.writerow(["ms de CPU por fichero", "15", "", "", "", "653 s-CPU / 43.298 — enorme para 2 KB"])
    w.writerow(["percentil 50", "1972", "", "", "", "B"])
    w.writerow(["percentil 90", "82998", "", "", "", "B"])
    w.writerow(["percentil 99", "733691", "", "", "", "B"])
print("bench/tallas.tsv escrito")
