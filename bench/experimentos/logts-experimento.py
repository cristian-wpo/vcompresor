#!/usr/bin/env python3
"""Sacar el timestamp de cabecera de los logs y guardarlo en delta. Reversible.

El reparto de bits (medido sobre 3 MB de nuestros logs, que van a 0,0415 bpb):

    fichero entero        18.698 bytes comprimidos
    sólo los timestamps    7.406      <- el 40% del total, y son el 5% del fichero en crudo
    en delta               5.818

El timestamp de cabecera se lleva cuatro de cada diez bits que aún cuestan los logs. Tiene
sentido: los microsegundos son casi ruido para un modelo de contextos, que sólo puede
predecirlos mirando dígitos anteriores. Pero entre dos líneas seguidas la diferencia es de
microsegundos, y ESO sí es predecible.

Ojo con lo que NO funciona, que también está medido: sacar TODOS los números de la línea
—plantilla completa— sale un 12,25% PEOR. Los números del mensaje pierden su contexto y el
modelo deja de poder predecirlos. La transformada tiene que ser estrecha.

Formato de salida (todo reversible, y se comprueba antes de aceptar):

    [1 byte marca]  [nº de líneas varint]
    flujo DELTA   — un decimal por línea con timestamp, diferencia en microsegundos
    flujo ZONA    — la zona horaria de cada línea (casi constante: comprime a nada)
    flujo RESTO   — el resto de cada línea; las que no llevaban timestamp van marcadas
"""
import sys, re, os, subprocess

RE_TS = re.compile(rb"^\[(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2}):(\d{2})\.(\d{6})([+-]\d{2}:\d{2})\]")
MARCA_SIN = b"\x01"          # esta línea no llevaba timestamp
MARCA_ESC = b"\x02"          # ...y empezaba por \x01 o \x02 de verdad

def dias_civiles(y, m, d):
    """Días desde 1970-01-01. Sin librerías: se porta a Rust en diez líneas."""
    y -= m <= 2
    era = (y if y >= 0 else y - 399) // 400
    yoe = y - era * 400
    doy = (153 * (m + (-3 if m > 2 else 9)) + 2) // 5 + d - 1
    doe = yoe * 365 + yoe // 4 - yoe // 100 + doy
    return era * 146097 + doe - 719468

def civil_de_dias(z):
    z += 719468
    era = (z if z >= 0 else z - 146096) // 146097
    doe = z - era * 146097
    yoe = (doe - doe // 1460 + doe // 36524 - doe // 146096) // 365
    y = yoe + era * 400
    doy = doe - (365 * yoe + yoe // 4 - yoe // 100)
    mp = (5 * doy + 2) // 153
    d = doy - (153 * mp + 2) // 5 + 1
    m = mp + (3 if mp < 10 else -9)
    return y + (m <= 2), m, d

def transformar(datos):
    lineas = datos.split(b"\n")
    deltas, zonas, restos = [], [], []
    prev = 0
    for l in lineas:
        m = RE_TS.match(l)
        if not m:
            r = l
            if r[:1] in (MARCA_SIN, MARCA_ESC): r = MARCA_ESC + r
            restos.append(MARCA_SIN + r)
            continue
        y, mo, d, h, mi, s, us = (int(m.group(i)) for i in range(1, 8))
        abs_us = ((dias_civiles(y, mo, d) * 86400 + h * 3600 + mi * 60 + s) * 1_000_000 + us)
        deltas.append(str(abs_us - prev).encode())
        prev = abs_us
        zonas.append(m.group(8))
        r = l[m.end():]
        if r[:1] in (MARCA_SIN, MARCA_ESC): r = MARCA_ESC + r
        restos.append(r)
    return (b"\n".join(deltas) + b"\x00" + b"\n".join(zonas) + b"\x00" + b"\n".join(restos))

def deshacer(blob):
    a, b, c = blob.split(b"\x00", 2)
    deltas = a.split(b"\n") if a else []
    zonas = b.split(b"\n") if b else []
    restos = c.split(b"\n")
    salida, prev, k = [], 0, 0
    for r in restos:
        if r[:1] == MARCA_SIN:
            r = r[1:]
            if r[:1] == MARCA_ESC: r = r[1:]
            salida.append(r); continue
        if r[:1] == MARCA_ESC: r = r[1:]
        prev += int(deltas[k])
        us = prev % 1_000_000
        seg = prev // 1_000_000
        y, mo, d = civil_de_dias(seg // 86400)
        t = seg % 86400
        cab = b"[%04d-%02d-%02dT%02d:%02d:%02d.%06d%s]" % (
            y, mo, d, t // 3600, (t // 60) % 60, t % 60, us, zonas[k])
        salida.append(cab + r); k += 1
    return b"\n".join(salida)

if __name__ == "__main__":
    vc, ruta = sys.argv[1], sys.argv[2]
    d = open(ruta, "rb").read()
    t = transformar(d)
    v = deshacer(t)
    print(f"reversible: {'SÍ' if v == d else '**NO**'}")
    if v != d:
        i = next((i for i in range(min(len(v), len(d))) if v[i] != d[i]), min(len(v), len(d)))
        print(f"  primer byte distinto en {i}: orig {d[i-40:i+40]!r}  vuelta {v[i-40:i+40]!r}")
        sys.exit(2)
    print(f"  crudo {len(d)} -> {len(t)}  ({100*(len(t)-len(d))/len(d):+.2f}%)")
    open("/tmp/lt-out", "wb").write(t)
    for nom, p in [("original", ruta), ("transformado", "/tmp/lt-out")]:
        subprocess.run([vc, "c", p, "/tmp/lt-" + nom + ".vc"], capture_output=True)
    a = os.path.getsize("/tmp/lt-original.vc"); b = os.path.getsize("/tmp/lt-transformado.vc")
    print(f"\n  vc-cm original      {a:>8}")
    print(f"  vc-cm transformado  {b:>8}   {100*(b-a)/a:+.2f}%")
