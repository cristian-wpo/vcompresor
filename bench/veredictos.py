#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""LOS VEREDICTOS: que se queda, que se va, y con que numero.

QUE CAMBIA EN ESTA VERSION, Y POR QUE. La primera version tenia las medidas ESCRITAS A MANO
dentro del propio guion, copiadas del registro. Daba una tabla correcta —los numeros eran los
que salieron— y aun asi el camino estaba mal, porque al copiar el numero se dejaba atras todo
lo que iba con el: la carga de la maquina, el commit, la hora. Y sin la carga no hay forma de
saber si las dos mitades de una comparacion son comparables.

No eran. bench/historia.jsonl lo dice desde el primer momento:

    JXL     = run a CARGA 56,3  contra run a CARGA  6,5
    agrupar = run a CARGA 53,8  contra run a CARGA 56,3

A carga 56 en 12 nucleos la maquina no es la misma maquina. Ni los CICLOS se salvan: el mismo
trabajo gasta mas ciclos cuando la cache esta compartida con media flota, porque los fallos se
pagan en ciclos parados.

TRES COSAS QUE AHORA PUEDE HACER Y ANTES NO:

  1. USA TODAS LAS PAREJAS, no una elegida a mano. Cada medida se reduce a «que piezas tenia
     apagadas», y dos medidas forman pareja cuando se diferencian en EXACTAMENTE UNA pieza.
     Asi aparecen solas parejas que nadie habia mirado —la de agrupar dentro de --no-jpeg, que
     resulta ser la unica limpia del arbol grande— y ninguna se cuela por descuido.
  2. RECHAZA la pareja si las cargas no son comparables, y lo DICE en vez de callarselo.
  3. Calcula el SUELO DE RUIDO repitiendo la base, y marca «NO MEDIBLE» lo que caiga por
     debajo. Tres de los cinco carriles de la tabla anterior estaban ahi y la tabla no lo decia.

POR QUE EN EUROS. Una pieza que se come el 42% del tiempo y compra el 3,9% del tamaño no se
puede juzgar con esos dos numeros: el tiempo se paga UNA VEZ al comprimir y el tamaño se paga
TODOS LOS MESES mientras el dato viva. El modelo, entero, para que se pueda discutir:
    1 PB de entrada, comprimido UNA vez, guardado 12 meses.
    disco = bytes x precio/TB-mes x 12 · CPU = segundos x (46 EUR/mes / 720 h) / 3600
