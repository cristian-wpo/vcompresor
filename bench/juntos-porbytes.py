# -*- coding: utf-8 -*-
"""EL CONTROL: grupos IGUALES EN BYTES, no en numero de ficheros.

Por que hace falta. La primera curva agrupaba por NUMERO de ficheros, y los ficheros del arbol
varian muchisimo de tamaño: a 128 ficheros por grupo el mayor eran 7,27 MB y a 256 eran 11,45 MB,
o sea que los grupos NO eran comparables entre filas. El «escalon» del 15,72% que aparecio a 256
puede ser eso y no un umbral del compresor: que a 256 cayera junto un racimo de ficheros parecidos
que a 128 quedaba partido en dos grupos.

Agrupando por BYTES el eje es limpio: lo unico que cambia entre filas es CUANTO DATO ve el
compresor a la vez.
"""
import subprocess, sys, os, glob, tempfile

raiz, exe, extra = sys.argv[1], sys.argv[2], sys.argv[3:]
fs = sorted([p for p in glob.glob(os.path.join(raiz,"**","*"), recursive=True)
             if os.path.isfile(p) and os.path.getsize(p) >= 4096])
bufs = [open(p,"rb").read() for p in fs]
total = sum(len(b) for b in bufs)
print(f"\n  {len(fs)} ficheros · {total:,} B · grupos IGUALES EN BYTES\n".replace(",","."))
print(f"  {'MB/grupo':>10} {'grupos':>7} {'salida':>14} {'vs el menor':>12} {'vs anterior':>12}")
print("  " + "-"*60)
base = prev = None
for mb in (0.5, 1, 2, 4, 8, 16, 32):
    lim = int(mb * 1024 * 1024)
    grupos, cur = [], bytearray()
    for b in bufs:
        if cur and len(cur) + len(b) > lim: grupos.append(bytes(cur)); cur = bytearray()
        cur += b
    if cur: grupos.append(bytes(cur))
    out = 0
    for g in grupos:
        with tempfile.NamedTemporaryFile(delete=False) as t: t.write(g); p = t.name
        o = p + ".vc"
        subprocess.run([exe,"compress",p,"-o",o,"--level","9"]+extra, capture_output=True)
        out += os.path.getsize(o) if os.path.exists(o) else 0
        os.unlink(p); os.path.exists(o) and os.unlink(o)
    if base is None: base = out
    da = "" if prev is None else f"{100*(out-prev)/prev:>+11.3f}%"
    print(f"  {mb:>10} {len(grupos):>7} {out:>14,} {100*(out-base)/base:>+11.3f}% {da:>12}".replace(",","."))
    prev = out
    if len(grupos) == 1: break
