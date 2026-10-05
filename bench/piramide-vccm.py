#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""LA PIRAMIDE DE vc-cm, nuestra alternativa a zstd-19.

Encargo del dueño: «la piramide de nuestra version propia del zstd-19 deberia tener el tiempo y
el CPU de todo». Y: «desde ahora piramide y aguja seran herramientas muy importantes».

POR QUE AQUI NO SE MIDE CON CRONOMETROS Y EN EL ARCHIVADOR SI. El archivador tiene fases que
duran milisegundos y se recorren miles de veces: un Instant::now() por fase cuesta ~25 ns y no
se nota. El bucle caliente de vc-cm va POR BIT: a 1 MB/s son ocho millones de bits por segundo,
y dos llamadas al reloj por fase y por bit serian ~0,4 s de instrumento por cada MB comprimido.
El instrumento costaria mas que lo medido y cambiaria el resultado.

Para un bucle asi el instrumento correcto es el MUESTREO: `perf` interrumpe cien veces por
segundo y anota donde estaba. No toca el codigo, asi que no lo altera. A cambio da porcentajes
y no segundos exactos, y eso se dice en cada fila.

DE DONDE SALE CADA COLUMNA, que no es todo de la misma fuente y mezclarlas sin decirlo seria
justo lo que este proyecto lleva el dia entero desmontando:
    pct_tiempo   bench/perfil.tsv     perf record sobre --max, muestra real
    MiB          bench/censo.tsv      cuanta memoria ocupa cada modelo
    ocupacion    bench/censo.tsv      que fraccion de sus ranuras usa de verdad
    bpb_solo     bench/censo.tsv      cuanto comprimiria ese modelo EL SOLO
    aporta/cuesta bench/ablacion.tsv  apagar el grupo y medir
