#!/usr/bin/env python3
# QUE CONTESTA LA REJILLA QUE EL BARRIDO POR COORDENADAS NO PUEDE.
#
# Tres preguntas, y las tres estaban abiertas:
#
#   1. ¿INTERACTUAN LAS PERILLAS? Se mide restando: el mejor punto de la rejilla ENTERA contra el
#      mejor punto que encuentra OFAT (mover una perilla dejando las demas en su defecto, y luego
#      juntar los ganadores). Si los dos coinciden, no interactuan y el barrido barato bastaba.
#      Si la rejilla gana, LA DIFERENCIA ES EXACTAMENTE lo que OFAT no puede ver. No es una
#      estimacion: son dos numeros medidos.
#
#      ⚠️ Y hay un modo de fallo peor que «OFAT encuentra menos»: que la config que OFAT propone
#      —juntar los ganadores de cada coordenada— sea PEOR que el defecto. Pasa cuando dos
#      perillas se compensan: cada una gana por separado y juntas se estorban. OFAT no puede
#      detectarlo porque nunca corre esa combinacion. La rejilla si, y se dice.
#
#   2. LOS TRES NIVELES (por muestra / por tipo / global), que es la pregunta del dueño:
#      - INDIVIDUAL: el mejor ajuste PARA CADA muestra. Es la cota del oraculo.
#      - GLOBAL: un solo ajuste para todas.
#      La brecha entre los dos es EL PRESUPUESTO DE LA SELECCION ADAPTATIVA: lo maximo que se
#      podria ganar detectando el tipo de dato y ajustando por el. Si es pequeña, la sonda de
#      familia no merece la pena; si es grande, es la siguiente pieza a construir.
#
#   3. ¿GANA ALGUNA EN EL BORDE? Heredado de mezclador.sh, porque un ganador en el extremo del
#      barrido no es un resultado: es un barrido que se quedo corto.
import os, sys, itertools
from collections import defaultdict

RAIZ = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
TSV = os.path.join(RAIZ, "bench/rejilla.tsv")

DEFECTOS = {"VC_MIX_LR": 12, "VC_DECAY": 0, "VC_ISSE_LR": 6, "VC_SSE": 5}
TOPES = {"VC_MIX_LR": {"alto"}, "VC_DECAY": {"bajo"}, "VC_ISSE_LR": set(), "VC_SSE": {"alto"}}


def carga():
    filas, nombres = [], None
    with open(TSV) as f:
        for ln in f:
            if ln.startswith("#"):
                continue
            c = ln.rstrip("\n").split("\t")
            if c[0] == "muestra":
                nombres = c[2:-1]
                continue
            if nombres is None or len(c) != len(nombres) + 3:
                continue
            filas.append((c[0], int(c[1]), tuple(int(x) for x in c[2:-1]), int(c[-1])))
    return nombres, filas


def pct(a, b):
    return 100.0 * (a - b) / b


