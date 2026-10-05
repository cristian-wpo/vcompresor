# -*- coding: utf-8 -*-
"""EL CRUCE QUE DECIDE SI LA FASE 3 SE AFINA O SE REESCRIBE.

Nuestra fase 3 acierta el 82,10% de los simbolos probando las NUEVE CONFIGURACIONES DE zlib. El
censo dice que un tercio del banco NO lo produjo zlib. La pregunta, que hasta ahora nadie habia
contestado con datos: ¿nuestro acierto se hunde justo en esos flujos?

  · SI SE HUNDE -> la identidad del codificador ES lo que nos limita. Meter el hash de libdeflate
    en el juego de candidatos es el camino, y esta medido cuanto hay debajo.
  · SI ES UNIFORME -> el codificador NO es lo que nos limita. Seria un resultado incomodo -tira
    la hipotesis del censo como PALANCA, aunque el censo siga siendo cierto- y habria que buscar
    el limite en otro sitio antes de reescribir nada.

Las dos respuestas valen. La que no vale es reescribir 549 lineas por una corazonada.
"""
import collections, sys

cen = {}
for i, l in enumerate(open("/tmp/censo.tsv")):
    if i == 0: continue
    p = l.rstrip("\n").split("\t")
    if len(p) >= 5: cen[p[0]] = (p[1], p[2], p[3])

f3 = {}
for i, l in enumerate(open("/tmp/fase3.tsv")):
    if l.startswith("#") or i == 0 or l.startswith("fichero"): continue
    p = l.rstrip("\n").split("\t")
    if len(p) >= 5:
        try: f3[p[0]] = (int(p[1]), int(p[2]), int(p[3]), int(p[4]))
        except ValueError: pass

comunes = set(cen) & set(f3)
print(f"\n  censo {len(cen)} · fase3 {len(f3)} · cruzan {len(comunes)}\n")
if not comunes: sys.exit("no cruzan: los nombres de fichero no coinciden")

# agregado por familia de codificador, PONDERADO POR SIMBOLOS (no por flujos: un flujo de 3
# simbolos y otro de 3 millones no valen lo mismo, y promediar porcentajes por flujo mentiria)
FAM = lambda h: ("zlib" if h.startswith("Zlib{") else
                 "libdeflate" if h.startswith("Libdeflate") else
                 "zlib-ng" if h == "ZlibNG" else
                 "miniz" if h == "MiniZFast" else
                 "SIN CLASIFICAR" if h == "SIN-CLASIFICAR" else "otros (crc32c/vector)")
ag = collections.defaultdict(lambda: [0, 0, 0])   # simbolos, aciertos, flujos
for k in comunes:
    h, _, _ = cen[k]; s, a, _, _ = f3[k]
    g = ag[FAM(h)]; g[0] += s; g[1] += a; g[2] += 1

print(f"  {'codificador':<22} {'flujos':>7} {'simbolos':>14} {'acierto':>9}")
print("  " + "-" * 56)
ts = ta = 0
for f, (s, a, n) in sorted(ag.items(), key=lambda x: -x[1][0]):
    ts += s; ta += a
    print(f"  {f:<22} {n:>7} {s:>14,} {100*a/max(s,1):>8.2f}%".replace(",", "."))
print("  " + "-" * 56)
print(f"  {'TODO':<22} {len(comunes):>7} {ts:>14,} {100*ta/max(ts,1):>8.2f}%".replace(",", "."))

z = ag.get("zlib", [0, 0, 0]); ld = ag.get("libdeflate", [0, 0, 0])
if z[0] and ld[0]:
    pz, pl = 100 * z[1] / z[0], 100 * ld[1] / ld[0]
    print()
    print(f"  zlib {pz:.2f}%  contra  libdeflate {pl:.2f}%   ->  {pz-pl:+.2f} puntos")
    print()
    if pz - pl > 3:
        print("  SE HUNDE. La identidad del codificador ES lo que limita la fase 3, y el camino")
        print("  es el del censo: meter el hash de libdeflate en el juego de candidatos.")
    elif abs(pz - pl) <= 3:
        print("  ⚠️ NO SE HUNDE: el acierto es parecido en los dos. El censo sigue siendo cierto")
        print("  -un tercio del banco es libdeflate- pero NO es lo que nos limita, asi que")
        print("  reescribir la fase 3 por ahi seria trabajo sin numero detras. El limite esta en")
        print("  otro sitio y hay que encontrarlo ANTES de tocar 549 lineas.")
    else:
        print("  ⚠️ AL REVES: acertamos MAS en libdeflate que en zlib. Eso no lo predice nadie y")
        print("  hay que entenderlo antes de usar ninguno de los dos numeros.")
