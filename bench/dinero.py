# -*- coding: utf-8 -*-
"""Arreglo de MI PROPIO error, y lo delato el resultado: salian los cuatro precios negativos.

QUE HICE MAL. Compare NUESTRO ratio sobre el arbol de hoy (38-43%, porque tiene 12.569 JPEG y
mucho medio ya comprimido) contra el ratio de restic medido sobre OTRO arbol (7,579%, que era
texto y codigo). Comparar el ratio de dos herramientas sobre corpus distintos no significa nada
— y es EXACTAMENTE la clase de error que llevo todo el dia persiguiendo en las tablas viejas.
Lo pille porque el resultado era absurdo, no porque revisara el metodo.

COMO SE ARREGLA. Se quita la comparacion prestada. La pregunta que SI se puede contestar con lo
medido es absoluta y no necesita rival: ¿cuanto cuesta EN TOTAL cada nivel, sumando el alquiler
del disco y la CPU? El minimo de esa suma es la respuesta, y todo esta medido sobre el mismo
arbol y la misma maquina.

Comparar con restic queda como pregunta ABIERTA hasta medirlo sobre este arbol. Se anota.
"""
import csv, json, os, re

# ── DE DONDE SALEN LAS VELOCIDADES ──────────────────────────────────────────────────────────
#
# DEL LIBRO MAYOR, NO DE AQUI. Estuvieron escritas a mano en una lista en este mismo fichero, y
# eran de una tanda en la que cada vuelta costaba 445-548 s. Despues el carril preflate paso a
# correr en paralelo y la misma vuelta tarda ~170 s: la tabla que decide QUE NIVEL SE VENDE POR
# DEFECTO llevaba semanas calculando con una CPU 2,6 veces mas cara de lo que es. Es el mismo
# defecto que la pregunta P10 persigue, en la tabla que mas dinero mueve de todo el banco.
#
# Las escribe bench/niveles.sh con la etiqueta «NIVEL n». Se coge la MAS RECIENTE de cada nivel.
# Si falta alguno, se dice y no se inventa: una fila de menos es un hueco, una fila inventada es
# una decision de producto tomada sobre nada.
NIVELES = [1, 3, 9, 12, 15, 17, 19]
_VIEJA = {1:473.03, 3:457.23, 9:327.09, 12:197.52, 15:82.09, 17:38.47, 19:13.58}

def _del_libro():
    ult = {}
    p = "bench/historia.jsonl"
    if not os.path.exists(p): return ult
    for l in open(p, encoding="utf-8"):
        l = l.strip()
        if not l: continue
        try: r = json.loads(l)
        except json.JSONDecodeError: continue
        m = re.fullmatch(r"NIVEL (\d+)", r.get("etiqueta", ""))
        if not m or r.get("rc") != 0: continue
        if not r.get("bytes_out") or not r.get("seg"): continue
        ult[int(m.group(1))] = r          # el fichero esta en orden, asi que la ultima gana
    return ult

# ── EL SUELO DE RUIDO, del mismo libro mayor y con la misma regla que el panel ───────────────
# La parte de CPU de esta tabla sale de un tiempo medido, asi que arrastra el ruido de la
# maquina. Sin eso, decir «gana el nivel 9» cuando el 12 esta a 38 € y la incertidumbre es
# mayor que 38 € es publicar ruido como si fuera una decision.
def _ruido():
    ctrl = {}
    p = "bench/historia.jsonl"
    if not os.path.exists(p): return None
    for l in open(p, encoding="utf-8"):
        l = l.strip()
        if not l: continue
        try: r = json.loads(l)
        except json.JSONDecodeError: continue
        et = re.sub(r"\s*\(vuelta\s*\d+\)\s*$", "", r.get("etiqueta", ""))
        # Solo los controles de cero, y solo los que se tomaron con el codigo commiteado.
        if not et.startswith("CERO") or r.get("sucio") or not r.get("seg"): continue
        ctrl.setdefault((et, r.get("commit")), []).append(r["seg"])
    rel = []
    for v in ctrl.values():
        if len(v) < 3: continue
        m = sum(v) / len(v)
        sd = (sum((x - m) ** 2 for x in v) / (len(v) - 1)) ** 0.5
        if m > 0: rel.append(sd / m)
    return max(rel) if rel else None      # el PEOR caso, que es el que no puede sorprenderte

RUIDO = _ruido()

_U = _del_libro()
_faltan = [n for n in NIVELES if n not in _U]
if _faltan:
    print(f"  AVISO: sin medida para los niveles {_faltan}. Corre bench/niveles.sh.")
if not _U:
    raise SystemExit("  No hay ni una medida de nivel en el libro mayor. Corre bench/niveles.sh\n"
                     "  primero: esta tabla ya no lleva cifras escritas a mano.")

MED = []
for n in NIVELES:
    r = _U.get(n)
    if not r: continue
    mbs = r["bytes_in"] / r["seg"] / 1048576
    MED.append((n, r["bytes_out"], r["seg"], mbs, _VIEJA.get(n, 0.0)))
ENTRADA = _U[MED[0][0]]["bytes_in"]
_COMMITS = sorted({_U[n]["commit"] for n, *_ in MED})
_CARGAS = [_U[n]["carga"] for n, *_ in MED]
PRECIOS = [("S3",21.30),("Backblaze",5.55),("bulk",2.50),("disco",1.20)]
EUR_H = 46.0/(30*24); PB = 1e15

