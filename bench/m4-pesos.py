#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""M4: QUE PESO LES DA EL MEZCLADOR A CADA UNO DE LOS 43 MODELOS.

El instrumento existe desde hace tiempo —VC_DUMP_W vuelca los pesos aprendidos— y NINGUNA tabla
del banco lo usa. Se calcula y se tira, que es la pregunta P4 del camino cometida dentro de la
pieza mas cara del proyecto.

QUE PUEDE DAR. El mezclador aprende cuanto se fia de cada modelo. Un modelo cuyo peso converge a
cero esta MUERTO: sigue costando su busqueda en tabla —y el 83,7% del tiempo de vc-cm es predecir+actualizar, donde viven
busquedas en tablas que no caben en cache— sin aportar nada a la prediccion. Si hay tres o
cuatro asi, apagarlos es velocidad gratis.

COMO SE LEE EL VOLCADO. w es Vec<Vec<i32>>: NMIX=4 mezcladores, y el mezclador k tiene
NSEL[k] selectores, cada uno con NCTXP=48 pesos (43 modelos reales + 5 de relleno que arrancan
a 0 y se quedan a 0 por construccion). El volcado los escribe seguidos como i32 little-endian.

QUE NO PRUEBA ESTO, dicho antes de mirar los numeros: un peso pequeño NO demuestra que el modelo
sobre. Puede tener peso bajo y aun asi aportar en los pocos sitios donde acierta, y el mezclador
lo compensa con el resto. Esto SEÑALA candidatos; quien decide es la ablacion (M2), apagandolos
de uno en uno y midiendo los bytes. Por eso la salida ordena candidatos y no dicta bajas.

