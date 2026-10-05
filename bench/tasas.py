# -*- coding: utf-8 -*-
"""LAS TASAS DE APRENDIZAJE: la marca de «hoy» SE LEE DEL CODIGO, no se escribe a mano.

POR QUE EXISTE ESTE GUION, y es un fallo que costo caro hoy. Las tres tablas —tasa-mezclador,
tasa-statemap, tasa-isse— estaban ESCRITAS A MANO con una etiqueta «(hoy)» al lado de un valor.
Era cierta cuando se midio y dejo de serlo al entrar el modelo v3, que cambio las dos primeras.
Nadie actualizo la etiqueta.

Consecuencia: leyendo el banco se concluye que «dos perillas estan puestas en valores que el
propio banco midio como peores, y valen un 1,1%». Se dijo dos veces. Y era falso: ya estaban
aplicadas. Comprobado a la fuerza, forzando los valores viejos sobre 2 MiB de codigo:
    por defecto (v3)          227.711 B
    forzando el paso >>10     231.651 B   +1,7303%
    forzando el tope 1023     228.127 B   +0,1827%
    los dos viejos a la vez   232.072 B   +1,9151%

Es R7 —«nunca enseñar lo viejo como si fuera de ahora»— dentro del propio banco. Y la unica
defensa que funciona no es acordarse: es que la etiqueta NO SE PUEDA escribir a mano.

QUE HACE. Lee `MODELO` y `tasas_de_modelo()` del fuente de Rust, marca el valor vigente, recalcula
el «vs actual» contra el, y AVISA A GRITOS si el optimo medido no es el vigente. Las medidas se
conservan tal cual: son datos y no se tocan.

⚠️ Y DICE DE QUE EPOCA SON. Estas medidas se tomaron con el modelo de entonces; desde entonces
han entrado modelos nuevos y la cadena ISSE. El propio 1,9151% de arriba, remedido hoy, es mayor
que el -1,111% que decia la tabla: el modelo actual aprovecha MAS el aprendizaje lento. Una tabla
de medidas viejas no es falsa, pero tampoco es de ahora, y eso tiene que estar escrito.
"""
import csv, io, os, re, sys

FUENTE = "crates/vc-cm/src/lib.rs"
s = io.open(FUENTE, encoding="utf-8").read()

m = re.search(r"pub const MODELO: u8 = (\d+);", s)
if not m: sys.exit("no encuentro MODELO en el fuente")
MODELO = int(m.group(1))

# `tasas_de_modelo` devuelve (tope_statemap, paso_mezclador). Se lee el brazo que aplica a la
# version vigente: el `_ =>` si no hay uno explicito para ella.
cuerpo = re.search(r"fn tasas_de_modelo\(modelo: u8\) -> \(usize, u32\) \{(.*?)\n\}", s, re.S)
if not cuerpo: sys.exit("no encuentro tasas_de_modelo")
brazos = re.findall(r"([^=>\n]+)=>\s*\((\d+),\s*(\d+)\)", cuerpo.group(1))
LIM = PASO = None
for cond, a, b in brazos:
    cond = cond.strip()
    if cond == "_":
        if LIM is None: LIM, PASO = int(a), int(b)
    else:
        vals = {int(x) for x in re.findall(r"\d+", cond)}
        if MODELO in vals: LIM, PASO = int(a), int(b); break
if LIM is None: sys.exit("no deduzco las tasas vigentes")

# ANCLADO A LA VARIABLE DE ENTORNO, no al nombre del campo. La primera version buscaba
# "isse_lr:" y enganchaba la DECLARACION del campo en el struct, no su valor por defecto:
# leia 3 donde el codigo pone 6, y regeneraba la tabla con la base equivocada. Un generador
# que existe para que las cifras no se escriban a mano no puede leerlas a ojo.
m2 = re.search(r'VC_ISSE_LR"\).*?unwrap_or\((\d+)\)', s, re.S)
if not m2: sys.exit("no deduzco la tasa ISSE vigente — se para en vez de inventarla")
ISSE = int(m2.group(1))