"""
import csv, os

# --- las etapas, del perfil (perf) -----------------------------------------------------------
# funcion -> (peldaño padre, que hace)
ETAPAS = {
    "set_byte_contexts": ("contextos", "Calcula los contextos del byte: las claves con las que cada modelo va a buscar."),
    "busca_cubo":        ("predecir",  "Busca en la tabla el cubo que corresponde al contexto."),
    "Predictor::p":      ("predecir",  "Cada modelo mira su cubo y dice que probabilidad le da al siguiente bit."),
    "punto_avx2":        ("mezclar",   "El mezclador: producto escalar de las opiniones por sus pesos, en SIMD."),
    "Predictor::update": ("actualizar","Con el bit ya conocido, cada modelo corrige su cuenta."),
    "fija_cubos":        ("actualizar","Escribe de vuelta el cubo en la tabla."),
    "actualiza_avx2":    ("actualizar","Corrige los pesos del mezclador segun el error, en SIMD."),
}
PADRES = {
    "contextos":  ("Preparar", "Antes de predecir nada hay que saber con que clave busca cada modelo."),
    "predecir":   ("Predecir", "La parte cara: N modelos, cada uno con una busqueda en tabla que casi siempre falla la cache."),
    "mezclar":    ("Mezclar",  "Combinar las N opiniones en una. Es barato: el coste esta en producirlas."),
    "actualizar": ("Actualizar", "Todo lo anterior otra vez, al reves, para aprender del bit que acaba de salir."),
}

def tsv(p):
    if not os.path.exists(p): return []
    return [l.rstrip("\n").split("\t") for l in open(p, encoding="utf-8")
            if l.strip() and not l.startswith("#")]

perfil = {f: float(v) for f, v in (r for r in tsv("bench/perfil.tsv")[1:] if len(r) == 2)}
censo  = tsv("bench/censo.tsv")
abl    = tsv("bench/ablacion.tsv")

filas = []
def pon(nivel, ident, padre, nombre, pct, fuente, nota, extra=""):
    filas.append([nivel, ident, padre, nombre, f"{pct:.1f}" if pct else "", fuente, nota, extra])

total_perf = sum(perfil.values())
pon(0, "vccm", "", "vc-cm — nuestro codec de mezcla de contextos", 100.0, "—",
    f"1.985 lineas nuestras contra las 58.395 de C de zstd. Comprime MAS que zstd-19 y va a ~1 MB/s. "
    f"El perfil cubre el {total_perf:.1f}% del tiempo; el resto se reparte por debajo del umbral del muestreo.",
    "")

# los peldaños grandes
por_padre = {}
for fn, pct in perfil.items():
    padre = ETAPAS.get(fn, ("otros", ""))[0]
    por_padre.setdefault(padre, []).append((fn, pct))
for pid, (nom, nota) in PADRES.items():
    sub = por_padre.get(pid, [])
    pon(1, pid, "vccm", nom, sum(p for _, p in sub), "perf record", nota)
    for fn, pct in sorted(sub, key=lambda x: -x[1]):
        pon(2, f"{pid}:{fn}", pid, fn, pct, "perf record", ETAPAS[fn][1])

# --- los modelos, del censo: memoria, ocupacion y lo que comprimirian solos --------------------
if censo:
    cab = censo[0]
    iM, iR, iU, iO, iB = (cab.index(x) for x in ("MiB", "ranuras", "usadas", "ocup", "bpb_solo"))
    mem = sum(float(r[iM]) for r in censo[1:] if len(r) > iM)
    pon(1, "memoria", "vccm", f"Las tablas de los modelos — {mem:.0f} MiB", 0, "bench/censo.tsv",
        f"{len(censo)-1} modelos. Aqui no se mide tiempo sino MEMORIA, y es la causa del tiempo: "
        f"cada prediccion es una busqueda en {mem:.0f} MiB que casi siempre falla la cache. "
        f"Por eso «predecir» es la etapa cara y no «mezclar».")
    # los diez que mas memoria ocupan, con su ocupacion real
    top = sorted(censo[1:], key=lambda r: -float(r[iM]) if len(r) > iM else 0)[:10]
    for r in top:
        if len(r) <= iB: continue
        ocup = float(r[iO])
        aviso = "  ← ocupa memoria y casi no la usa" if ocup < 15 and float(r[iM]) > 8 else ""
        pon(2, f"mem:{r[0]}", "memoria", r[0], 0, "bench/censo.tsv",
            f"{r[iM]} MiB · {r[iU]} de {r[iR]} ranuras usadas ({ocup:.1f}%) · "
            f"el solo comprimiria a {r[iB]} bits por byte{aviso}")

# --- los grupos, de la ablacion: que aporta y que cuesta --------------------------------------
if abl:
    pon(1, "grupos", "vccm", "Los grupos de modelos — que aporta y que cuesta cada uno", 0,
        "bench/ablacion.tsv",
        "Se apaga el grupo entero y se mide. «aporta» = cuanto EMPEORA el archivo al quitarlo; "
        "«cuesta» = cuanto tiempo se ahorra. Un grupo que aporta poco y cuesta mucho es candidato a irse.")
    for r in abl[1:]:
        if len(r) < 3: continue
        ap, cu = float(r[1]), float(r[2])
        if ap < 0:      v = "ESTORBA: quitarlo MEJORA el archivo"
        elif ap < 0.5 and cu <= -10: v = "MAL NEGOCIO: aporta poco y cuesta mucho"
        elif ap > 3:    v = "imprescindible"
        else:           v = "se queda"
        pon(2, f"g:{r[0]}", "grupos", r[0], 0, "bench/ablacion.tsv",
            f"aporta {ap:+.2f}% de ratio · cuesta {cu:+.0f}% de tiempo — {v}")

with open("bench/piramide-vccm.tsv", "w", newline="") as f:
    f.write("# LA PIRAMIDE DE vc-cm, nuestra alternativa a zstd-19.\n")
    f.write("# El bucle caliente va POR BIT: un cronometro por fase costaria mas que lo medido y\n")
    f.write("# cambiaria el resultado. Para un bucle asi el instrumento correcto es el MUESTREO\n")
    f.write("# (perf), que no toca el codigo. A cambio da porcentajes y no segundos, y se dice.\n")
    f.write("# CADA FILA LLEVA SU FUENTE porque no todas vienen del mismo sitio, y mezclarlas sin\n")
    f.write("# decirlo seria justo lo que este proyecto lleva el dia desmontando.\n")
    f.write("# Lo escribe bench/piramide-vccm.py.\n")
    w = csv.writer(f, delimiter="\t")
    w.writerow(["nivel", "id", "padre", "nombre", "pct_tiempo", "fuente", "nota", "extra"])
    for r in filas:
        w.writerow(r)

print(f"bench/piramide-vccm.tsv: {len(filas)} peldaños")
print(f"  el perfil cubre el {total_perf:.1f}% del tiempo")
print()
hijos = {}
for r in filas: hijos.setdefault(r[2], []).append(r)
def pinta(i, prof):
    for r in sorted(hijos.get(i, []), key=lambda x: -float(x[4] or 0)):
        print(f"  {'   '*prof}{r[3][:52]:<52}{(r[4]+'%') if r[4] else '':>8}  {r[5]}")
        pinta(r[1], prof+1)
pinta("", 0)