ES INDEPENDIENTE DE LA CARGA: los pesos son deterministas para la misma entrada.
"""
import subprocess
import struct
import sys
import os

NMIX = 4
NSEL = [512, 256, 32, 256]
NCTXP = 48
NOMBRES = [
    "orden-0", "orden-1", "orden-2", "orden-3", "orden-4", "orden-5", "palabra", "orden-8",
    "sparse-d2", "sparse-d1+3", "indirecto-1", "columna x sep", "encima x col", "encima x byte",
    "orden-12", "pareja-palabra", "pareja-minus", "columna2 x sep2", "encima2 x col2",
    "encima2 x byte", "forma-texto", "forma-texto-larga", "sparse-paso4", "sparse-paso8",
    "sparse-paso2y4", "indirecto-2",
    "num:digito-encima", "num:largo", "num:delta", "num:prefijo",
    "txt:trigrama", "txt:raiz", "txt:trigrama-raices", "txt:skip-grama",
    "DMC", "match-6", "match-12", "cadena ISSE", "match-disp-2", "match-disp-4", "match-disp-8",
    "no-lineal:dispersion", "no-lineal:disp x media",
]
NCTX = len(NOMBRES)

VC_CM = "./target/release/vc-cm"


def corre(muestra, etiqueta):
    dump = f"/tmp/w-{etiqueta}.bin"
    out = f"/tmp/o-{etiqueta}.cm"
    env = dict(os.environ, VC_DUMP_W=dump)
    r = subprocess.run([VC_CM, "c", muestra, out], env=env, capture_output=True)
    if r.returncode != 0 or not os.path.exists(dump):
        print(f"  !! {etiqueta}: no salio el volcado (rc={r.returncode})")
        print("     " + r.stderr.decode()[:200])
        return None
    d = open(dump, "rb").read()
    esperado = sum(NSEL) * NCTXP * 4
    if len(d) != esperado:
        print(f"  !! {etiqueta}: el volcado mide {len(d)} y se esperaban {esperado}.")
        print("     La forma del volcado no es la que cree este guion: NO se interpreta.")
        return None
    w = struct.unpack(f"<{len(d)//4}i", d)
    # por modelo: el peso ABSOLUTO medio sobre todos los selectores de todos los mezcladores
    porm = [[] for _ in range(NCTXP)]
    p = 0
    for k in range(NMIX):
        for _ in range(NSEL[k]):
            for i in range(NCTXP):
                porm[i].append(w[p + i])
            p += NCTXP
    ent = os.path.getsize(muestra)
    sal = os.path.getsize(out)
    os.remove(dump); os.remove(out)
    return porm, ent, sal


MUESTRAS = sys.argv[1:]
if not MUESTRAS:
    print("uso: m4-pesos.py <fichero> [fichero...]"); raise SystemExit(2)

print("M4 · LOS PESOS QUE EL MEZCLADOR HA APRENDIDO")
print("Un peso que converge a cero = modelo muerto que sigue pagando su busqueda en tabla.")
print()

acum = {}
for m in MUESTRAS:
    et = os.path.basename(m)[:20]
    r = corre(m, et.replace("/", "_"))
    if not r:
        continue
    porm, ent, sal = r
    print(f"  {et:<22} {ent:>10,} B -> {sal:>9,} B   {8*sal/ent:.4f} bits/byte")
    acum[et] = porm

if not acum:
    print("\n  Sin volcados validos. Nada que interpretar."); raise SystemExit(1)

# media del |peso| por modelo, sobre todas las muestras
med = []
for i in range(NCTX):
    vals = [abs(x) for porm in acum.values() for x in porm[i]]
    med.append(sum(vals) / len(vals) if vals else 0)

# las ranuras de relleno tienen que estar EXACTAMENTE a cero: si no, el volcado no es lo que creemos
relleno = [abs(x) for porm in acum.values() for i in range(NCTX, NCTXP) for x in porm[i]]
print()
if relleno and max(relleno) != 0:
    print(f"  !! las {NCTXP-NCTX} ranuras de relleno deberian estar a CERO y la mayor vale "
          f"{max(relleno)}.")
    print("     O el volcado no tiene la forma que cree este guion, o el relleno no es neutro.")
    print("     Cualquiera de las dos cosas invalida la lectura de abajo. NO se publica.")
    raise SystemExit(1)
print(f"  ✓ las {NCTXP-NCTX} ranuras de relleno estan a cero exacto: el volcado tiene la forma esperada")

# POR MUESTRA, no solo la media. El codigo de vc-cm avisa en su linea 317: «medir en un
# subconjunto y generalizar es el error que mas veces se ha repetido en este compresor». Un
# modelo puede estar a cero en texto y ser imprescindible en binario; la media lo taparia.
import csv as _csv
with open("bench/m4-pesos.tsv", "w", newline="") as _f:
    _f.write("# M4: el |peso| medio que el mezclador aprende para cada modelo, POR MUESTRA.\n")
    _f.write("# Un peso cerca de cero SEÑALA un candidato a estar muerto; NO lo demuestra.\n")
    _f.write("# Quien decide es la ablacion de M2: apagarlo y mirar los BYTES.\n")
    _f.write("# Lo escribe bench/m4-pesos.py.\n")
    _w = _csv.writer(_f, delimiter="\t")
    _w.writerow(["modelo", "indice"] + list(acum) + ["media", "pct_del_mayor"])
    for _i in range(NCTX):
        _v = [sum(abs(x) for x in acum[m][_i]) / len(acum[m][_i]) for m in acum]
        _w.writerow([NOMBRES[_i], _i] + [f"{x:.1f}" for x in _v] +
                    [f"{med[_i]:.1f}", f"{100*med[_i]/(max(med) or 1):.1f}"])
print(f"  bench/m4-pesos.tsv: {NCTX} modelos x {len(acum)} muestras")

orden = sorted(range(NCTX), key=lambda i: med[i])
mx = max(med) or 1
print()
print(f"  {'modelo':<24} {'|peso| medio':>13} {'del mayor':>10}")
print("  " + "-" * 50)
for i in orden:
    barra = "#" * int(30 * med[i] / mx)
    print(f"  {NOMBRES[i]:<24} {med[i]:13.1f} {100*med[i]/mx:9.1f}%  {barra}")

muertos = [NOMBRES[i] for i in orden if med[i] < 0.02 * mx]
print()
print(f"  CANDIDATOS a estar muertos (|peso| < 2% del mayor): {len(muertos)}")
for n in muertos:
    print(f"    · {n}")
print()
print("  ESTO SEÑALA, NO DICTA. Un peso bajo no prueba que el modelo sobre: puede acertar en")
print("  pocos sitios y que el mezclador lo compense con el resto. Quien decide es la ablacion")
print("  de M2 —apagarlos de uno en uno y mirar los BYTES—, y estos son los primeros a probar.")