def main():
    if not os.path.exists(TSV):
        sys.exit(f"no hay {TSV} todavia")
    nombres, filas = carga()
    if not filas:
        sys.exit("el TSV esta vacio")

    # muestra -> config -> bytes
    por = defaultdict(dict)
    for m, tam, cfg, b in filas:
        por[(m, tam)][cfg] = b

    idef = tuple(DEFECTOS[n] for n in nombres)
    vals = {n: sorted({c[i] for c in por[next(iter(por))]}) for i, n in enumerate(nombres)}

    print("=" * 92)
    print("REJILLA COMPLETA DEL MEZCLADOR · %d perillas · %s" % (len(nombres), " x ".join(
        f"{n}({len(vals[n])})" for n in nombres)))
    print("=" * 92)

    completas = {}
    for clave, d in sorted(por.items()):
        total = 1
        for n in nombres:
            total *= len(vals[n])
        if len(d) < total:
            print(f"  {clave[0]}: {len(d)}/{total} medidas — INCOMPLETA, se salta")
            continue
        completas[clave] = d

    if not completas:
        sys.exit("ninguna muestra tiene la rejilla completa todavia")

    # ---------- 1. ¿interactuan? ----------
    print("\n1) ¿INTERACTUAN LAS PERILLAS? (rejilla completa contra lo que OFAT puede ver)\n")
    print(f"  {'muestra':<10} {'defecto':>9} {'OFAT':>9} {'rejilla':>9} {'OFAT vs def':>12} "
          f"{'rejilla vs def':>15} {'lo que OFAT NO VE':>18}")
    print("  " + "-" * 88)
    resumen = []
    for (m, tam), d in completas.items():
        base = d[idef]
        # lo que ve OFAT: por cada perilla, el mejor valor con las demas EN SU DEFECTO
        ofat_cfg = []
        for i, n in enumerate(nombres):
            mejor, mejorb = DEFECTOS[n], base
            for v in vals[n]:
                c = list(idef)
                c[i] = v
                b = d[tuple(c)]
                if b < mejorb:
                    mejor, mejorb = v, b
            ofat_cfg.append(mejor)
        ofat_cfg = tuple(ofat_cfg)
        ofat_b = d[ofat_cfg]
        mejor_cfg = min(d, key=lambda c: d[c])
        mejor_b = d[mejor_cfg]
        resumen.append((m, tam, base, ofat_cfg, ofat_b, mejor_cfg, mejor_b))
        print(f"  {m:<10} {base:>9} {ofat_b:>9} {mejor_b:>9} {pct(ofat_b,base):>+11.3f}% "
              f"{pct(mejor_b,base):>+14.3f}% {pct(mejor_b,ofat_b):>+17.3f}%")

    print()
    for m, tam, base, oc, ob, mc, mb in resumen:
        eti_o = " ".join(f"{n}={v}" for n, v in zip(nombres, oc) if v != DEFECTOS[n]) or "(el defecto)"
        eti_m = " ".join(f"{n}={v}" for n, v in zip(nombres, mc) if v != DEFECTOS[n]) or "(el defecto)"
        print(f"  {m:<10} OFAT propone: {eti_o}")
        print(f"  {'':<10} la rejilla:   {eti_m}")
        if ob > base:
            print(f"  {'':<10} ⚠️ JUNTAR LOS GANADORES DE OFAT SALE PEOR QUE EL DEFECTO "
                  f"({pct(ob,base):+.3f}%): dos perillas se compensan y OFAT no puede verlo.")

    # ---------- 2. los tres niveles ----------
    print("\n2) LOS TRES NIVELES · ¿cuanto vale ajustar por tipo de dato?\n")
    comunes = None
    for d in completas.values():
        comunes = set(d) if comunes is None else (comunes & set(d))
    suma_def = sum(d[idef] for d in completas.values())
    glob_cfg = min(comunes, key=lambda c: sum(d[c] for d in completas.values()))
    suma_glob = sum(d[glob_cfg] for d in completas.values())
    suma_ind = sum(min(d.values()) for d in completas.values())
    print(f"  defecto de hoy (una config para todas) : {suma_def:>10} B")
    print(f"  GLOBAL   (la mejor config unica)       : {suma_glob:>10} B  {pct(suma_glob,suma_def):+.3f}%")
    print(f"  INDIVIDUAL (la mejor POR muestra)      : {suma_ind:>10} B  {pct(suma_ind,suma_def):+.3f}%")
    print()
    print(f"  la mejor config unica: " + (" ".join(f"{n}={v}" for n, v in zip(nombres, glob_cfg)
                                                   if v != DEFECTOS[n]) or "(el defecto)"))
    print(f"  🔑 PRESUPUESTO DE LA SELECCION ADAPTATIVA (global -> individual): "
          f"{pct(suma_ind, suma_glob):+.3f}%")
    print("     Es el techo de lo que se puede ganar detectando el tipo de dato y ajustando por")
    print("     el. Nadie puede cobrar mas que eso con una sonda de familia.")
    print()
    for (m, tam), d in completas.items():
        mc = min(d, key=lambda c: d[c])
        eti = " ".join(f"{n}={v}" for n, v in zip(nombres, mc) if v != DEFECTOS[n]) or "(el defecto)"
        print(f"    {m:<10} su optimo: {eti:<45} {pct(d[mc], d[glob_cfg]):+.3f}% bajo el global")

    # ---------- 3. bordes ----------
    print("\n3) ¿GANA ALGUNA EN EL BORDE DEL BARRIDO?\n")
    hay = False
    for (m, tam), d in completas.items():
        mc = min(d, key=lambda c: d[c])
        for i, n in enumerate(nombres):
            v = mc[i]
            if v == min(vals[n]) and "bajo" not in TOPES[n]:
                print(f"  ⚠️ {m}: {n}={v} gana EN EL BORDE BAJO — el barrido se quedo corto")
                hay = True
            if v == max(vals[n]) and "alto" not in TOPES[n]:
                print(f"  ⚠️ {m}: {n}={v} gana EN EL BORDE ALTO — el barrido se quedo corto")
                hay = True
    if not hay:
        print("  ninguno. Todos los ganadores caen dentro del barrido o en un tope real.")

    # ---------- 4. efectos e interacciones ----------
    print("\n4) LA INTERACCION MAS FUERTE POR PAREJAS (media sobre todas las muestras)\n")
    pares = []
    for i, j in itertools.combinations(range(len(nombres)), 2):
        peor = 0.0
        for d in completas.values():
            base = d[idef]
            for vi in vals[nombres[i]]:
                for vj in vals[nombres[j]]:
                    c = list(idef); c[i] = vi; c[j] = vj
                    ci = list(idef); ci[i] = vi
                    cj = list(idef); cj[j] = vj
                    # cuanto se desvia el efecto conjunto de la SUMA de los efectos sueltos
                    e_ij = d[tuple(c)] - base
                    e_i = d[tuple(ci)] - base
                    e_j = d[tuple(cj)] - base
                    peor = max(peor, abs(e_ij - (e_i + e_j)) / base * 100)
        pares.append((peor, nombres[i], nombres[j]))
    for p, a, b in sorted(pares, reverse=True):
        print(f"  {a:<12} x {b:<12} desviacion maxima de la aditividad: {p:.3f}%")
    print("\n  Una desviacion grande significa que el efecto conjunto NO es la suma de los")
    print("  efectos sueltos — o sea, que un barrido por coordenadas no puede predecir el punto.")


if __name__ == "__main__":
    main()
