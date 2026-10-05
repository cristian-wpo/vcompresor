# -*- coding: utf-8 -*-
"""NUESTRAS HERRAMIENTAS, cada una con lo suyo: su estado, sus preguntas y sus datos.

Encargo del dueño: «hacer el panel mas entendible, para saber cuales son nuestras herramientas y
que se puedan trabajar por separado» y «cada herramienta propia con sus propias preguntas y todo
propio».

POR QUE HACIA FALTA. El panel tenia los datos repartidos por temas —la piramide, la aguja, los
rivales, el camino— y para saber «como va vdeflate» habia que ir juntando trozos de cinco sitios.
Peor: nada impedia que una cifra de una herramienta acabara decidiendo por otra, y eso paso de
verdad el 2026-07-31 con --price.

LO QUE NO SE HACE AQUI: escribir cifras a mano. Cada columna se LEE de donde vive —el libro
mayor, la tabla de terceros, las matrices de preguntas— y si un dato no existe, se dice que no
existe en vez de rellenarlo. Un panel que inventa para que la tabla quede bonita es peor que uno
con huecos.
"""
import csv, json, os, re, sys
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__))))
import atribucion as ATR

def tsv(p):
    r = []
    try:
        with open(os.path.join("bench", p), encoding="utf-8") as f:
            for l in f:
                if l.startswith("#") or not l.strip(): continue
                r.append(l.rstrip("\n").split("\t"))
    except FileNotFoundError:
        pass
    return r

# ── QUE HERRAMIENTAS SON NUESTRAS ────────────────────────────────────────────────────────────
# La lista NO vive aqui: vive en bench/atribucion.py, una sola vez para todo el banco. Vivia
# aqui Y dentro de panel.py, y las dos copias dejaron de coincidir — la misma fila del libro
# mayor tenia dos duenos segun a quien preguntaras. Anadir una herramienta es anadir una linea
# alli y nada mas.
H = [(k, nom, que, sus, cod, pes) for k, nom, que, sus, cod, pes, _res in ATR.HERRAMIENTAS]

# ── SUS PREGUNTAS: cuantas del CAMINO se le han hecho a cada una ──────────────────────────────
# Se leen de las matrices que ya existen. Los nombres de pieza de las matrices no coinciden con
# nuestras claves, asi que hay un mapa explicito — y lo que no case se dice, no se inventa.
MAPA = ATR.MAPA_PIEZA
preg = {k: {"hechas": 0, "hallazgos": 0, "sin": 0} for k, *_ in H}
# El veredicto vive en la columna 6 de estas dos matrices —pieza, donde, lineas, que_es,
# pregunta_id, la_pregunta, VEREDICTO, la_respuesta—, no en la 2. Con el indice mal salian
# cero hallazgos en todas las herramientas menos una, que es un resultado tan limpio como falso.
VER = 6
for f in tsv("auditoria-camino.tsv")[1:]:
    if len(f) <= VER: continue
    k = MAPA.get(f[0])
    if not k or k not in preg: continue
    v = f[VER]
    if v.startswith("SIN"): preg[k]["sin"] += 1
    elif v.startswith("NO"): pass          # «no aplica» no cuenta como pregunta hecha
    else:
        preg[k]["hechas"] += 1
        if v.startswith("HALLAZGO") or v.startswith("SOSPECHA"): preg[k]["hallazgos"] += 1
# vdeflate tiene ademas su matriz propia de 16 preguntas x 5 piezas
for f in tsv("vdeflate-preguntas.tsv")[1:]:
    if len(f) <= VER: continue
    v = f[VER]
    if v.startswith("SIN"): preg["vdeflate"]["sin"] += 1
    elif v.startswith("NO"): pass
    else:
        preg["vdeflate"]["hechas"] += 1
        if v.startswith("HALLAZGO") or v.startswith("SOSPECHA"): preg["vdeflate"]["hallazgos"] += 1
