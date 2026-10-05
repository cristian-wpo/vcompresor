#!/usr/bin/env python3
# LA REJILLA COMPLETA CON 55 COMPRESIONES EN VEZ DE 4.620.
#
# La rejilla del mezclador son cuatro perillas vivas: VC_MIX_LR (7) x VC_DECAY (4) x VC_ISSE_LR
# (11) x VC_SSE (3) = 924 puntos por muestra. Medirlos a lo bruto es 924 compresiones, y a 3,65 s
# cada una (512 KiB en el Mac) son 56 minutos POR MUESTRA. Con cinco, casi cinco horas.
#
# EL REPARTO QUE LO ARREGLA, y sale de mirar DONDE vive cada perilla:
#
#   VC_ISSE_LR mueve `st[IISSE]` — o sea, cambia lo que los modelos predicen. No hay atajo:
#   cada valor necesita su compresion de verdad. Son 11.
#
#   VC_MIX_LR, VC_DECAY y VC_SSE viven AGUAS ABAJO de `st[]`. Ninguna puede mover un solo `st`,
#   porque ningun submodelo ve la prediccion final. Las 84 combinaciones de las tres se sacan
#   del MISMO volcado, sin volver a correr un modelo.
#
# O sea: 11 compresiones (volcando) + 84 replays por volcado = las 924. Y las 11 compresiones
# sirven ademas como las 11 filas de la columna VC_ISSE_LR.
#
# ⚠️ NO ES UNA ESTIMACION. El replay ejecuta `mezcla()`/`actualiza_mezcla()`, los mismos metodos
# que usa `comprimir`, y da los mismos bytes al byte. Lo comprueba `bench/replay-control.sh`, y
# ademas este guion cruza sus filas contra las que se midieron a lo bruto (ver --control).
#
# ⚠️ TIPO BYTES. Deterministas: se puede correr con la maquina llena. El reloj de aqui no vale.
import argparse, os, re, shutil, subprocess, sys, tempfile, time

