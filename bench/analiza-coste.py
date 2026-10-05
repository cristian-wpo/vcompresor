#!/usr/bin/env python3
"""Donde se nos van los bits: coste real por posicion, y QUE contenido hay ahi.

El censo responde "que modelo es malo". Esto responde "que contenido no sabemos predecir",
que es la pregunta que lleva a un modelo nuevo en vez de a adivinar familias y medir a
posteriori.

Uso: analiza_coste.py <muestra> <coste.f32>
"""
import sys, struct, array, collections

muestra, costef = sys.argv[1], sys.argv[2]
datos = open(muestra, "rb").read()
n = len(datos)

c = array.array("f")
with open(costef, "rb") as f:
    c.frombytes(f.read(n * 4))          # los primeros n: la pasada real (la sonda va detras)
assert len(c) == n

tot = sum(c)
print(f"muestra {muestra}  {n:,} bytes")
print(f"coste total {tot/8:,.0f} bytes  ({tot/n:.4f} bpb)")
print()

# --- concentracion: que fraccion de bytes se lleva que fraccion de bits ---------------------
orden = sorted(range(n), key=lambda i: -c[i])
acc = 0.0
print("CONCENTRACION")
hitos = [0.001, 0.005, 0.01, 0.02, 0.05, 0.10, 0.25, 0.50]
h = 0
for rank, i in enumerate(orden):
    acc += c[i]
    if h < len(hitos) and rank + 1 >= hitos[h] * n:
        print(f"  el {hitos[h]*100:5.1f}% de bytes mas caros se lleva el {acc/tot*100:5.1f}% de los bits")
        h += 1
print()

# --- por byte: cual es caro y cuanto pesa en el total ---------------------------------------
porbyte = collections.defaultdict(lambda: [0, 0.0])
for i in range(n):
    e = porbyte[datos[i]]
    e[0] += 1
    e[1] += c[i]
print("LOS 15 BYTES QUE MAS BITS SE LLEVAN EN TOTAL")
print(f"  {'byte':<8} {'veces':>10} {'bpb':>8} {'% del total':>12}")
for b, (k, s) in sorted(porbyte.items(), key=lambda kv: -kv[1][1])[:15]:
    r = repr(chr(b)) if 32 <= b < 127 else f"\\x{b:02x}"
    print(f"  {r:<8} {k:>10,} {s/k:>8.3f} {s/tot*100:>11.1f}%")
print()

# --- las regiones caras: donde estan y que dicen ---------------------------------------------
V = 256
nv = n // V
vent = [(sum(c[j*V:(j+1)*V]), j) for j in range(nv)]
vent.sort(reverse=True)
print(f"LAS 12 VENTANAS DE {V} BYTES MAS CARAS")
for s, j in vent[:12]:
    frag = datos[j*V:j*V+110]
    txt = "".join(chr(x) if 32 <= x < 127 else "." for x in frag)
    print(f"  off {j*V:>9,}  {s/V:5.2f} bpb  |{txt}|")
print()

# --- y las baratas, para saber que YA hacemos bien --------------------------------------------
print(f"LAS 4 VENTANAS MAS BARATAS (lo que ya predecimos casi gratis)")
for s, j in vent[-4:]:
    frag = datos[j*V:j*V+110]
    txt = "".join(chr(x) if 32 <= x < 127 else "." for x in frag)
    print(f"  off {j*V:>9,}  {s/V:5.3f} bpb  |{txt}|")
