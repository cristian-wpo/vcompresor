#!/usr/bin/env python3
# LA REJILLA COMPLETA, Y POR QUE AHORA SI CABE.
#
# `bench/mezclador.sh` barre POR COORDENADAS y lo dice por escrito: «3^5 = 243 compresiones no
# caben en una suite, y ademas la rejilla completa contesta una pregunta que no tenemos
# (¿interactuan?)». Las dos mitades de esa frase han dejado de ser verdad:
#
#   1. SI CABE. El barrido por coordenadas se escribio para correr EN EL SERVIDOR y en serie,
#      porque alli el reloj es lo unico que vale y el reloj no se paraleliza. Pero esto NO MIDE
#      RELOJ: mide BYTES, y los bytes son DETERMINISTAS y bit-identicos entre x86 y ARM (control
#      del 03-08: el mismo trozo de 512 KiB da 18.066 bytes y el mismo sha256 en las dos
#      maquinas). O sea que la rejilla entera se puede repartir entre los 10 nucleos del Mac sin
#      tocar el servidor y sin falsear nada.
#
#   2. LA PREGUNTA SI LA TENEMOS, y es la mas cara que hay abierta. El barrido por coordenadas
#      solo puede decir «cada perilla, ¿esta donde toca ESTANDO LAS DEMAS EN SU SITIO?». No
#      puede ver que dos perillas se compensen entre si, que es exactamente como se pierde un
#      optimo. La rejilla completa da las dos cosas de una vez: el mejor punto de verdad, y
#      —restandole el mejor punto POR COORDENADAS— cuanto vale lo que OFAT no puede ver.
#
# LO QUE ESTE GUION NO ES. No sustituye a `mezclador.sh`: aquel mide en dos tallas y avisa del
# ganador en el borde, y esas dos salvaguardas siguen siendo suyas. Este contesta otra pregunta.
#
# ⚠️ TIPO BYTES. No mires el reloj de aqui: hay 8 procesos peleandose por la CPU y ninguno de sus
# segundos es el de nadie. Para reloj, servidor y en serie.
#
#   bench/rejilla.py --muestras codigo,sql --tam 524288 --jobs 8
#   bench/rejilla.py --analiza                 (solo lee el TSV y contesta)
import argparse, itertools, os, re, subprocess, sys, tempfile, time
from concurrent.futures import ThreadPoolExecutor

RAIZ = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
VC = os.path.join(RAIZ, "target/release/vc-cm")
SALIDA = os.path.join(RAIZ, "bench/rejilla.tsv")

# perilla -> (defecto, valores). Los MISMOS que barre mezclador.sh, para que las dos medidas
# sean comparables fila a fila y una pueda auditar a la otra.
#
# ⚠️ VC_DISP NO ESTA, Y NO ES UN OLVIDO: ES UNA PERILLA MUERTA. Medido el 03-08 en el Mac sobre
# 512 KiB de codigo:
#
#     v6            18.084      v7            18.066
#     v6 VC_DISP=0  18.066      v7 VC_DISP=0  18.066
#
# O sea que `v7 == v6 + VC_DISP=0`, byte a byte: desde la v7 las ranuras IDISP/IDISP2 entran en
# la mascara de apagados (lib.rs:493), asi que la dispersion se calcula y el bucle de ablacion la
# borra a cero acto seguido. `VC_DISP` no puede mover un solo byte en el modelo de hoy.
#
# Y `mezclador.sh` la barre igual —`VC_DISP|1|0 1`— gastando dos compresiones por muestra y por
# talla que no pueden informar de nada. Su aviso de «empatan en bytes» NO salta, porque solo
# avisa cuando el defecto declarado no esta entre los empatados, y aqui esta. Una perilla inerte
# barrida en silencio es peor que no barrerla: ocupa el sitio de una viva.
#
# (Que la dispersion se CALCULE para borrarla es ademas reloj tirado en cada bit. Eso es una
# medida de TIEMPO: servidor y en serie, no aqui.)
PERILLAS = {
    "VC_MIX_LR":   (12, [8, 10, 11, 12, 13, 14, 16]),
    "VC_DECAY":    (0,  [0, 14, 16, 18]),
    "VC_ISSE_LR":  (6,  [3, 4, 6, 8, 12, 16, 20, 24, 32, 48, 64]),
    "VC_SSE":      (5,  [3, 4, 5]),
}
NOMBRES = list(PERILLAS)
RE_OUT = re.compile(rb"out=(\d+)")


def comprime(ruta, cfg):
    """Devuelve los BYTES que salen. cfg es un dict perilla->valor."""
    env = dict(os.environ)
    for k, v in cfg.items():
        env[k] = str(v)
    with tempfile.NamedTemporaryFile(suffix=".vc", delete=False) as f:
        out = f.name
    try:
        r = subprocess.run([VC, "c", ruta, out], env=env, capture_output=True)
        if r.returncode != 0:
            return None
        m = RE_OUT.search(r.stderr)
        return int(m.group(1)) if m else None
    finally:
        try:
            os.unlink(out)
        except OSError:
            pass


