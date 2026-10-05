# -*- coding: utf-8 -*-
"""EL CONTROL: ¿el escalon es CONTENIDO o es un UMBRAL del compresor?

Las dos curvas -zstd y el mezclador- dan un escalon al llegar a 256 ficheros. Dos explicaciones
incompatibles:
  (a) UMBRAL: algo del compresor cambia a ese tamaño de grupo.
  (b) CONTENIDO: esos 256 ficheros concretos se parecen mucho entre si -van ordenados por ruta,
      o sea que son del mismo directorio- y partirlos en dos grupos rompe el parecido.

Se distinguen BARAJANDO EL ORDEN. Si el escalon es un umbral, sigue ahi. Si es contenido, se
esparce: el parecido queda repartido entre todos los grupos y la curva sale suave.

⚠️ La baraja es DETERMINISTA (semilla fija): la misma entrada tiene que dar la misma tabla.
"""
import subprocess, os, glob, tempfile
fs = sorted([p for p in glob.glob("/tmp/corpus-hilos/codigo/**/*", recursive=True)
             if os.path.isfile(p) and os.path.getsize(p) >= 4096])[:256]
bufs = [open(p,"rb").read() for p in fs]
x = 987654321
def rnd():
    global x; x = (x*1103515245+12345) & 0x7FFFFFFF; return x
idx = list(range(len(bufs)))
for i in range(len(idx)-1, 0, -1):
    j = rnd() % (i+1); idx[i], idx[j] = idx[j], idx[i]
baraj = [bufs[i] for i in idx]
print(f"\n  {len(fs)} ficheros · {sum(len(b) for b in bufs):,} B · MISMO dato, orden BARAJADO\n".replace(",","."))
print(f"  {'fich/grupo':>11} {'grupos':>7} {'ordenado':>13} {'barajado':>13} {'vs ant (baraj)':>15}")
print("  " + "-"*64)
ORD = {1:2247789, 2:2114692, 4:2036552, 8:1933438, 16:1837803, 32:1784166, 64:1751073,
       128:1736876, 256:1354391}
prev = None
N = 1
while N <= len(baraj):
    out = 0
    for i in range(0, len(baraj), N):
        b = b"".join(baraj[i:i+N])
        with tempfile.NamedTemporaryFile(delete=False) as t: t.write(b); p = t.name
        o = p + ".vc"
        subprocess.run(["./target/release/vcompresor","compress",p,"-o",o,"--level","9","--max"],
                       capture_output=True)
        out += os.path.getsize(o) if os.path.exists(o) else 0
        os.unlink(p); os.path.exists(o) and os.unlink(o)
    g = (len(baraj)+N-1)//N
    da = "" if prev is None else f"{100*(out-prev)/prev:>+14.3f}%"
    print(f"  {N:>11} {g:>7} {ORD.get(N,0):>13,} {out:>13,} {da:>15}".replace(",","."))
    prev = out
    if N >= len(baraj): break
    N = min(N*2, len(baraj))
