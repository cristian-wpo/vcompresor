#!/usr/bin/env python3
"""EL RELOJ DE LA BARRA: cuando sale cada linea de avance y que dice, contra el final REAL.

No mide bytes ni compresion: mide si lo que la barra promete se parece a lo que pasa. Se lanza
el compresor con --progress, se sella cada linea con el reloj monotono y al terminar se compara
la ULTIMA linea con el instante en que el proceso muere. Esa diferencia es «la cola».
"""
import re, subprocess, sys, time

cmd = sys.argv[1:]
t0 = time.monotonic()
p = subprocess.Popen(cmd, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, text=True, bufsize=1)
lineas = []
pat = re.compile(r"(\d+)/(\d+) ficheros \(([\d.]+)%\), (\d+) MiB")
for linea in p.stderr:
    t = time.monotonic() - t0
    m = pat.search(linea)
    if m:
        lineas.append((t, float(m.group(3)), int(m.group(4))))
    elif "[traza" in linea:
        print(f"  ...{t:6.2f}s  {linea.strip()}   (la barra iba por el {lineas[-1][1] if lineas else 0}%)")
p.wait()
fin = time.monotonic() - t0

if not lineas:
    print("SIN LINEAS DE AVANCE")
    sys.exit(1)

print(f"corrida        {fin:6.2f} s   ·   {len(lineas)} lineas de avance")
print(f"primera linea  {lineas[0][0]:6.2f} s  ({lineas[0][1]}%)")
print(f"ultima linea   {lineas[-1][0]:6.2f} s  ({lineas[-1][1]}%, {lineas[-1][2]} MiB)")
print(f"LA COLA        {fin - lineas[-1][0]:6.2f} s sin noticias, y se quedo en el {lineas[-1][1]}%")
peor = 0
for i in range(1, len(lineas)):
    peor = max(peor, lineas[i-1][1] - lineas[i][1])
print(f"RETROCESO      {peor} puntos es lo mas que la barra fue hacia atras")
mayor = max(lineas[i][0] - lineas[i-1][0] for i in range(1, len(lineas))) if len(lineas) > 1 else 0
print(f"MAYOR SILENCIO {max(mayor, lineas[0][0]):6.2f} s entre dos lineas seguidas")
print()
print("  el avance contra el reloj (cada decil de tiempo, lo que decia la barra):")
for k in range(1, 11):
    corte = fin * k / 10.0
    vistas = [l for l in lineas if l[0] <= corte]
    if not vistas:
        print(f"    {corte:6.2f}s  ({k*10:3d}% del reloj)   la barra: (aun no dice nada)")
        continue
    t, pct, mb = vistas[-1]
    print(f"    {corte:6.2f}s  ({k*10:3d}% del reloj)   la barra: {pct:3d}%   error {pct - k*10:+4d} puntos")
print()
print("  todas las lineas (segundo -> %):")
print("   " + "  ".join(f"{t:.1f}s:{p}%" for t, p, _ in lineas))
