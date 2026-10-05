# -*- coding: utf-8 -*-
"""EN QUE VERSION ESTAMOS Y QUE SE HA APLICADO.

POR QUE EXISTE. Pregunta del dueño, 31-07: «¿aun no me sirve el panel para ver que se mueve de lo
implementado y en que version estamos?». La respuesta era NO, y por dos motivos distintos:

  · La VERSION no salia en ningun sitio. El panel tiene una clave `versiones`, pero es de los
    RIVALES —que zstd, que xz— no la nuestra. En que modelo de vc-cm estamos, que frame es el
    defecto, que archivos sabemos abrir: nada de eso se veia.

  · La LINEA DE TIEMPO estaba plana desde db76ff6 y parecia averiada. No lo estaba: no tenia nada
    que dibujar. Solo compara EL MISMO experimento medido en DOS commits, y esa tarde se midio
    casi todo en el Mac —el servidor tenia una cadena de reloj ocupandolo— y el Mac NO TIENE
    BASCULA: `mide-mac.sh` lleva su propio cronometro que imprime por pantalla y no escribe en
    `historia.jsonl`, y el Mac ni siquiera es repo git, asi que no puede sellar el commit.

LA DECISION DE DISEÑO, y es la que importa: esto NO tapa el segundo agujero, LO ENSEÑA. La tabla
lleva una columna «medidas en el banco» por commit, y los que van a cero salen marcados. Un panel
que no puede enseñar el movimiento tiene que decir por que, no quedarse plano y que parezca que
no se ha hecho nada.

DE DONDE SALE CADA COSA, que es lo unico que hace fiable a una tabla asi:
  · las versiones, LEIDAS DEL FUENTE de Rust. Como bench/tasas.py, y por la misma razon: las
    tablas escritas a mano con una etiqueta «(hoy)» al lado son las que hicieron afirmar dos veces
    un 1,1% que no existia. Si un ancla no aparece, esto ABORTA en vez de inventar el numero.
  · los commits, de `git log`. No se escriben aqui.
  · la cobertura de medidas, contando en `bench/historia.jsonl` por commit. Tampoco se escribe.

O sea: en este fichero no hay ni un dato a mano. Solo anclas y cuentas.

Lo escribe bench/aplicado.py -> bench/aplicado.tsv + bench/aplicado-version.tsv
"""
import csv, hashlib, io, json, os, re, subprocess, sys, collections

# ── 1. LAS VERSIONES, leidas del fuente ───────────────────────────────────────────────────────
#
# nombre · fichero · patron · que significa
# El patron TIENE que anclar en algo que solo salga una vez. Si sale cero o mas de una, se para:
# un ancla ambigua es como leer `isse_lr` = 3 porque coincidio con la declaracion del campo en vez
# de con el valor -paso de verdad esta mañana-.
ANCLAS = [
    ("modelo de vc-cm", "crates/vc-cm/src/lib.rs", r"^pub const MODELO: u8 = (\d+);",
     "la version con la que se ESCRIBE por defecto. Sube cada vez que cambia algo que altere UNA "
     "SOLA prediccion, y viaja dentro del flujo: un archivo se abre con el modelo de su epoca."),
    # ⚠️ ESTE ANCLA SE ROMPIO EL 17-08 Y HAY QUE SABER POR QUE, porque el arreglo no es cambiar la
    # expresion regular hasta que enganche. `MODELOS_LEIBLES` era una lista LITERAL —y por eso se
    # podia leer entera de aqui— hasta que se DERIVO de una sola constante para que no volviera a
    # desincronizarse del `match` de `descomprimir`. Ya no hay lista que leer: la fuente de verdad
    # es `MODELO_MAX`, y la lista es 1..=MODELO_MAX. Se ancla ahi y se expande abajo, que es leer
    # del fuente igual que antes; escribir la lista a mano aqui seria justo lo que este fichero
    # existe para no hacer.
    ("modelos que sabemos abrir", "crates/vc-cm/src/lib.rs",
     r"^pub const MODELO_MAX: u8 = (\d+);",
     "la lista se DERIVA de MODELO_MAX (1..=MODELO_MAX) para que no pueda desincronizarse del "
     "reparto de descomprimir. Quitar uno es romper archivos: solo tras ventana de obsolescencia."),
    ("frame por defecto (MiB)", "crates/vc-cli/src/main.rs", r"^const SOLIDO_DEF: usize = (\d+);",
     "64 y no 128 porque esta medido: 128 y 256 MiB dan MAS bytes y hasta 1,48x el RSS."),
    ("version del paquete", "Cargo.toml", r'^version = "([^"]+)"',
     "la del workspace; todos los crates la heredan."),
]