def prepara_muestra(nombre, tam, dst_dir):
    """Corta los primeros `tam` bytes de la muestra. Mismo corte que el control cruzado."""
    for cand in (os.path.join(MUESTRAS, nombre + ".10m"),
                 os.path.join(MUESTRAS, nombre + ".8m"),
                 os.path.join(MUESTRAS, nombre)):
        if os.path.exists(cand):
            dst = os.path.join(dst_dir, f"{nombre}.{tam}.bin")
            if not os.path.exists(dst) or os.path.getsize(dst) != tam:
                with open(cand, "rb") as a, open(dst, "wb") as b:
                    b.write(a.read(tam))
            return dst
    return None


def sha_bin():
    import hashlib
    h = hashlib.sha256()
    with open(VC, "rb") as f:
        for trozo in iter(lambda: f.read(1 << 20), b""):
            h.update(trozo)
    return h.hexdigest()


def carga_hechas():
    hechas = {}
    if not os.path.exists(SALIDA):
        return hechas
    with open(SALIDA) as f:
        for ln in f:
            if ln.startswith("#") or ln.startswith("muestra\t"):
                continue
            c = ln.rstrip("\n").split("\t")
            if len(c) < 3 + len(NOMBRES):
                continue
            clave = (c[0], int(c[1]), tuple(int(x) for x in c[2:2 + len(NOMBRES)]))
            hechas[clave] = int(c[2 + len(NOMBRES)])
    return hechas


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--muestras", default="codigo,sql")
    ap.add_argument("--tam", type=int, default=524288)
    ap.add_argument("--jobs", type=int, default=8)
    ap.add_argument("--muestras-dir", default=os.environ.get(
        "VC_MUESTRAS", "muestras-vc"))
    ap.add_argument("--analiza", action="store_true")
    a = ap.parse_args()

    global MUESTRAS
    MUESTRAS = a.muestras_dir

    if a.analiza:
        import analiza_rejilla
        return analiza_rejilla.main()

    if not os.path.exists(VC):
        sys.exit(f"falta {VC} — cargo build --release")

    combos = list(itertools.product(*[PERILLAS[n][1] for n in NOMBRES]))
    tipos = [t for t in a.muestras.split(",") if t]
    tmpd = tempfile.mkdtemp(prefix="rejilla.")
    hechas = carga_hechas()

    trabajo = []
    for t in tipos:
        ruta = prepara_muestra(t, a.tam, tmpd)
        if not ruta:
            print(f"  falta la muestra {t} en {MUESTRAS} — se salta")
            continue
        for c in combos:
            if (t, a.tam, c) in hechas:
                continue
            trabajo.append((t, ruta, c))

    total = len(trabajo)
    sha0 = sha_bin()
    print(f"REJILLA COMPLETA · {len(combos)} combinaciones x {len(tipos)} muestras de {a.tam} B",
          flush=True)
    print(f"  ya medidas: {len(hechas)} · quedan: {total} · {a.jobs} en paralelo", flush=True)
    print(f"  binario: {sha0[:16]}", flush=True)
    print(f"  ⚠️ BYTES, no reloj. Los segundos de aqui no son de nadie.", flush=True)
    if total == 0:
        print("  nada que hacer.")
        return

    nuevo = not os.path.exists(SALIDA)
    f = open(SALIDA, "a", buffering=1)
    if nuevo:
        f.write("# rejilla completa de las perillas del mezclador · BYTES (deterministas)\n")
        f.write("muestra\ttam\t" + "\t".join(NOMBRES) + "\tbytes\n")

    t0 = time.time()
    hechos = 0

    def tarea(item):
        t, ruta, c = item
        b = comprime(ruta, dict(zip(NOMBRES, c)))
        return (t, c, b)

    # ⚠️ `as_completed`, NO `map`. Con `map` los resultados salen EN ORDEN: si la primera
    # compresion se atasca, no se escribe NI UNA fila aunque hayan terminado doscientas, y desde
    # fuera una tanda sana es indistinguible de una colgada. Pasó en la primera ejecucion.
    from concurrent.futures import as_completed
    with ThreadPoolExecutor(max_workers=a.jobs) as ex:
        futs = {ex.submit(tarea, it): it for it in trabajo}
        for fu in as_completed(futs):
            t, c, b = fu.result()
            hechos += 1
            if b is None:
                print(f"  ⚠️ FALLO en {t} {dict(zip(NOMBRES, c))}", flush=True)
                continue
            f.write(f"{t}\t{a.tam}\t" + "\t".join(str(x) for x in c) + f"\t{b}\n")
            if hechos % 50 == 0 or hechos == 1:
                el = time.time() - t0
                eta = el / hechos * (total - hechos)
                print(f"  {hechos}/{total} · {el/60:.1f} min · faltan ~{eta/60:.1f} min",
                      flush=True)
    f.close()
    # ATRIBUCION: una tanda, un binario. Si alguien recompila a mitad —y en esta maquina hay mas
    # de una sesion trabajando— las filas de antes y las de despues no son del mismo programa y
    # la tabla no vale. Se comprueba al final y se dice; no se corrige solo.
    if sha_bin() != sha0:
        print("  🚨 EL BINARIO CAMBIO A MITAD DE LA TANDA: estas filas NO son atribuibles a un "
              "solo programa. Borra bench/rejilla.tsv y repite.", flush=True)
    print(f"HECHO en {(time.time()-t0)/60:.1f} min -> {SALIDA}", flush=True)


if __name__ == "__main__":
    main()
