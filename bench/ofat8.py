#!/usr/bin/env python3
# EL BARRIDO POR COORDENADAS A 8 MiB — EL QUE DECIDE.
#
# POR QUE POR COORDENADAS Y NO LA REJILLA. Porque la rejilla completa ya se corrio (4.620 puntos,
# `bench/rejilla.tsv`) y su resultado fue: **en 4 de 5 muestras OFAT encuentra EXACTAMENTE el
# mismo punto**, y en la quinta se deja 0,006%. O sea que el metodo barato esta LICENCIADO por
# medida, no por fe. Gastar la rejilla cara otra vez aqui seria pagar dos veces por la misma
# respuesta — y a 8 MiB no cabe: el volcado son 23,5 GB y el replay de 67M bits x 84 configs x 5
# muestras se iria a mas de un dia.
#
# POR QUE A 8 MiB Y NO A 512 KiB. Porque la tabla de 512 KiB NO SE PUEDE APLICAR. `VC_ISSE_LR`
# domina los cinco optimos y es justo la perilla de la que esta escrito que depende del tamaño:
# ganaba a 512 KB y perdia en cinco de seis a 8 MiB. Cuatro veces en este proyecto el tamaño de
# la muestra ha decidido la respuesta (ISSE_LR, cuantos, familia, sse_w0). A 512 KiB se explora;
# a 8 MiB se decide.
#
# ⚠️ VC_SSE VA DE 1 A 5, no de 3 a 5. `mezclador.sh` empieza en 3, que es EL BORDE, y por eso no
# podia ver que en codigo el optimo es 2 (17.962 contra 17.967). Un barrido que empieza en el
# borde solo puede decir «mas abajo es mejor».
#
# ⚠️ TIPO BYTES: deterministas, paralelizables, y el Mac vale (mismo sha256 que el servidor).
# El reloj de aqui NO vale para nada.
import argparse, os, re, subprocess, sys, tempfile, time
from concurrent.futures import ThreadPoolExecutor, as_completed

RAIZ = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SALIDA = os.path.join(RAIZ, "bench/ofat8.tsv")
RE_OUT = re.compile(rb"out=(\d+)")

# perilla | defecto | valores | que borde es TOPE REAL (no se avisa si gana ahi)
PERILLAS = [
    ("VC_MIX_LR",  12, [8, 10, 11, 12, 13, 14, 16],                  {"alto"}),
    ("VC_DECAY",    0, [0, 14, 16, 18],                              {"bajo"}),
    ("VC_ISSE_LR",  6, [3, 4, 6, 8, 12, 16, 20, 24, 32, 48, 64],     set()),
    ("VC_SSE",      5, [1, 2, 3, 4, 5],                              {"bajo", "alto"}),
]


