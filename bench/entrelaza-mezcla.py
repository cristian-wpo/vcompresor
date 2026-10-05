# -*- coding: utf-8 -*-
"""¿COMPRIME MAS SI LEEMOS N FICHEROS A LA VEZ EN VEZ DE UNO DETRAS DE OTRO?

La pregunta es del dueño (01-08): «¿se ha probado mezclando archivos... leerlos a la vez? 8
archivos a la vez de bit a bit pero juntos». NUNCA SE HABIA PROBADO. Lo que si estaba medido es
JUNTARLOS uno detras de otro -vale la MITAD del archivo, -50,5% en codigo- y AGRUPARLOS por
parecido -0,108%-. Entrelazarlos es otra cosa distinta.

LA IDEA, que es buena y tiene precedentes: si ocho ficheros comparten estructura -mismo esquema,
mismo formato de linea- el byte 37 de cada uno se parece mas entre si que a su propio vecino.
Poniendolos en columnas, el modelo tiene el dato parecido A UNA DISTANCIA FIJA Y CORTA. Es lo que
hace el filtro `shuffle` de Blosc, lo que hace Parquet al guardar por columnas, y lo que hace
`bitshuffle` a nivel de bit.

CUATRO MONTAJES DE LOS MISMOS BYTES, para que la comparacion sea limpia (el total no cambia):
  1. concatenado           lo de hoy: fichero tras fichero
  2. entrelazado-1B        un byte de cada uno, por turnos
  3. entrelazado-8B        ocho bytes de cada uno, por turnos
  4. bit-transpuesto       el bit k de cada uno de los 8 -> un byte. Es «8 ficheros, 8 bits a la
                           vez» tal cual lo dijo el dueño.

⚠️ LO QUE HAY QUE ESPERAR, escrito ANTES de medir para no interpretar a posteriori: entrelazar
DEBERIA HACER DAÑO a un compresor LZ -parte las coincidencias largas en trozos de un byte- y
PODRIA ayudar a un modelo de contexto si la estructura es columnar. Si sale al reves, hay que
entenderlo antes de creerselo.
"""
import sys, os, glob

def lee(d, n):
    fs = sorted(glob.glob(os.path.join(d, "**", "*"), recursive=True))
    fs = [f for f in fs if os.path.isfile(f)][:n]
    return [(f, open(f, "rb").read()) for f in fs]

d, n, out = sys.argv[1], int(sys.argv[2]), sys.argv[3]
fs = lee(d, n)
if len(fs) < 2: sys.exit("hacen falta al menos 2 ficheros")
bufs = [b for _, b in fs]
L = min(len(b) for b in bufs)
os.makedirs(out, exist_ok=True)

# 1 · concatenado (y las colas van al final en todos, para que el TOTAL sea identico)
colas = b"".join(b[L:] for b in bufs)
open(f"{out}/1-concatenado.bin", "wb").write(b"".join(bufs))

# 2 y 3 · entrelazado por bloques de k bytes
for k in (1, 8, 64):
    o = bytearray()
    for i in range(0, L, k):
        for b in bufs:
            o += b[i:i+k]
    o += colas
    open(f"{out}/2-entrelazado-{k}B.bin", "wb").write(bytes(o))

# 4 · bit-transpuesto en grupos de 8 ficheros: el bit j del byte i de cada uno -> un byte
o = bytearray()
for g in range(0, len(bufs), 8):
    gr = bufs[g:g+8]
    if len(gr) < 8: break
    lg = min(len(b) for b in gr)
    for i in range(lg):
        bs = [b[i] for b in gr]
        for j in range(8):
            o.append(sum(((bs[t] >> j) & 1) << t for t in range(8)))
o += colas
if len(o) > len(colas):
    open(f"{out}/3-bit-transpuesto.bin", "wb").write(bytes(o))

print(f"  {len(fs)} ficheros · el mas corto {L:,} B · total {sum(len(b) for b in bufs):,} B".replace(",", "."))
for f in sorted(os.listdir(out)):
    print(f"    {f:<26} {os.path.getsize(os.path.join(out,f)):>12,} B".replace(",", "."))
