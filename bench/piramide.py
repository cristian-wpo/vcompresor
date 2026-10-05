#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""LA PIRAMIDE. Todo el gasto de recursos, del primer dato al programa completo.

Encargo del dueño: «ordenar todo desde el primer punto, el primer dato, hasta el programa
completo, indicando en cada peldaño cada proceso o capa lo que gasta de tiempo y CPU, y que cada
capa se pueda desmenuzar para ver 1 a 1 donde estan los gastos. Esto debe medir COMPLETO».

LO QUE LA PIRAMIDE ENSEÑA Y UN PERFIL NO: la diferencia entre CPU y RELOJ.

    comprimir (zstd-19)   133,28 s de CPU   72,3% del trabajo   pero en DIEZ HILOS
    transformar (preflate) 37,79 s de CPU   20,5% del trabajo   pero en el HILO PRINCIPAL

El que mas CPU gasta es zstd, con diferencia. Pero corre en diez hilos, asi que sus 133 segundos
de CPU cuestan unos 13 de reloj. Los 37,79 de preflate van en serie: son 37,79 segundos que la
persona espera delante de la pantalla. UN PELDAÑO PARALELO Y UNO EN SERIE NO CUESTAN LO MISMO
AUNQUE MARQUEN EL MISMO NUMERO, y esa es toda la diferencia entre saber donde va el tiempo y
saber donde va LA ESPERA.

Por eso cada peldaño lleva las dos columnas y dice si es serie o paralelo.