def mide(vc, ruta, perilla, valor):
    env = dict(os.environ)
    if perilla:
        env[perilla] = str(valor)
    with tempfile.NamedTemporaryFile(suffix=".vc", delete=False) as f:
        out = f.name
    try:
        r = subprocess.run([vc, "c", ruta, out], env=env, capture_output=True)
        if r.returncode != 0:
            return None
        m = RE_OUT.search(r.stderr)
        return int(m.group(1)) if m else None
    finally:
        try: os.unlink(out)
        except OSError: pass


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--muestras", default="codigo,sql,logs,csvjson,enwik8")
    ap.add_argument("--tam", type=int, default=8388608)
    ap.add_argument("--jobs", type=int, default=4)
    ap.add_argument("--bin", default=os.path.join(RAIZ, "target/release"))
    ap.add_argument("--muestras-dir", default=os.environ.get(
        "VC_MUESTRAS", "muestras-vc"))
    a = ap.parse_args()

    vc = os.path.join(a.bin, "vc-cm")
    if not os.path.exists(vc):
        sys.exit(f"falta {vc}")

    tmpd = tempfile.mkdtemp(prefix="ofat8.")
    trabajo = []
    for m in a.muestras.split(","):
        src = next((os.path.join(a.muestras_dir, m + s) for s in (".10m", ".8m", "")
                    if os.path.exists(os.path.join(a.muestras_dir, m + s))), None)
        if not src:
            print(f"  falta {m}", flush=True); continue
        ruta = os.path.join(tmpd, f"{m}.bin")
        with open(src, "rb") as f1, open(ruta, "wb") as f2:
            f2.write(f1.read(a.tam))
        trabajo.append((m, ruta, None, 0))          # la linea base
        for p, d, vals, _ in PERILLAS:
            for v in vals:
                if v != d:                          # el defecto YA es la linea base
                    trabajo.append((m, ruta, p, v))

    print(f"OFAT a {a.tam//1024//1024} MiB · {len(trabajo)} compresiones · {a.jobs} en paralelo",
          flush=True)
    print("⚠️ BYTES. El reloj de aqui no vale.", flush=True)
    res, t0, hechos = {}, time.time(), 0
    with ThreadPoolExecutor(max_workers=a.jobs) as ex:
        futs = {ex.submit(mide, vc, r, p, v): (m, p, v) for m, r, p, v in trabajo}
        for fu in as_completed(futs):
            m, p, v = futs[fu]
            b = fu.result()
            hechos += 1
            if b is None:
                print(f"  ⚠️ FALLO {m} {p}={v}", flush=True); continue
            res[(m, p, v)] = b
            if hechos % 20 == 0:
                el = time.time() - t0
                print(f"  {hechos}/{len(trabajo)} · {el/60:.1f} min · "
                      f"faltan ~{el/hechos*(len(trabajo)-hechos)/60:.1f} min", flush=True)

    with open(SALIDA, "w") as f:
        f.write(f"# OFAT a {a.tam} B · el barrido que DECIDE (la rejilla de 512 KiB lo licencio)\n")
        f.write("muestra\ttam\tperilla\tvalor\tbytes\n")
        for (m, p, v), b in sorted(res.items(), key=lambda x: (x[0][0], x[0][1] or "", x[0][2])):
            f.write(f"{m}\t{a.tam}\t{p or 'BASE'}\t{v}\t{b}\n")

    print(f"\n{'='*86}\nRESULTADO · {a.tam//1024//1024} MiB\n{'='*86}")
    total_def = total_mej = 0
    avisos = []
    for m in a.muestras.split(","):
        base = res.get((m, None, 0))
        if base is None: continue
        total_def += base
        print(f"\n  {m} · con los defectos: {base} B")
        mejor_m = base
        for p, d, vals, topes in PERILLAS:
            linea, mejor, mejorb = [], d, base
            for v in vals:
                b = base if v == d else res.get((m, p, v))
                if b is None: continue
                linea.append(f"{v}{'*' if v == d else ''}:{100*(b-base)/base:+.3f}")
                if b < mejorb: mejor, mejorb = v, b
            print(f"    {p:<12} {' '.join(linea)}")
            if mejor != d:
                print(f"    {'':<12} mejor={mejor} ({100*(mejorb-base)/base:+.3f}%)")
                mejor_m = min(mejor_m, mejorb)
            if mejor == min(vals) and "bajo" not in topes:
                avisos.append(f"{m}/{p}={mejor} gana EN EL BORDE BAJO")
            if mejor == max(vals) and "alto" not in topes:
                avisos.append(f"{m}/{p}={mejor} gana EN EL BORDE ALTO")
        total_mej += mejor_m
    if total_def:
        print(f"\n  TOTAL con los defectos     : {total_def} B")
        print(f"  TOTAL con la mejor perilla : {total_mej} B  {100*(total_mej-total_def)/total_def:+.3f}%")
        print("  (⚠️ es la mejor perilla SUELTA por muestra, no una config unica aplicable)")
    print("\n  BORDES:")
    for x in avisos or ["  ninguno"]:
        print(f"    ⚠️ {x}" if avisos else f"  {x}")
    print(f"\n  {len(res)} medidas en {(time.time()-t0)/60:.1f} min -> {SALIDA}")


if __name__ == "__main__":
    main()
