# -*- coding: utf-8 -*-
"""¿CUANTOS FICHEROS A LA VEZ LE COMPENSA VER AL MEZCLADOR? La curva, no la anecdota.

Pregunta del dueño (01-08): «esto tambien aprovechar en el mezclador para varios archivos a la
vez, probar si aun no se hace». LO PRIMERO, leido en el codigo: vc-cm NO SABE donde acaba un
fichero y empieza otro — ve un flujo plano de bytes. Lo que cruza la frontera es su MATCH MODEL
(ranuras de 6 bytes, 12 y disperso), que engancha por CONTENIDO, no por posicion. O sea que si
aprovecha varios ficheros a la vez, pero por un mecanismo distinto del que la pregunta supone.

LO QUE NO ESTABA MEDIDO ES LA CURVA: ¿sigue mejorando cuantos mas le des, o satura? Eso decide si
el tamaño de frame de hoy (64 MiB) deja algo encima de la mesa.

EL MONTAJE, y es lo que hace la comparacion limpia: LA MISMA MASA DE DATOS SIEMPRE. Se parte en
grupos de N ficheros, se comprime cada grupo POR SEPARADO, y se suman las salidas. Lo unico que
cambia entre filas es cuantos ficheros ve el compresor a la vez.
"""
import subprocess, sys, os, glob, tempfile

raiz, nmax, exe = sys.argv[1], int(sys.argv[2]), sys.argv[3]
fs = [p for p in glob.glob(os.path.join(raiz,"**","*"), recursive=True)
      if os.path.isfile(p) and os.path.getsize(p) >= 4096]
fs.sort(); fs = fs[:nmax]
bufs = [open(p,"rb").read() for p in fs]
total = sum(len(b) for b in bufs)
print(f"\n  {len(fs)} ficheros · {total:,} B en total · el mismo dato en todas las filas\n".replace(",","."))
print(f"  {'ficheros/grupo':>15} {'grupos':>7} {'salida':>14} {'vs 1':>9} {'vs anterior':>12}")
print("  " + "-"*62)
base = prev = None
N = 1
while N <= len(fs):
    out = 0
    for i in range(0, len(bufs), N):
        b = b"".join(bufs[i:i+N])
        with tempfile.NamedTemporaryFile(delete=False) as t: t.write(b); p = t.name
        o = p + ".vc"
        subprocess.run([exe,"compress",p,"-o",o,"--level","9","--max"], capture_output=True)
        out += os.path.getsize(o) if os.path.exists(o) else 0
        os.unlink(p); os.path.exists(o) and os.unlink(o)
    g = (len(bufs)+N-1)//N
    if base is None: base = out
    d1 = 100*(out-base)/base
    da = "" if prev is None else f"{100*(out-prev)/prev:>+11.3f}%"
    print(f"  {N:>15} {g:>7} {out:>14,} {d1:>+8.3f}% {da:>12}".replace(",","."))
    prev = out
    if N >= len(fs): break
    N = min(N*2, len(fs))
