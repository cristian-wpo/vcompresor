#!/usr/bin/env python3
"""¿Dónde están los 54.423 bytes que aún cuestan los logs?

logs.10m va a 0,0415 bpb — 193:1. Antes de construir una transformada de plantillas hay que
saber qué parte del fichero se está llevando esos bits. Si son los timestamps, el delta sirve;
si son los mensajes, no.

Se mide separando el fichero en partes y comprimiendo cada una con el MISMO compresor:

    A) el fichero entero
    B) sólo los timestamps, tal cual
    C) sólo los timestamps, en delta de microsegundos
    D) el fichero sin los timestamps
    E) plantilla (la línea con los números sustituidos por un hueco)
    F) sólo los números que se sacaron

La suma D+C dice si separar timestamp del resto compensa. La suma E+F dice lo mismo para la
plantilla completa. Y todo lleva su coste de metadatos contado.
"""
import sys, re, os, subprocess

vc, ruta = sys.argv[1], sys.argv[2]
d = open(ruta, "rb").read()
lineas = d.split(b"\n")
print(f"fichero {len(d)} bytes, {len(lineas)} líneas, media {len(d)//max(1,len(lineas))} B/línea")

# ---- el timestamp de cabecera: [2026-07-14T14:48:59.180146+02:00] ----
RE_TS = re.compile(rb"^\[(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2}):(\d{2})\.(\d{6})([+-]\d{2}:\d{2})\]")

tss, restos, con_ts = [], [], 0
for l in lineas:
    m = RE_TS.match(l)
    if m:
        con_ts += 1
        # microsegundos absolutos del día, que es lo que de verdad avanza
        h, mi, s, us = int(m.group(4)), int(m.group(5)), int(m.group(6)), int(m.group(7))
        tss.append(((h * 3600 + mi * 60 + s) * 1_000_000 + us, m.group(0)))
        restos.append(l[m.end():])
    else:
        restos.append(l)
print(f"líneas con timestamp: {con_ts} ({100*con_ts/max(1,len(lineas)):.1f}%)")

bruto_ts = b"\n".join(t[1] for t in tss)
delta_ts = []
prev = 0
for v, _ in tss:
    delta_ts.append(str(v - prev).encode()); prev = v
delta_ts = b"\n".join(delta_ts)
sin_ts = b"\n".join(restos)

# ---- plantilla: sustituir CUALQUIER número por un hueco ----
RE_NUM = re.compile(rb"\d+")
plant, nums = [], []
for l in lineas:
    trozos = []
    ult = 0
    for m in RE_NUM.finditer(l):
        trozos.append(l[ult:m.start()]); nums.append(m.group(0)); ult = m.end()
    trozos.append(l[ult:])
    plant.append(b"\x01".join(trozos))
plantilla = b"\n".join(plant)
numeros = b"\n".join(nums)

partes = {
    "A entero": d,
    "B timestamps tal cual": bruto_ts,
    "C timestamps en delta": delta_ts,
    "D sin timestamps": sin_ts,
    "E plantilla (sin números)": plantilla,
    "F sólo los números": numeros,
}
res = {}
for nom, dat in partes.items():
    p = "/tmp/lb-" + nom.split()[0]
    open(p, "wb").write(dat)
    subprocess.run([vc, "c", p, p + ".vc"], capture_output=True)
    res[nom] = (len(dat), os.path.getsize(p + ".vc"))
    print(f"  {nom:<26} crudo {len(dat):>9}  comprimido {res[nom][1]:>8}")

a = res["A entero"][1]
print()
print(f"  A                       = {a}")
print(f"  D + B (separar ts)      = {res['D sin timestamps'][1] + res['B timestamps tal cual'][1]:>8}"
      f"  {100*(res['D sin timestamps'][1]+res['B timestamps tal cual'][1]-a)/a:+.2f}%")
print(f"  D + C (ts en delta)     = {res['D sin timestamps'][1] + res['C timestamps en delta'][1]:>8}"
      f"  {100*(res['D sin timestamps'][1]+res['C timestamps en delta'][1]-a)/a:+.2f}%")
print(f"  E + F (plantilla+nums)  = {res['E plantilla (sin números)'][1] + res['F sólo los números'][1]:>8}"
      f"  {100*(res['E plantilla (sin números)'][1]+res['F sólo los números'][1]-a)/a:+.2f}%")
