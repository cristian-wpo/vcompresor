#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""EL CAMINO DEL BYTE, MEDIDO DIRECTO: donde se va el trabajo de verdad.

Una sola pasada sobre el arbol completo (43.298 ficheros, 2.499 MiB) con la maquina a carga 6,4
y VC_FASES encendido. Sin restar totales, sin suelo de ruido que discutir: cada fase lleva su
propio cronometro por dentro.

EL RESULTADO REORDENA LAS PRIORIDADES DEL PROYECTO:

    transformar   264,01 s   69,9%    preflate + E8E9 + log-ts
    jxl            64,12 s   17,0%
    comprimir      41,25 s   10,9%    <- el CODEC
    el resto        8,28 s    2,2%

El compresor es el 10,9% del trabajo. Todo el esfuerzo del proyecto esta en el codec —vc-cm son
1.945 lineas, 43 modelos, 484 MiB de tablas, la piramide entera, catorce medidas M1..M14— y
actua sobre una decima parte del tiempo. El grueso esta en DESHACER Y REHACER DEFLATE, que es
donde vive preflate-rs (de terceros) y donde nuestro vc-inflate va a 285 MB/s contra el ~1 GB/s
que da zlib.

LO CONFIRMA UN SEGUNDO DATO, y es el que no deja lugar a dudas: «esperar» son 0,14 s. El hilo
principal casi NO espera a los hilos de compresion, asi que esos hilos no son el cuello — no se
les llega a dar trabajo. El cuello esta antes de ellos.

CONSECUENCIA CONCRETA: la tabla de 9 bits en vc-inflate deja de ser un «estaria bien» y pasa a
estar en el camino del 69,9%. Y la pregunta «¿podemos hacerlo nosotros mejor que preflate-rs?»
deja de ser filosofica.

ADEMAS, ESTA MEDIDA VALIDA P15 CON UN NUMERO. Para saber lo que costaba «agrupar»:
    por ablacion   6 compresiones del arbol entero, ~30 min ->  0,78% ± ruido 2,05% = NADA
    por fases      1 pasada                                  ->  0,32%, sin ruido
Media hora de maquina para no poder afirmar nada, teniendo el instrumento directo en el codigo.

LO QUE ESTA TABLA NO DICE, para que nadie la lea de mas: el TOTAL de 377,66 s no es el reloj
(348,7 s) porque las fases de compresion ocurren en otros hilos y se solapan. Y la fase «stat»
cuenta 43.298 llamadas —una por fichero— cuando strace mide 5,73 statx por fichero: la
instrumentacion solo ve las que estan envueltas en cron(F_STAT), no las de recorrer() ni las de
meta+xattr. La tabla dice donde se va el TIEMPO, no cuantas llamadas se hacen.
"""
import csv

# fase · segundos · veces · us por vez
F = [
    ("recorrer",      0.43,      1, 425026.4),
    ("agrupar",       1.20,      1, 1201666.4),
    ("stat",          0.17,  43298,      4.0),
    ("enrutar",       0.34,  44093,      7.6),
    ("jxl",          64.12,    677,  94705.6),
    ("transformar", 264.01,   5560,  47483.7),
    ("leer+trocear",  2.02,  62742,     32.2),
    ("hash",          3.16, 192310,     16.4),
    ("dedup+copiar",  0.64,  96672,      6.6),
    ("esperar",       0.14,   1654,     81.9),
    ("meta+xattr",    0.20,  59640,      3.3),
    ("comprimir",    41.25,   1654,  24937.8),
]
TOT = sum(f[1] for f in F)

with open("bench/camino-byte.tsv", "w", newline="") as f:
    f.write("# EL CAMINO DEL BYTE, medido DIRECTO con VC_FASES en una sola pasada.\n")
    f.write("# 43.298 ficheros, 2.499 MiB, maquina a carga 6,4. Sin restar totales.\n")
    f.write("# EL COMPRESOR ES EL 10,9% DEL TRABAJO. El 69,9% son las transformadas.\n")
    f.write("# El TOTAL no es el reloj (348,7 s): las fases de compresion van en otros hilos.\n")
    f.write("# Lo escribe bench/camino-byte.py con los datos de bench/fases.sh.\n")
    w = csv.writer(f, delimiter="\t")
    w.writerow(["fase", "segundos", "pct", "veces", "us_por_vez", "que_es"])
    QUE = {
        "transformar": "preflate + E8E9 + log-ts: deshacer y rehacer DEFLATE",
        "jxl": "JPEG -> JXL sin perdida",
        "comprimir": "el codec: zstd o vc-cm. SOLO el 10,9%",
        "esperar": "lo que el principal espera a los hilos de compresion. Casi cero = NO son el cuello",
        "hash": "blake3 de cada trozo",
        "stat": "solo las envueltas en cron(F_STAT); strace ve 5,73 statx/fichero en total",
        "agrupar": "reordenar por similitud. Por ablacion salia 0,78% ± 2,05%; aqui 0,32% exacto",
    }
    for n, s, v, us in sorted(F, key=lambda x: -x[1]):
        w.writerow([n, f"{s:.2f}", f"{100*s/TOT:.1f}", v, f"{us:.1f}", QUE.get(n, "")])

print(f"bench/camino-byte.tsv · total {TOT:.2f} s de trabajo\n")
print(f"  {'fase':<14} {'seg':>8} {'%':>7}   {'que es'}")
for n, s, v, us in sorted(F, key=lambda x: -x[1]):
    barra = "#" * int(40 * s / TOT)
    print(f"  {n:<14} {s:8.2f} {100*s/TOT:6.1f}%  {barra}")
print()
print("  El CODEC es el 10,9%. Las TRANSFORMADAS, el 69,9%.")
print("  Y «esperar» = 0,14 s: los hilos de compresion no son el cuello, no se les da trabajo.")