LO QUE NO ENTRA: descomprimir, la red y las peticiones. Los tres favorecen al que comprime mas,
asi que dejarlos fuera perjudica a las piezas que comprimen: es la hipotesis conservadora.
"""
import csv
import json
import collections
import itertools

EUR_H = 46.0 / (30 * 24)
PB = 1e15
PRECIOS = [("S3", 21.30), ("Backblaze", 5.55), ("bulk", 2.50), ("disco", 1.20)]

# Cuando dos cargas dejan de ser comparables. No hay ciencia detras del umbral: hay que poner
# uno, y este deja pasar 4,4 contra 6,8 (la tanda limpia) y corta 6,5 contra 56,3.
CARGA_MAX, CARGA_DIF = 12.0, 4.0

# Como se escribe cada pieza en el campo «config» del registro. Dos notaciones porque unas
# ablaciones se hicieron con banderas y otras con variables de entorno.
PIEZAS = {
    "agrupar":  ["--no-group", "sin agrupar"],
    "JXL":      ["--no-jpeg", "sin JXL"],
    "preflate": ["sin preflate"],
    "E8E9":     ["sin E8E9"],
    "log-ts":   ["sin log-ts"],
}
TODAS = frozenset(PIEZAS)


def apagadas(cfg):
    """Que piezas tenia apagadas esta medida. None si la config no se entiende: mejor dejarla
    fuera que meterla en una pareja adivinando."""
    c = cfg.strip()
    if not c:
        return None
    if c.startswith("solo zstd"):
        return TODAS
    if c in ("completo", "base"):
        return frozenset()
    fuera = {p for p, marcas in PIEZAS.items() if any(m in c for m in marcas)}
    resto = c
    for marcas in PIEZAS.values():
        for m in marcas:
            resto = resto.replace(m, "")
    # lo que sobra tiene que ser solo el nivel; si hay otra cosa, no se entiende la config
    resto = resto.replace("--level", "").strip()
    if resto and not resto.replace(" ", "").isdigit():
        return None
    return frozenset(fuera)


def nivel(cfg, cmd):
    for t in (cfg + " " + cmd).split():
        pass
    p = (cfg + " " + cmd).split()
    for i, t in enumerate(p):
        if t in ("--level", "-l") and i + 1 < len(p):
            return p[i + 1]
    return "19"


R = [json.loads(l) for l in open("bench/historia.jsonl")]

# tanda -> (piezas apagadas, nivel) -> [medidas]
G = collections.defaultdict(lambda: collections.defaultdict(list))
for r in R:
    if "ablacion" not in r.get("etiqueta", "") or r.get("rc") != 0:
        continue
    a = apagadas(r.get("config", ""))
    if a is None:
        continue
    tanda = r["etiqueta"].split(":")[0].strip()
    G[(tanda, r["muestra"])][(a, nivel(r.get("config", ""), r.get("cmd", "")))].append(r)


def med(rs, k):
    return sum(r[k] for r in rs) / len(rs)


filas, descartes = [], []
for (tanda, muestra), casos in sorted(G.items()):
    # EL SUELO DE RUIDO: cualquier configuracion repetida sirve para estimarlo. Sin ninguna
    # repeticion el suelo es DESCONOCIDO, y eso se dice en vez de suponer cero.
    suelos = [100 * (max(cs) - min(cs)) / min(cs)
              for rs in casos.values() if len(rs) > 1
              for cs in [[r["ciclos"] for r in rs]]]
    suelo = max(suelos) if suelos else None
    suelo_txt = f"{suelo:.2f}%" if suelo is not None else "desconocido (nada se repitio)"

    # LAS PAREJAS: dos configuraciones que se diferencian en exactamente una pieza.
    for ka, kb in itertools.permutations(list(casos), 2):
        (a, na), (b, nb) = ka, kb
        if na != nb or not (a < b) or len(b - a) != 1:
            continue
        pieza = next(iter(b - a))
        con, sin = casos[ka], casos[kb]         # «con» la pieza puesta, «sin» ella
        cl, sl = med(con, "carga"), med(sin, "carga")
        ctx = "sola" if not a else "sin " + "+".join(sorted(a))

        if max(cl, sl) > CARGA_MAX or abs(cl - sl) > CARGA_DIF:
            descartes.append(f"{tanda} · {muestra} · {pieza} ({ctx}): cargas {cl:.1f} contra "
                             f"{sl:.1f} — NO comparable")
            continue

        b_c, b_s, b_b, b_i = (med(con, k) for k in ("ciclos", "seg", "bytes_out", "bytes_in"))
        s_c, s_s, s_b = (med(sin, k) for k in ("ciclos", "seg", "bytes_out"))
        pct_c = 100 * (b_c - s_c) / b_c
        medible = "?" if suelo is None else ("si" if abs(pct_c) >= suelo else "NO")

        esc = PB / b_i
        cpu = (b_s - s_s) * esc / 3600 * EUR_H
        tb = (s_b - b_b) * esc / 1e12
        f = {"tanda": tanda, "muestra": muestra, "pieza": pieza, "ctx": ctx,
             "pct_c": pct_c, "pct_t": 100 * (b_s - s_s) / b_s, "pct_b": 100 * (s_b - b_b) / b_b,
             "cpu": cpu, "medible": medible, "suelo": suelo_txt,
             "carga": max(cl, sl), "n": f"{len(con)}+{len(sin)}", "nivel": na}
        for p, pr in PRECIOS:
            f[p] = tb * pr * 12
        filas.append(f)


def veredicto(r):
    if r["medible"] == "NO":
        return "NO MEDIBLE — por debajo del ruido del banco"
    if r["medible"] == "?":
        return "SIN SUELO — nada se repitio, no se puede afirmar"
    if r["pct_b"] < 0.05:
        return "NO COMPRA NADA"
    rel = [r[p] / max(r["cpu"], 1e-9) for p, _ in PRECIOS]
    if min(rel) >= 1.2:
        return "PAGA a todos los precios"
    if max(rel) < 1.0:
        return "PIERDE a todos los precios"
    return "DEPENDE del precio"


filas.sort(key=lambda r: (r["pieza"], r["muestra"]))
with open("bench/veredictos.tsv", "w", newline="") as f:
    f.write("# LOS VEREDICTOS. Se LEEN de bench/historia.jsonl: asi la carga de la maquina viaja\n")
    f.write("# con el numero. La version anterior los tenia copiados a mano y publicaba una\n")
    f.write("# pareja de carga 56 contra una de carga 6,5 sin poder saberlo.\n")
    f.write("# «contexto» = que otras piezas estaban apagadas mientras se medio esta.\n")
    f.write("# 1 PB, 12 meses, CPU a %.4f EUR/hora. No entran descomprimir, red ni peticiones.\n" % EUR_H)
    f.write("# Lo escribe bench/veredictos.py.\n")
    w = csv.writer(f, delimiter="\t")
    w.writerow(["pieza", "corpus", "contexto", "pct_ciclos", "pct_reloj", "pct_bytes",
                "sobre_el_ruido", "suelo_de_ruido", "carga_max", "n", "cuesta_CPU_eur_PB"] +
               [f"ahorra_{p}_eur_PB_ano" for p, _ in PRECIOS] + ["veredicto"])
    for r in filas:
        r["v"] = veredicto(r)
        w.writerow([r["pieza"], r["muestra"], r["ctx"], f"{r['pct_c']:.2f}", f"{r['pct_t']:.2f}",
                    f"{r['pct_b']:.3f}", r["medible"], r["suelo"], f"{r['carga']:.1f}", r["n"],
                    f"{r['cpu']:.0f}"] + [f"{r[p]:.0f}" for p, _ in PRECIOS] + [r["v"]])

print(f"bench/veredictos.tsv: {len(filas)} parejas validas · {len(descartes)} descartadas\n")
if descartes:
    print("  DESCARTADAS POR CARGA (la version anterior publicaba dos de estas):")
    for d in descartes:
        print("    " + d)
    print()
print("  %-9s %-11s %-14s %9s %9s %9s %9s   %s" %
      ("pieza", "corpus", "contexto", "% ciclos", "% reloj", "% bytes", "CPU €/PB", "veredicto"))
for r in filas:
    print("  %-9s %-11s %-14s %8.2f%% %8.2f%% %8.3f%% %9.0f   %s" %
          (r["pieza"], r["muestra"], r["ctx"], r["pct_c"], r["pct_t"], r["pct_b"], r["cpu"], r["v"]))
print()
for t in sorted({(r["tanda"], r["muestra"], r["suelo"]) for r in filas}):
    print(f"  suelo de ruido de «{t[0]} · {t[1]}»: {t[2]}")
print()
for r in filas:
    if r["medible"] == "si":
        rel = {p: r[p] / max(r["cpu"], 1e-9) for p, _ in PRECIOS}
        print(f"  {r['pieza']:<9} {r['muestra']:<11} " +
              " · ".join(f"{p} {rel[p]:5.2f}x" for p, _ in PRECIOS))
