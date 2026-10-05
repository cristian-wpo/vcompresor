# -*- coding: utf-8 -*-
"""¿QUE CARRIL SE PAGA A QUE PRECIO? La tabla que le falta a `--price` para poder decidir.

DE DONDE VIENE. D8 dice desde hace semanas que `--price` optimiza la perilla que no importa:
sondea para elegir el NIVEL de zstd —que entre el 17 y el 19 mueve un 0,8% del tamaño— y no toca
los CARRILES. D13 lo puso en numeros: la decision de carril mueve un 43,6% de bytes y un 99,4%
de CPU. Falta lo que aqui se calcula: por carril, cuanto COMPRA y cuanto CUESTA.

LA CUENTA, y sus dos sumandos no se parecen en nada:
  · lo que COMPRA un carril son bytes que no se guardan, y el disco se alquila TODOS LOS MESES
  · lo que CUESTA es CPU, y la CPU se paga UNA VEZ

Por eso un carril caro en CPU puede pagarse de sobra si guarda mucho y el dato se conserva
años, y no pagarse nada si se borra en un mes. El resultado no es «este carril es bueno»: es
«este carril se paga por encima de tal precio y tal retencion», que es justo lo que --price
tendria que estar decidiendo por el usuario y no decide.

LO QUE NO SE PUBLICA: una diferencia por debajo del suelo de ruido de la maquina. Un carril
cuyo coste no se distingue del ruido no es un carril barato, es un carril SIN MEDIR, y se dice.
Lo escribe bench/carriles.py con las filas «F13 carril:» de bench/historia.jsonl.
"""
import csv, json, re, sys

PRECIOS = [("S3", 21.30), ("Backblaze", 5.55), ("bulk", 2.50), ("disco", 1.20)]
EUR_H = 46.0 / (30 * 24)          # la misma maquina que usa bench/dinero.py
RETENCIONES = [1, 12, 60]         # un mes, un año, cinco años
VUELTA = re.compile(r"\s*\(vuelta\s*\d+\)\s*$")

# ── lo medido ────────────────────────────────────────────────────────────────────────────────
H = []
for l in open("bench/historia.jsonl", encoding="utf-8"):
    l = l.strip()
    if not l: continue
    try: H.append(json.loads(l))
    except json.JSONDecodeError: pass

G = {}
for r in H:
    if not r.get("etiqueta", "").startswith("F13 carril:"): continue
    if r.get("rc") != 0: continue
    G.setdefault(r.get("config", ""), []).append(r)

if "completo" not in G:
    sys.exit("  No hay filas «F13 carril: completo». Corre bench/carriles.sh primero:\n"
             "  esta tabla no lleva ninguna cifra escrita a mano.")

def med(rs, k):
    v = [r[k] for r in rs if isinstance(r.get(k), (int, float))]
    return sum(v) / len(v) if v else 0.0
def cpu(rs):
    v = [r["seg"] * r["cpu_pct"] / 100 for r in rs if r.get("seg") and r.get("cpu_pct")]
    return sum(v) / len(v) if v else 0.0

# ── EL SUELO DE RUIDO, con la misma regla que el panel: n>=3, arbol limpio ───────────────────
ctrl = {}
for r in H:
    et = VUELTA.sub("", r.get("etiqueta", ""))
    if not et.startswith("CERO") or r.get("sucio") or not r.get("seg"): continue
    ctrl.setdefault((et, r.get("commit")), []).append(r["seg"])
rel = []
for v in ctrl.values():
    if len(v) < 3: continue
    m = sum(v) / len(v)
    sd = (sum((x - m) ** 2 for x in v) / (len(v) - 1)) ** 0.5
    if m > 0: rel.append(sd / m)
RUIDO = max(rel) if rel else None

base = G["completo"]
b_bytes, b_cpu, b_in = med(base, "bytes_out"), cpu(base), med(base, "bytes_in")
carga_base = [r.get("carga", 0) for r in base]

filas = []
for cfg, rs in sorted(G.items()):
    if cfg == "completo": continue
    carril = cfg.replace("sin-", "")
    sin_b, sin_cpu = med(rs, "bytes_out"), cpu(rs)
    compra = sin_b - b_bytes          # bytes que el carril AHORRA
    coste = b_cpu - sin_cpu           # CPU-s que el carril CUESTA
    # ¿Se distingue del ruido? El ruido esta medido sobre el RELOJ y se aplica al coste de CPU,
    # que es lo que lo arrastra. Los BYTES no tienen ruido: dos vueltas de la misma
    # configuracion dan el MISMO archivo, y eso esta verificado en el control de cero.
    inc = (1.414 * RUIDO * max(b_cpu, sin_cpu)) if RUIDO else 0.0
    cargas = carga_base + [r.get("carga", 0) for r in rs]
    # ── LA TRAMPA QUE SE LLEVA POR DELANTE UNA ABLACION ENTERA ──────────────────────────────
    # Si «sin X» saca EXACTAMENTE los mismos bytes que «completo», no son dos configuraciones:
    # la bandera no llego a hacer nada y la ablacion se esta comparando consigo misma. El
    # resultado —«ese carril compra 0,000%»— es perfectamente creible y perfectamente falso.
    # Paso de verdad con `agrupar` el 2026-07-31: el defecto habia cambiado y `--group` era un
    # no-op, asi que las dos configuraciones agrupaban. Aqui NO se publica: se denuncia.
    if int(sin_b) == int(b_bytes):
        filas.append({"carril": carril, "n": len(rs), "compra_b": 0.0, "compra_pct": 0.0,
                      "coste_cpu": 0.0, "coste_pct": 0.0, "medible": False, "inc": 0.0,
                      "bandera_muerta": True,
                      "carga_min": 0, "carga_max": 0})
        continue
    filas.append({"bandera_muerta": False, "carril": carril, "n": len(rs), "compra_b": compra,
                  "compra_pct": 100.0 * compra / b_bytes if b_bytes else 0.0,
                  "coste_cpu": coste,
                  "coste_pct": 100.0 * coste / b_cpu if b_cpu else 0.0,
                  "medible": abs(coste) > inc, "inc": inc,
                  "carga_min": min(cargas), "carga_max": max(cargas)})
