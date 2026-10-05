#!/usr/bin/env python3
"""EL ESTIMADOR DE LA APP, TAL CUAL, CONTRA EL TIEMPO QUE DE VERDAD FALTABA.

La sonda anterior usaba el ritmo MEDIO, que no es lo que hace la app. Esta reproduce
app-macos/Sources/VCompresor/Motor.swift linea por linea -media exponencial 0,7/0,3, la puerta de
dt>0,35 s, los umbrales de 2 s y 1%, y el candado de dos estimaciones seguidas dentro del 35%- y
ademas pasa el resultado por `cuantoFalta`, que es el redondeo que ve el usuario.

Asi lo que sale por pantalla aqui es LITERALMENTE lo que pone la ventana.

CONTROL, dicho antes de mirar: un estimador util deberia (a) ensenar un numero durante la mayor
parte de la corrida y (b) no equivocarse por mas del doble. Si sale que se calla la mayor parte
del tiempo, el problema es el candado; si sale que habla pero miente, el problema es el ritmo.
"""
import re
import subprocess
import sys
import time


def cuanto_falta(s):
    """`cuantoFalta` de App.swift, igual."""
    if s < 10:
        return "unos segundos"
    if s < 60:
        return "%d s" % (round(s / 5) * 5)
    if s < 3600:
        m = round(s / 60)
        return "1 min" if m <= 1 else "%d min" % m
    h = s / 3600
    return "más de 1 hora" if h < 2 else "unas %d horas" % round(h)


cmd = sys.argv[1:]
pat = re.compile(r"(\d+)/(\d+) ficheros \((\d+)%\), (\d+) MiB")
arranque = time.monotonic()
p = subprocess.Popen(cmd, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE,
                     text=True, bufsize=1)

# ── el estado de Motor.swift, con sus mismos nombres ──────────────────────────────────────────
ritmo = 0.0
ult_miB = 0.0
ult_t = arranque
total_miB = 0.0
ult_estimacion = None

obs = []
for linea in p.stderr:
    m = pat.search(linea)
    if not m:
        continue
    ahora = time.monotonic()
    hechos, tot_f, pct, miB = (int(m.group(i)) for i in (1, 2, 3, 4))
    miB = float(miB)

    if pct >= 1 and miB > 0:
        total_miB = max(total_miB, miB * 100.0 / pct)
    dt = ahora - ult_t
    d_miB = miB - ult_miB
    if dt > 0.35 and d_miB > 0:
        inst = d_miB / dt
        ritmo = inst if ritmo == 0 else ritmo * 0.7 + inst * 0.3
        ult_miB, ult_t = miB, ahora

    restante = None
    transcurrido = ahora - arranque
    if ritmo > 0 and total_miB > 0 and miB > 0 and transcurrido >= 2 and miB / total_miB >= 0.01:
        bruta = max(0.0, (total_miB - miB) / ritmo)
        if ult_estimacion and ult_estimacion > 0 and \
           abs(bruta - ult_estimacion) / max(ult_estimacion, 1) <= 0.35:
            restante = bruta
        ult_estimacion = bruta
    obs.append((transcurrido, hechos, tot_f, pct, miB, ritmo, restante))

p.wait()
fin = time.monotonic() - arranque

print("\ncorrida real: %.1f s · %d lineas de avance\n" % (fin, len(obs)))
print("%7s %5s %7s %9s %14s %11s %9s" %
      ("t", "pct", "MiB", "MiB/s", "la app dice", "de verdad", "error"))
paso = max(1, len(obs) // 22)
for t, h, tf, pct, miB, ritmo, r in obs[::paso]:
    real = fin - t
    if r is None:
        print("%7.1f %5d %7.0f %9.1f %14s %10.1fs %9s" % (t, pct, miB, ritmo, "(callada)", real, "-"))
    else:
        err = (r - real) / max(real, 0.1) * 100
        print("%7.1f %5d %7.0f %9.1f %14s %10.1fs %+8.0f%%" %
              (t, pct, miB, ritmo, "~" + cuanto_falta(r), real, err))

# ── el resumen, que es lo que decide ──────────────────────────────────────────────────────────
con = [(t, r, fin - t) for t, _, _, _, _, _, r in obs if r is not None]
sin = len(obs) - len(con)
print("\nlineas con numero: %d de %d (%.0f%%) · CALLADA el %.0f%% del tiempo"
      % (len(con), len(obs), 100 * len(con) / max(len(obs), 1), 100 * sin / max(len(obs), 1)))
if con:
    errs = sorted(abs(r - real) / max(real, 0.1) for _, r, real in con)
    peor = max(con, key=lambda x: abs(x[1] - x[2]) / max(x[2], 0.1))
    print("error absoluto mediano: %.0f%%  ·  peor: %.0f%% (a los %.1f s decia ~%s y faltaban %.1f s)"
          % (errs[len(errs) // 2] * 100,
             abs(peor[1] - peor[2]) / max(peor[2], 0.1) * 100,
             peor[0], cuanto_falta(peor[1]), peor[2]))
    largos = sum(1 for _, r, real in con if r > real * 1.5)
    cortos = sum(1 for _, r, real in con if r < real * 0.66)
    print("se queda CORTA %d veces · se pasa de LARGA %d veces · en el rango %d"
          % (cortos, largos, len(con) - largos - cortos))