filas=[]
for niv,out,seg,mbs,tabla in MED:
    ratio = out/ENTRADA
    cpu = (PB/(mbs*1048576))/3600*EUR_H          # EUR de CPU por PB comprimido
    tb  = PB*ratio/1e12                          # TB guardados por PB
    f = {"nivel":niv,"ratio":ratio*100,"mbs":mbs,"tabla":tabla,"err":tabla/mbs,
         "tb":tb,"cpu":cpu,"seg":seg,"out":out}
    for nom,pr in PRECIOS: f[nom] = tb*pr*12 + cpu    # COSTE TOTAL, menor es mejor
    filas.append(f)

with open("bench/economia-nivel.tsv","w",newline="") as fh:
    fh.write("# EL COSTE TOTAL DE CADA NIVEL: alquiler del disco + CPU, por PB de entrada y año.\n")
    fh.write("# MENOR ES MEJOR. No hay comparacion con rivales: para eso haria falta medir a\n")
    fh.write("# restic SOBRE ESTE MISMO ARBOL, y no esta hecho. La version anterior de esta tabla\n")
    fh.write("# comparaba nuestro ratio aqui contra el de restic en OTRO corpus, que no significa\n")
    fh.write("# nada; se quito. Lo escribe bench/dinero.py.\n")
    fh.write(f"# Velocidades LEIDAS de bench/historia.jsonl (etiqueta «NIVEL n»), no escritas aqui:\n")
    fh.write(f"# estuvieron a mano y caducaron cuando el carril preflate paso a paralelo.\n")
    fh.write(f"# commit(s) {', '.join(_COMMITS)} · carga {min(_CARGAS):.1f}-{max(_CARGAS):.1f} · "
             f"entrada {ENTRADA} B\n")
    fh.write("# La columna MB_s_tabla_vieja es la del codec zstd sobre otro corpus, no la\n")
    fh.write("# codec zstd sobre un arbol de 656 ficheros grandes.\n")
    w=csv.writer(fh,delimiter="\t")
    w.writerow(["nivel","ratio_pct","MB_s_real","MB_s_tabla_vieja","error_x","TB_por_PB",
                "CPU_eur_por_PB"]+["coste_"+n for n,_ in PRECIOS])
    for r in filas:
        w.writerow([r["nivel"],f"{r['ratio']:.3f}",f"{r['mbs']:.2f}",f"{r['tabla']:.2f}",
                    f"{r['err']:.0f}",f"{r['tb']:.1f}",f"{r['cpu']:.0f}"]+
                   [f"{r[n]:.0f}" for n,_ in PRECIOS])

print("  COSTE TOTAL por PB de entrada y año (disco 12 meses + CPU). MENOR ES MEJOR.")
print()
print("  %-6s %9s %10s %9s %10s"%("nivel","ratio","MB/s real","err x","CPU €/PB")+
      "".join("%13s"%n for n,_ in PRECIOS))
for r in filas:
    print("  %-6d %8.3f%% %10.2f %8.0fx %10.0f"%(r["nivel"],r["ratio"],r["mbs"],r["err"],r["cpu"])+
          "".join("%13,.0f".replace(",","")%r[n] for n,_ in PRECIOS))
print()
for nom,_ in PRECIOS:
    g=min(filas,key=lambda r:r[nom]); peor=max(filas,key=lambda r:r[nom])
    ah=(peor[nom]-g[nom])
    # ¿Gana de verdad, o gana dentro del ruido? La CPU es la unica parte con incertidumbre
    # -el tamaño es exacto y determinista-, asi que la de la DIFERENCIA entre dos niveles es
    # ~sqrt(2) veces el ruido relativo por su coste de CPU. Si la ventaja no la supera, son
    # dos opciones empatadas y hay que decirlo: elegir una es tirar una moneda con decimales.
    if RUIDO is not None:
        orden = sorted(filas, key=lambda f: f[nom])
        g2 = orden[1] if len(orden) > 1 else None
        if g2:
            inc = 1.414 * RUIDO * max(g["cpu"], g2["cpu"])
            if g2[nom] - g[nom] < inc:
                print(f"  a precio {nom:<12} EMPATAN los niveles {g['nivel']} y {g2['nivel']} "
                      f"({g[nom]:,.0f} y {g2[nom]:,.0f} €/PB-año, se llevan {g2[nom]-g[nom]:,.0f} "
                      f"y la incertidumbre es ±{inc:,.0f} con un ruido del {100*RUIDO:.2f}%)")
                continue
    print(f"  a precio {nom:<12} gana el NIVEL {g['nivel']:>2}  ({g[nom]:,.0f} €/PB-año, "
          f"{ah:,.0f} menos que el peor)")
print()
b,p=filas[0],filas[-1]
print(f"  del nivel {b['nivel']} al {p['nivel']}: {(p['seg']/b['seg']-1)*100:+.1f}% de tiempo, "
      f"{(p['out']/b['out']-1)*100:+.1f}% de tamaño.")
print(f"  la tabla vieja decia que el 19 costaba {b['tabla']/p['tabla']:.0f}x mas tiempo que el 1. "
      f"Cuesta {p['seg']/b['seg']:.1f}x.")