filas.sort(key=lambda f: -f["compra_b"])

def decide(f, precio, meses):
    """€ que ahorra el carril menos € que cuesta, por PB de ENTRADA. Positivo = se paga."""
    esc = 1e15 / b_in if b_in else 0.0
    return (f["compra_b"] * esc / 1e12) * precio * meses \
         - (f["coste_cpu"] * esc / 3600.0) * EUR_H

with open("bench/carriles.tsv", "w", newline="") as fh:
    fh.write("# QUE CARRIL SE PAGA A QUE PRECIO. Lo escribe bench/carriles.py con las filas\n")
    fh.write("# «F13 carril:» de bench/historia.jsonl. NO se edita a mano.\n")
    fh.write("# «compra» = bytes que el carril ahorra · «cuesta» = CPU-s que añade.\n")
    fh.write("# El disco se alquila TODOS LOS MESES y la CPU se paga UNA VEZ: por eso la\n")
    fh.write("# retencion decide tanto como el precio. Las columnas son € por PB de entrada.\n")
    w = csv.writer(fh, delimiter="\t")
    cab = ["carril", "n", "compra_bytes", "compra_pct", "cuesta_cpu_s", "cuesta_pct", "medible"]
    for nom, _ in PRECIOS:
        for m in RETENCIONES: cab.append(f"{nom}_{m}m")
    w.writerow(cab)
    for f in filas:
        fila = [f["carril"], f["n"], int(f["compra_b"]), f"{f['compra_pct']:.3f}",
                f"{f['coste_cpu']:.1f}", f"{f['coste_pct']:.2f}",
                "si" if f["medible"] else "NO-ruido"]
        for _, pr in PRECIOS:
            for m in RETENCIONES: fila.append(f"{decide(f, pr, m):.0f}")
        w.writerow(fila)

pt = lambda x: f"{x:,}".replace(",", ".")
print()
print(f"  LA ECONOMIA DE CADA CARRIL — arbol {pt(int(b_in))} B, archivo completo {pt(int(b_bytes))} B")
print("  suelo de ruido: " + ("sin medir" if RUIDO is None else f"{100*RUIDO:.2f}%"))
print()
print(f"  {'carril':<12} {'compra (bytes)':>18} {'':>9} {'cuesta CPU':>12} {'':>8}  medible")
for f in filas:
    print(f"  {f['carril']:<12} {pt(int(f['compra_b'])):>18} {f['compra_pct']:>8.3f}% "
          f"{f['coste_cpu']:>11.1f}s {f['coste_pct']:>7.2f}%  "
          + ("si" if f["medible"] else "NO (ruido)"))
print()
print("  ¿SE PAGA? € por PB de entrada: ahorro de disco menos coste de CPU. Positivo = SI.")
print()
print("  " + " " * 12 + "".join(f"{nom+' '+str(m)+'m':>14}" for nom, _ in PRECIOS for m in RETENCIONES))
for f in filas:
    fila = f"  {f['carril']:<12}"
    for _, pr in PRECIOS:
        for m in RETENCIONES: fila += f"{decide(f, pr, m):>14,.0f}".replace(",", ".")
    print(fila)
print()
print("  COMO SE LEE. Cada columna es un precio de almacenamiento y una retencion. Un numero")
print("  NEGATIVO dice que ese carril cuesta mas de lo que ahorra con ese dinero y ese plazo, y")
print("  que --price deberia apagarlo. Hoy --price no apaga ninguno: solo mueve el nivel de")
print("  zstd, que entre el 17 y el 19 mueve un 0,8% del tamaño. Eso es D8.")
muertas = [f["carril"] for f in filas if f.get("bandera_muerta")]
if muertas:
    print()
    print(f"  ⛔ LA BANDERA NO HIZO NADA en: {', '.join(muertas)}. Sacan EXACTAMENTE los mismos")
    print("     bytes que «completo», o sea que la ablacion se comparo consigo misma. Eso no es")
    print("     «ese carril no compra nada»: es que no se ha medido. NO PUBLICAR esa fila.")
sm = [f["carril"] for f in filas if not f["medible"] and not f.get("bandera_muerta")]
if sm:
    print()
    print(f"  ⚠ SIN MEDIR, por debajo del ruido: {', '.join(sm)}. No es que sean baratos: es que")
    print("    su coste no se distingue del ruido de la maquina con estas vueltas. Para decidir")
    print("    sobre ellos hacen falta mas repeticiones, no mas confianza.")
print()
print(f"  bench/carriles.tsv escrito · {len(filas)} carriles")