# y comprimir, que es del archivador
for f in tsv("camino-comprimir.tsv")[1:]:
    if len(f) < 3: continue
    v = f[2]
    if v.startswith("SIN"): preg["archivador"]["sin"] += 1
    else:
        preg["archivador"]["hechas"] += 1
        if v.startswith("HALLAZGO") or v.startswith("SOSPECHA"): preg["archivador"]["hallazgos"] += 1

# ── SUS DATOS: que tablas del banco son de cada una ───────────────────────────────────────────
# Por prefijo del nombre del fichero, que es como estan puestos. Lo que no case va a «del banco».
tablas = {k: [] for k, *_ in H}
sueltas = []
for f in sorted(os.listdir("bench")):
    if not f.endswith(".tsv"): continue
    n = f[:-4]
    k = ATR.herr_de_tabla(n)
    if k in tablas: tablas[k].append(n)
    elif k is None: sueltas.append(n)

# ── SUS MEDIDAS: cuantas filas del libro mayor son suyas ──────────────────────────────────────
med = {k: 0 for k, *_ in H}
try:
    for l in open("bench/historia.jsonl", encoding="utf-8"):
        l = l.strip()
        if not l: continue
        try: r = json.loads(l)
        except json.JSONDecodeError: continue
        h = ATR.herr_de_fila(r)[0]
        if h in med: med[h] += 1
except FileNotFoundError:
    pass

# ── SU ESTADO: se lee de la tabla de terceros, que ya lo lleva ────────────────────────────────
estado = {}
for f in tsv("terceros.tsv")[1:]:
    if len(f) < 4: continue
    nu = f[2].strip()
    for k, *_ in H:
        if k in nu or nu.endswith(k): estado[k] = f[3]

with open("bench/herramientas-propias.tsv", "w", newline="") as fh:
    fh.write("# NUESTRAS HERRAMIENTAS, cada una con lo suyo. Lo escribe bench/propias.py.\n")
    fh.write("# Lo unico a mano es QUE existe y QUE sustituye; el resto se LEE del banco.\n")
    fh.write("# «preguntas» son las del CAMINO que se le han hecho a esa herramienta.\n")
    fh.write("# «medidas» son sus filas del libro mayor. Un cero ahi no es que sea barata:\n")
    fh.write("# es que nunca ha pasado por la bascula.\n")
    w = csv.writer(fh, delimiter="\t")
    w.writerow(["clave", "nombre", "que_es", "sustituye_a", "codigo", "pestana",
                "estado", "preguntas_hechas", "hallazgos", "preguntas_sin_hacer",
                "medidas", "tablas_propias", "sus_tablas"])
    for k, nom, que, sus, cod, pes in H:
        p = preg[k]
        w.writerow([k, nom, que, sus, cod, pes, estado.get(k, "—"),
                    p["hechas"], p["hallazgos"], p["sin"], med.get(k, 0),
                    len(tablas[k]), " ".join(tablas[k][:40])])

print(f"\n  NUESTRAS HERRAMIENTAS — {len(H)}\n")
print(f"  {'herramienta':<16}{'sustituye a':<28}{'preguntas':>11}{'hallazgos':>11}{'medidas':>9}{'tablas':>8}")
print("  " + "-" * 84)
for k, nom, que, sus, cod, pes in H:
    p = preg[k]
    print(f"  {nom[:15]:<16}{sus[:26]:<28}{p['hechas']:>11}{p['hallazgos']:>11}"
          f"{med.get(k,0):>9}{len(tablas[k]):>8}")
print()
if sueltas:
    print(f"  ⚠ {len(sueltas)} tablas sin dueño asignado: {', '.join(sueltas[:12])}"
          + (" ..." if len(sueltas) > 12 else ""))
    print("    No se reparten a ojo: una tabla que no se sabe de quien es, se dice.")
print()
print("  bench/herramientas-propias.tsv escrito")