def lee_ancla(fichero, patron):
    try: s = io.open(fichero, encoding="utf-8").read()
    except OSError: return None, "no existe el fichero"
    m = re.findall(patron, s, re.M)
    if len(m) == 0: return None, "el ancla no aparece"
    if len(m) > 1: return None, f"el ancla sale {len(m)} veces"
    return m[0].strip(), ""

VER, fallos = [], []
for nombre, fichero, patron, porque in ANCLAS:
    v, err = lee_ancla(fichero, patron)
    if v is None:
        fallos.append(f"{nombre} ({fichero}): {err}")
        continue
    # La unica expansion: MODELO_MAX es un numero en el fuente y la fila es la LISTA que ese
    # numero significa. No es un dato a mano —sale entero del ancla— y se rompe igual que las
    # demas si el ancla desaparece.
    if nombre == "modelos que sabemos abrir":
        v = ", ".join(str(i) for i in range(1, int(v) + 1))
    VER.append((nombre, v, fichero, porque))

if fallos:
    print("ABORTA: no se pudo leer del fuente:", file=sys.stderr)
    for f in fallos: print("   ·", f, file=sys.stderr)
    print("   Antes que enseñar una version inventada, esto se para.", file=sys.stderr)
    sys.exit(2)

# ── 2. LOS COMMITS, de git ────────────────────────────────────────────────────────────────────
def git(*a):
    return subprocess.run(["git", *a], capture_output=True, text=True).stdout

DESDE = os.environ.get("APLICADO_DESDE", "40")   # cuantos commits hacia atras
crudo = git("log", f"-{DESDE}", "--format=%h\x01%cI\x01%s")
COMMITS = [l.split("\x01") for l in crudo.splitlines() if l.count("\x01") == 2]

# ── 3. LA COBERTURA: cuantas medidas tiene cada commit en el banco ────────────────────────────
#
# 🚨 Y AQUI ESTABA EL AGUJERO QUE DEJABA LA LINEA DE TIEMPO PLANA. Contar por `commit` cuenta
# «que habia en HEAD cuando se midio», y el metodo de esta casa es MEDIR PRIMERO Y COMMITEAR
# DESPUES con la cifra en el mensaje. O sea que la fila cae SIEMPRE bajo el commit anterior y el
# que contiene el cambio sale a cero. El 02-08: diez commits de una tarde, todos a cero, y las
# cuatro filas buenas archivadas bajo el commit de la vispera.
#
# Ahora se casa por `arbol_sha` —el digesto de los blobs de crates/ + Cargo.* que mide `mide.sh`
# sobre EL DISCO— contra el mismo digesto calculado para cada commit desde `git ls-tree`. Dos
# digestos iguales son EL MISMO CODIGO, lo hubiera commiteado alguien o no.
#
# El commit sigue contando como respaldo para las filas viejas, que no llevan `arbol_sha`.
def arbol_sha_de(commit):
    """El mismo digesto que mide.sh, pero sacado del commit sin leer un fichero."""
    salida = git("ls-tree", "-r", commit, "--", "crates", "Cargo.toml", "Cargo.lock")
    lineas = []
    for l in salida.splitlines():
        if "\t" not in l: continue
        meta, ruta = l.split("\t", 1)
        partes = meta.split()
        if len(partes) < 3: continue
        lineas.append(f"{ruta} {partes[2]}")
    lineas.sort()
    if not lineas: return ""
    crudo = "".join(x + "\n" for x in lineas).encode("utf-8")
    return hashlib.sha256(crudo).hexdigest()[:12]

med = collections.Counter()          # por commit (respaldo, filas viejas)
med_arbol = collections.Counter()    # por arbol_sha (lo bueno)
etiquetas = collections.defaultdict(set)
etiq_arbol = collections.defaultdict(set)
try:
    for l in io.open("bench/historia.jsonl", encoding="utf-8"):
        l = l.strip()
        if not l: continue
        try: d = json.loads(l)
        except ValueError: continue
        a = str(d.get("arbol_sha", "") or "")
        if a:
            med_arbol[a] += 1
            if d.get("etiqueta"): etiq_arbol[a].add(d["etiqueta"])
            continue
        c = str(d.get("commit", ""))[:7]
        if c:
            med[c] += 1
            if d.get("etiqueta"): etiquetas[c].add(d["etiqueta"])
except OSError:
    pass