MEDIDO, no derivado: las catorce fases se cronometran DENTRO del hilo que hace el trabajo,
incluidas las de los hilos de compresion. La version anterior no las tenia y habia que restarlas
del total, y restar no es medir.
"""
import csv, os

# ---------------------------------------------------------------- lo medido
# Mac M2 Pro, 10 nucleos, corpus de 10.824 ficheros / 544,9 MiB, --level 19.
RELOJ, CPU_USER, CPU_SYS, RSS = 52.69, 191.01, 6.45, 1_668_169_728
FICHEROS, BYTES, SALIDA = 10824, 571_336_360, 305_244_334
NUCLEOS = 10

# fase -> (segundos, veces, en_paralelo)
F = {
    "recorrer":     (0.20,      1, False),
    "agrupar":      (1.33,      1, True),    # sus firmas van en hilos, pero es una barrera
    "stat":         (0.06,  21648, False),
    "enrutar":      (1.23,  11012, False),
    "jxl":          (8.48,    170, True),
    "transformar":  (37.79,  1402, False),
    "leer+trocear": (0.52,  15102, False),
    "hash":         (1.40,  21805, False),
    "dedup+copiar": (0.11,   9513, False),
    "esperar":      (0.00,    524, False),
    "meta+xattr":   (0.04,  21735, False),
    "comprimir":    (133.28,  524, True),
}
# lo que compra cada pieza, de la ablacion del Mac (mismo arbol, misma maquina)
COMPRA = {"transformar": 7.136 + 0.185 + 0.005, "jxl": 4.766, "agrupar": 0.097}
# el desglose de «transformar», de la ablacion
DENTRO = {
    "transformar": [("preflate", 7.136, "58,5% del CPU total al ablacionarlo"),
                    ("E8E9",     0.185, "coste no medible, por debajo del ruido"),
                    ("log-ts",   0.005, "coste no medible, y no compra nada")],
}

CPU_TOT = CPU_USER + CPU_SYS
serie = sum(s for s, _, p in F.values() if not p)
paral = sum(s for s, _, p in F.values() if p)
TOTAL = serie + paral

filas = []
def pon(nivel, ident, padre, nombre, seg, cpu, modo, veces, compra, nota):
    filas.append([nivel, ident, padre, nombre, f"{seg:.2f}", f"{cpu:.2f}", modo,
                  veces, f"{seg*1e6/veces:.1f}" if veces else "",
                  f"{compra:.3f}" if compra is not None else "", nota])

pon(0, "todo", "", "EL PROGRAMA ENTERO", RELOJ, CPU_TOT, "—", 1, None,
    f"{FICHEROS:,} ficheros, {BYTES/1048576:.1f} MiB -> {SALIDA/1048576:.1f} MiB. "
    f"RAM pico {RSS/1073741824:.2f} GB. {CPU_TOT/RELOJ:.1f} nucleos de {NUCLEOS} ocupados de media.")

pon(1, "serie", "todo", "EN SERIE — el hilo principal", serie, serie, "serie", 1, None,
    f"{100*serie/RELOJ:.0f}% del reloj. ESTO ES LO QUE LA PERSONA ESPERA: un solo hilo, "
    f"nada se solapa. Bajar un segundo aqui es un segundo menos de espera.")
pon(1, "paralelo", "todo", "EN PARALELO — los otros hilos", paral/NUCLEOS, paral, "paralelo", 1, None,
    f"{paral:.0f} s de CPU repartidos entre {NUCLEOS} hilos = ~{paral/NUCLEOS:.0f} s de reloj. "
    f"Es {100*paral/TOTAL:.0f}% del TRABAJO y solo ~{100*(paral/NUCLEOS)/RELOJ:.0f}% de la ESPERA.")

for n, (s, v, p) in sorted(F.items(), key=lambda x: -x[1][0]):
    padre = "paralelo" if p else "serie"
    pon(2, n, padre, n, s if not p else s, s, "paralelo" if p else "serie", v,
        COMPRA.get(n), "")

for padre, hijos in DENTRO.items():
    tot_hijo = F[padre][0]
    for nom, compra, nota in hijos:
        # el reparto de segundos dentro de «transformar» sale de la ablacion, no de un
        # cronometro por pieza: se dice, porque no es lo mismo.
        pon(3, f"{padre}:{nom}", padre, nom, 0, 0, "serie", 0, compra,
            f"{nota}. El reparto de segundos dentro de «{padre}» sale de la ABLACION "
            f"(apagar la pieza y restar), no de un cronometro propio.")

with open("bench/piramide.tsv", "w", newline="") as f:
    f.write("# LA PIRAMIDE: todo el gasto de recursos, del primer dato al programa completo.\n")
    f.write("# Cada peldaño lleva RELOJ y CPU por separado, y dice si va EN SERIE o EN PARALELO,\n")
    f.write("# porque un peldaño paralelo y uno en serie no cuestan lo mismo aunque marquen el\n")
    f.write("# mismo numero: el paralelo gasta CPU, el de la serie gasta ESPERA.\n")
    f.write("# «compra_pct» = cuanto CRECE el archivo si se apaga esa pieza (de la ablacion).\n")
    f.write("# Lo escribe bench/piramide.py con lo que mide VC_FASES=1.\n")
    w = csv.writer(f, delimiter="\t")
    w.writerow(["nivel", "id", "padre", "nombre", "reloj_s", "cpu_s", "modo", "veces",
                "us_por_vez", "compra_pct", "nota"])
    for r in filas:
        w.writerow(r)

print(f"bench/piramide.tsv: {len(filas)} peldaños\n")
print(f"  {'peldaño':<36}{'reloj':>9}{'CPU':>10}{'modo':>11}{'veces':>9}{'compra':>9}")
# SE RECORRE EL ARBOL DE VERDAD. La version anterior imprimia en orden de insercion y en
# pantalla parecia que todo colgaba de «paralelo»: el dato estaba bien y la vista mentia.
por_padre = {}
for r in filas:
    por_padre.setdefault(r[2], []).append(r)
def pinta(ident, prof):
    for r in sorted(por_padre.get(ident, []), key=lambda x: -float(x[5])):
        sang = "   " * prof
        print(f"  {sang}{r[3][:34-len(sang)]:<{34-len(sang)}}{r[4]:>9}{r[5]:>10}{r[6]:>11}"
              f"{r[7]:>9}{(r[9]+'%') if r[9] else '':>9}")
        pinta(r[1], prof + 1)
pinta("", 0)
print()
print(f"  EN SERIE:    {serie:6.2f} s = {100*serie/RELOJ:.0f}% del reloj  <- la espera")
print(f"  EN PARALELO: {paral:6.2f} s de CPU = {100*paral/TOTAL:.0f}% del trabajo, "
      f"~{100*(paral/NUCLEOS)/RELOJ:.0f}% de la espera")