print(f"\n  LEIDO DEL CODIGO: MODELO = v{MODELO} · tope StateMap = {LIM} · paso mezclador = >>{PASO}"
      f" · tasa ISSE = {ISSE}\n")

# tabla · columna del valor · valor vigente · como se escribe el valor en la tabla
TABLAS = [
    ("tasa-mezclador.tsv", "paso",  PASO, lambda v: f">>{v}",
     "el paso del mezclador: cuanto corrige los pesos en cada bit"),
    ("tasa-statemap.tsv",  "tope",  LIM,  lambda v: str(v),
     "el tope de observaciones del StateMap: cuanto se congela un contexto maduro"),
    ("tasa-isse.tsv",      "tasa",  ISSE, lambda v: str(v),
     "cuanto corrige cada etapa de la cadena ISSE"),
]

avisos = []
for fich, col, vigente, fmt, que in TABLAS:
    ruta = os.path.join("bench", fich)
    try:
        filas = [l.rstrip("\n").split("\t") for l in io.open(ruta, encoding="utf-8")
                 if l.strip() and not l.startswith("#")]
    except FileNotFoundError:
        print(f"  {fich}: no esta"); continue
    cab, datos = filas[0], filas[1:]
    # El valor de cada fila: el primer numero de la primera columna.
    def val(f):
        n = re.findall(r"\d+", f[0])
        return int(n[0]) if n else None
    # La base es la fila del valor VIGENTE, no la que lleve la etiqueta escrita a mano.
    base = next((f for f in datos if val(f) == vigente), None)
    if base is None:
        avisos.append(f"{fich}: el valor vigente ({vigente}) NO ESTA MEDIDO en la tabla")
        continue
    b = float(base[1])
    mejor = min(datos, key=lambda f: float(f[1]))
    salida = [[fmt(val(f)) + ("  <-- VIGENTE" if val(f) == vigente else ""),
               f[1], f"{100*(float(f[1])-b)/b:+.3f}"] for f in datos]
    with open(ruta, "w", newline="") as fh:
        fh.write(f"# {que.upper()}\n")
        fh.write("# LA MARCA «VIGENTE» SE LEE DEL CODIGO, no se escribe a mano. Antes se escribia,\n")
        fh.write("# y al cambiar el modelo v3 dejo de ser cierta sin que nadie la tocara: leyendo\n")
        fh.write("# esta tabla se concluia que habia un 1,1% sin aplicar que YA estaba aplicado.\n")
        fh.write(f"# Vigente hoy: {fmt(vigente)} (modelo v{MODELO}). «vs_vigente» se recalcula contra el.\n")
        fh.write("# ⚠️ LAS MEDIDAS SON DE SU EPOCA. Se tomaron con el modelo de entonces; desde\n")
        fh.write("# entonces han entrado modelos nuevos y la cadena ISSE. No son falsas, pero no\n")
        fh.write("# son de ahora — remedido hoy, volver a los valores viejos cuesta 1,9151% y la\n")
        fh.write("# tabla decia 1,111%: el modelo actual aprovecha MAS el aprendizaje lento.\n")
        fh.write("# Lo escribe bench/tasas.py.\n")
        w = csv.writer(fh, delimiter="\t")
        w.writerow([col, "total", "vs_vigente"]); w.writerows(salida)
    vm = val(mejor)
    marca = "✓" if vm == vigente else "⚠️"
    print(f"  {marca} {fich:<22} vigente {fmt(vigente):<6} · mejor medido {fmt(vm):<6} "
          f"({100*(float(mejor[1])-b)/b:+.3f}%)")
    if vm != vigente:
        avisos.append(f"{fich}: el optimo MEDIDO es {fmt(vm)} y corre {fmt(vigente)} "
                      f"({100*(float(mejor[1])-b)/b:+.3f}%) — remedir con el modelo de hoy antes de mover")

print()
for a in avisos: print(f"  ⚠️  {a}")
if not avisos: print("  las tres perillas corren en su optimo medido")
print()