RAIZ = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SALIDA = os.path.join(RAIZ, "bench/rejilla.tsv")
NOMBRES = ["VC_MIX_LR", "VC_DECAY", "VC_ISSE_LR", "VC_SSE"]
ISSE_LR = [3, 4, 6, 8, 12, 16, 20, 24, 32, 48, 64]
RE_OUT = re.compile(rb"out=(\d+)")
CAB = 10   # el .vc lleva 8 B de longitud + 1 de modelo + 1 de familia/perfil


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--muestras", default="codigo,sql,logs,csvjson,enwik8")
    ap.add_argument("--tam", type=int, default=131072)
    ap.add_argument("--jobs", type=int, default=4)
    ap.add_argument("--bin", default=os.path.join(RAIZ, "target/release"))
    ap.add_argument("--muestras-dir", default=os.environ.get(
        "VC_MUESTRAS", "muestras-vc"))
    ap.add_argument("--control", metavar="TSV",
                    help="cruza contra un TSV medido a lo bruto y se calla si todo cuadra")
    a = ap.parse_args()

    vc = os.path.join(a.bin, "vc-cm")
    vr = os.path.join(a.bin, "vc-replay")
    for x in (vc, vr):
        if not os.path.exists(x):
            sys.exit(f"falta {x} — cargo build --release")

    tmpd = tempfile.mkdtemp(prefix="pipe.")
    filas = []
    t0 = time.time()
    compresiones = 0
    try:
        for m in a.muestras.split(","):
            src = None
            for suf in (".10m", ".8m", ""):
                c = os.path.join(a.muestras_dir, m + suf)
                if os.path.exists(c):
                    src = c
                    break
            if not src:
                print(f"  falta la muestra {m} — se salta", flush=True)
                continue
            trozo = os.path.join(tmpd, f"{m}.bin")
            with open(src, "rb") as f1, open(trozo, "wb") as f2:
                f2.write(f1.read(a.tam))

            for lr in ISSE_LR:
                volc = os.path.join(tmpd, "v.st")
                env = dict(os.environ, VC_VOLCAR=volc, VC_ISSE_LR=str(lr))
                r = subprocess.run([vc, "c", trozo, os.path.join(tmpd, "o.vc")],
                                   env=env, capture_output=True)
                compresiones += 1
                if r.returncode != 0:
                    print(f"  ⚠️ {m} ISSE_LR={lr}: la compresion fallo", flush=True)
                    continue
                real = int(RE_OUT.search(r.stderr).group(1)) - CAB

                # las 84 de la cola, del mismo volcado
                rr = subprocess.run([vr, volc, "--barrido", "--jobs", str(a.jobs)],
                                    capture_output=True)
                if rr.returncode != 0:
                    print(f"  ⚠️ replay fallo: {rr.stderr.decode()[:200]}", flush=True)
                    continue
                n, cuadra = 0, False
                for ln in rr.stdout.decode().splitlines():
                    if not ln or ln[0] not in "0123456789":
                        continue
                    mx, dc, ss, by = ln.split("\t")
                    # ⚠️ +CAB, Y ESTO LO CAZO EL CRUCE. El replay devuelve los bytes del
                    # CODIFICADOR; `rejilla.py` y todo el banco anotan `out=`, que es el ARCHIVO
                    # —codificador mas 10 B de cabecera—. Sin sumarlos, las dos tablas hablan de
                    # cosas distintas y ninguna lo dice. El cruce saco 191 puntos discrepando
                    # todos en EXACTAMENTE 10, cero variabilidad: eso no es un replay que falla,
                    # es un replay exacto medido en otra unidad. Un control que solo mirara «¿son
                    # iguales?» habria tirado el metodo entero por un desfase de cabecera.
                    filas.append((m, a.tam, int(mx), int(dc), lr, int(ss), int(by) + CAB))
                    n += 1
                    # ⚠️ EL CONTROL QUE VIAJA CON CADA VOLCADO: la fila del defecto de la cola
                    # (12/0/5) tiene que dar EXACTAMENTE los bytes de la compresion que genero
                    # este volcado. Si no, el replay esta desplazado y TODAS sus filas mienten.
                    if (int(mx), int(dc), int(ss)) == (12, 0, 5) and int(by) == real:
                        cuadra = True
                os.unlink(volc)
                if not cuadra:
                    print(f"  🚨 {m} ISSE_LR={lr}: el replay NO reproduce los {real} B de su "
                          f"propia compresion. Estas filas NO valen.", flush=True)
                print(f"  {m} ISSE_LR={lr:>2}: real {real} B · {n} configs de la cola "
                      f"{'✅' if cuadra else '🚨'} · {(time.time()-t0)/60:.1f} min", flush=True)
    finally:
        shutil.rmtree(tmpd, ignore_errors=True)

    if not filas:
        sys.exit("no salio ni una fila")

    # ACUMULA, no sobrescribe. La primera version abria en "w" y una segunda tanda con otras
    # muestras borraba la anterior — justo lo que hace falta para los TRES NIVELES, que solo
    # significan algo con todas las muestras juntas. Se reescribe entero (leido + nuevo) para que
    # una fila re-medida gane a la vieja en vez de duplicarse.
    previas = {}
    if os.path.exists(SALIDA):
        for ln in open(SALIDA):
            if ln.startswith("#") or ln.startswith("muestra\t"):
                continue
            c = ln.rstrip("\n").split("\t")
            if len(c) == 7:
                previas[tuple(c[:6])] = c[6]
    for r in filas:
        previas[tuple(str(x) for x in r[:6])] = str(r[6])
    with open(SALIDA, "w") as f:
        f.write("# rejilla completa por PIPELINE: 1 compresion por VC_ISSE_LR + replay de la cola\n")
        f.write("muestra\ttam\t" + "\t".join(NOMBRES) + "\tbytes\n")
        for k in sorted(previas, key=lambda x: (x[0], int(x[1]), *(int(v) for v in x[2:]))):
            f.write("\t".join(k) + "\t" + previas[k] + "\n")

    seg = time.time() - t0
    print(f"\n{len(filas)} puntos con {compresiones} compresiones en {seg/60:.1f} min -> {SALIDA}")
    print(f"a lo bruto habrian sido {len(filas)} compresiones: {len(filas)/max(compresiones,1):.0f}x "
          f"menos trabajo de modelo")

    # ── cruce contra las filas medidas a lo bruto ──────────────────────────────────────────────
    # Es la validacion que de verdad importa: no que el replay cuadre consigo mismo, sino que
    # coincida con compresiones de verdad hechas por otro camino y en otro momento.
    if a.control and os.path.exists(a.control):
        mio = {(r[0], r[1], r[2], r[3], r[4], r[5]): r[6] for r in filas}
        n, mal = 0, 0
        with open(a.control) as f:
            for ln in f:
                if ln.startswith("#") or ln.startswith("muestra\t"):
                    continue
                c = ln.rstrip("\n").split("\t")
                if len(c) != 7:
                    continue
                k = (c[0], int(c[1]), int(c[2]), int(c[3]), int(c[4]), int(c[5]))
                if k not in mio:
                    continue
                n += 1
                if mio[k] != int(c[6]):
                    mal += 1
                    if mal <= 5:
                        print(f"  🚨 DISCREPA {k}: bruto {c[6]} · pipeline {mio[k]}")
        if n == 0:
            print("\n  (el control no comparte ni un punto con esta tanda — nada que cruzar)")
        elif mal == 0:
            print(f"\n  ✅ CRUCE CONTRA MEDIDA BRUTA: {n} puntos, {n} coinciden AL BYTE.")
            print("     El pipeline no es una estimacion del metodo bruto: es el mismo numero.")
        else:
            print(f"\n  🚨 CRUCE: {mal} de {n} puntos NO coinciden. El pipeline no vale.")
            sys.exit(1)


if __name__ == "__main__":
    main()