# ── 3b. LAS PRUEBAS DE DISEÑO, que tampoco salian ─────────────────────────────────────────────
#
# `bench/pruebas.sh --diseno` contesta las preguntas mas caras del proyecto —la sonda de familias,
# los DUPLICADOS, las perillas del mezclador— y NINGUNA pasa por `mide.sh`: escriben su respuesta
# en bench/registros/pruebas/<nombre>.log y ahi se queda. El panel no miraba esa carpeta, asi que
# trece de las diecisiete medidas del 02-08 no existian para la linea de tiempo.
#
# No se las mete por la bascula a la fuerza: una prueba de diseño corre decenas de compresiones y
# meterla en el libro mayor generaria decenas de filas cuyos segundos no significan nada. Lo que
# se cuenta es lo que de verdad tiene: QUE pregunta se contesto, CUANDO y con QUE codigo — y el
# commit lo escribe su propia cabecera (`cab()` en comun.sh).
pruebas = collections.defaultdict(set)
_pd = os.path.join("bench", "registros", "pruebas")
if os.path.isdir(_pd):
    for _f in sorted(os.listdir(_pd)):
        if not _f.endswith(".log") or _f.startswith("_"): continue
        try: _txt = io.open(os.path.join(_pd, _f), encoding="utf-8", errors="replace").read(4000)
        except OSError: continue
        _m = re.search(r"^\s*commit\s+([0-9a-f]{7,})", _txt, re.M)
        if _m: pruebas[_m.group(1)[:7]].add(_f[:-4])

FILAS = []
for sha, cuando, asunto in COMMITS:
    a = arbol_sha_de(sha)
    n = med_arbol.get(a, 0) + med.get(sha[:7], 0)
    etq = set(etiq_arbol.get(a, ())) | set(etiquetas.get(sha[:7], ()))
    pr = sorted(pruebas.get(sha[:7], ()))
    de_que = " · ".join(sorted(etq))
    if pr: de_que = (de_que + " · " if de_que else "") + "pruebas: " + " ".join(pr)
    FILAS.append([sha, cuando[:16].replace("T", " "), asunto, str(n + len(pr)), de_que[:160]])

sin = sum(1 for f in FILAS if f[3] == "0")

# ── 4. a disco ────────────────────────────────────────────────────────────────────────────────
with open("bench/aplicado-version.tsv", "w", newline="") as f:
    f.write("# EN QUE VERSION ESTAMOS. Cada valor LEIDO DEL FUENTE por bench/aplicado.py.\n")
    f.write("# Ni uno escrito a mano: si un ancla no aparece, el generador ABORTA.\n")
    w = csv.writer(f, delimiter="\t")
    w.writerow(["que", "valor", "de_donde", "por_que_importa"])
    w.writerows(VER)

with open("bench/aplicado.tsv", "w", newline="") as f:
    f.write("# QUE SE HA APLICADO, y si el banco lo vio pasar.\n")
    f.write("# La columna «medidas» casa cada fila de bench/historia.jsonl con el commit cuyo CODIGO\n")
    f.write("# midio -por `arbol_sha`, el digesto de crates/ + Cargo.*- y no con el que habia en HEAD\n")
    f.write("# al medir. No es lo mismo: aqui se mide primero y se commitea despues con la cifra en\n")
    f.write("# el mensaje, asi que contando por HEAD la fila caia bajo el commit ANTERIOR y el que\n")
    f.write("# contiene el cambio salia a cero. Y ademas se cuentan las PRUEBAS DE DISEÑO, que no\n")
    f.write("# pasan por la bascula y viven en bench/registros/pruebas/<nombre>.log.\n")
    f.write("# Un 0 sigue queriendo decir algo: que ese commit no dejo ni medida ni prueba.\n")
    w = csv.writer(f, delimiter="\t")
    w.writerow(["commit", "cuando", "que_hizo", "medidas", "de_que"])
    w.writerows(FILAS)

print(f"\n  EN QUE VERSION ESTAMOS\n")
for n, v, d, _ in VER:
    print(f"    {n:<28} {v:<12}  ({d})")
print(f"\n  QUE SE HA APLICADO · ultimos {len(FILAS)} commits\n")
print(f"    {'commit':<9} {'cuando':<17} {'med':>4}  que hizo")
print("    " + "-" * 92)
for sha, cuando, asunto, n, _ in FILAS[:14]:
    marca = "  " if n != "0" else " ⚠"
    print(f"    {sha:<9} {cuando:<17} {n:>4}{marca} {asunto[:64]}")
print()
print(f"  {len(FILAS)} commits · {len(FILAS)-sin} con medidas en el banco · {sin} SIN")
if sin:
    print(f"\n  ⚠️ {sin} commits no aparecen en la linea de tiempo, y no porque no hicieran nada.")
    print( "     Las tres causas, por orden de frecuencia y ya sin adivinar cual es:")
    print( "       1. la fila es ANTERIOR al sello `arbol_sha` (antes del 02-08) y se midio con el")
    print( "          arbol sucio, asi que solo se sabe que NO era el codigo de su commit.")
    print( "       2. se midio en el Mac, que no tiene bascula que escriba en historia.jsonl.")
    print( "       3. el commit es de documentacion o de banco y no habia nada que medir.")
    print( "     De la 1 no queda ninguna nueva: `mide.sh` sella el codigo desde el 02-08.")
print()
print("  bench/aplicado-version.tsv · bench/aplicado.tsv")
