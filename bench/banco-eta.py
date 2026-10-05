#!/usr/bin/env python3
"""EL BANCO DEL TIEMPO QUE FALTA: N estimadores sobre UNA traza, no una corrida por idea.

POR QUE ASI. Cada corrida del arbol mixto son dos minutos y hay que hacerla en serie con la
maquina para ella sola. Probar cinco estimadores serian diez minutos de reloj y cinco trazas
distintas -o sea cinco corridas que no son comparables entre si, porque la maquina no repite-.
Capturando la traza UNA vez y pasandola por todos, todos ven EXACTAMENTE los mismos datos y la
comparacion es limpia.

  uso:  python3 banco-eta.py --captura traza.jsonl -- <comando>   graba
        python3 banco-eta.py traza.jsonl                          compara

QUE SE MIDE. Para cada estimador y cada linea de avance: lo que diria contra lo que de verdad
faltaba. Y se resume con la MEDIANA del error absoluto, que es lo que uno nota, mas cuantas veces
se pasa de largo y cuantas se queda corto — porque no duelen igual: decir «un minuto» cuando
faltan diez te hace irte, y decir «diez» cuando falta uno te hace no irte.
"""
import json
import re
import subprocess
import sys
import time

PAT = re.compile(r"(\d+)/(\d+) ficheros \(([\d.]+)%\), (\d+) MiB")


def captura(destino, cmd):
    t0 = time.monotonic()
    p = subprocess.Popen(cmd, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE,
                         text=True, bufsize=1)
    filas = []
    for linea in p.stderr:
        m = PAT.search(linea)
        if m:
            filas.append({"t": time.monotonic() - t0, "pct": float(m.group(3)),
                          "miB": float(m.group(4))})
    p.wait()
    fin = time.monotonic() - t0
    with open(destino, "w") as fh:
        json.dump({"fin": fin, "filas": filas}, fh)
    print("traza: %.1f s · %d lineas -> %s" % (fin, len(filas), destino))


# ── LOS ESTIMADORES. Cada uno recibe la traza y devuelve una lista de (t, restante|None) ──────
def _total(filas):
    """El denominador que ve la app: se deduce del porcentaje, igual que Motor.swift."""
    tot = 0.0
    out = []
    for f in filas:
        if f["pct"] >= 1 and f["miB"] > 0:
            tot = max(tot, f["miB"] * 100.0 / f["pct"])
        out.append(tot)
    return out


def exponencial(filas, alfa=0.3, candado=0.35):
    """EL DE HOY: media exponencial del ritmo reciente + candado de dos seguidas."""
    tots = _total(filas)
    ritmo = ult_miB = 0.0
    ult_t = 0.0
    ult_est = None
    out = []
    for f, tot in zip(filas, tots):
        dt, dm = f["t"] - ult_t, f["miB"] - ult_miB
        if dt > 0.35 and dm > 0:
            inst = dm / dt
            ritmo = inst if ritmo == 0 else ritmo * (1 - alfa) + inst * alfa
            ult_miB, ult_t = f["miB"], f["t"]
        r = None
        if ritmo > 0 and tot > 0 and f["miB"] > 0 and f["t"] >= 2 and f["miB"] / tot >= 0.01:
            bruta = max(0.0, (tot - f["miB"]) / ritmo)
            if ult_est and ult_est > 0 and abs(bruta - ult_est) / max(ult_est, 1) <= candado:
                r = bruta
            ult_est = bruta
        out.append((f["t"], r))
    return out


def media(filas):
    """EL RITMO MEDIO DE TODA LA CORRIDA. Con el denominador arreglado vuelve a tener sentido:
    la fase rapida y la lenta se promedian, y lo que queda ya no es una fraccion equivocada."""
    tots = _total(filas)
    out = []
    for f, tot in zip(filas, tots):
        r = None
        if f["t"] >= 2 and f["miB"] > 0 and tot > 0:
            r = max(0.0, f["t"] * (tot - f["miB"]) / f["miB"])
        out.append((f["t"], r))
    return out


def mezcla(filas, peso=0.5):
    """La media y la exponencial a partes iguales: la media da el suelo, la reciente reacciona."""
    a, b = dict(media(filas)), dict(exponencial(filas))
    return [(t, None if a.get(t) is None or b.get(t) is None else a[t] * peso + b[t] * (1 - peso))
            for t, _ in a.items()]


def media_amortiguada(filas):
    """La media, pero sin dejar que el final mienta: cuando queda menos del 3% del trabajo, el
    tiempo que falta no se puede estimar por regla de tres -el indice se comprime al final y no
    es proporcional-, asi que se dice «unos segundos» y punto."""
    out = []
    tots = _total(filas)
    for f, tot in zip(filas, tots):
        r = None
        if f["t"] >= 2 and f["miB"] > 0 and tot > 0:
            queda = (tot - f["miB"]) / tot
            r = 0.0 if queda < 0.03 else max(0.0, f["t"] * (tot - f["miB"]) / f["miB"])
        out.append((f["t"], r))
    return out


def compara(ruta):
    d = json.load(open(ruta))
    fin, filas = d["fin"], d["filas"]
    print("\ntraza: %.1f s · %d lineas\n" % (fin, len(filas)))
    print("%-26s %9s %9s %9s %9s" % ("estimador", "mediana", "habla", "de largo", "corto"))
    for nom, fn in (("exponencial (el de hoy)", exponencial),
                    ("ritmo medio", media),
                    ("media + exponencial", mezcla),
                    ("media, amortiguada", media_amortiguada)) + VARIANTES:
        con = [(t, r, fin - t) for t, r in fn(filas) if r is not None]
        if not con:
            print("%-26s %9s" % (nom, "nunca habla"))
            continue
        errs = sorted(abs(r - real) / max(real, 0.5) for _, r, real in con)
        largo = sum(1 for _, r, real in con if r > real * 1.5)
        corto = sum(1 for _, r, real in con if r < real * 0.66)
        print("%-26s %8.0f%% %8.0f%% %9d %9d"
              % (nom, errs[len(errs) // 2] * 100, 100 * len(con) / len(filas), largo, corto))


# ── LAS VARIANTES QUE FALTABAN ────────────────────────────────────────────────────────────────
#
# Las cuatro de arriba se quedan entre el 143% y el 334% de error mediano, asi que ninguna sirve.
# Antes de cambiar el motor otra vez conviene agotar lo que se puede probar SOBRE LA TRAZA, que
# es gratis: la traza ya esta capturada y pasarla por un estimador mas cuesta milisegundos.

def ventana(filas, seg=30.0):
    """El ritmo de los ultimos N segundos. Ni la media de toda la corrida -que mezcla la fase
    rapida con la lenta- ni la exponencial -que persigue el ultimo frame-. Con frames de ~6 s,
    una ventana de 30 cubre cinco y no la sacude ninguno."""
    tots = _total(filas)
    out = []
    for i, (f, tot) in enumerate(zip(filas, tots)):
        r = None
        j = i
        while j > 0 and f["t"] - filas[j]["t"] < seg:
            j -= 1
        dt, dm = f["t"] - filas[j]["t"], f["miB"] - filas[j]["miB"]
        if dt > 1 and dm > 0 and tot > 0 and f["t"] >= 2:
            r = max(0.0, (tot - f["miB"]) / (dm / dt))
        out.append((f["t"], r))
    return out


def ventana60(filas):
    return ventana(filas, 60.0)


def pesimista(filas):
    """El ritmo MAS LENTO sostenido: el minimo de las ventanas de 30 s vistas hasta ahora.

    La idea es que equivocarse por arriba y por abajo NO duele igual. Decir «un minuto» cuando
    faltan diez hace que te vayas y vuelvas a encontrarte la ventana igual; decir «diez» cuando
    falta uno solo hace que te sorprendas para bien. Si hay que fallar, que sea por largo."""
    v = ventana(filas, 30.0)
    tots = _total(filas)
    peor = None
    out = []
    for (t, r), f, tot in zip(v, filas, tots):
        if r is not None and tot > f["miB"]:
            ritmo = (tot - f["miB"]) / max(r, 0.001)
            peor = ritmo if peor is None else min(peor, ritmo)
            out.append((t, (tot - f["miB"]) / peor))
        else:
            out.append((t, None))
    return out


def techo_por_lo_hecho(filas):
    """LO QUE HA COSTADO LO HECHO, APLICADO A LO QUE QUEDA — pero sin regla de tres sobre bytes:
    sobre el PORCENTAJE, que es lo que el motor ya modela. Si el 40% costo 50 s, el 100% costara
    125 y quedan 75. Es la media, dicha de otra forma, y sirve de control: si sale igual que
    `media`, es que el porcentaje y los MiB cuentan lo mismo y una de las dos sobra."""
    out = []
    for f in filas:
        r = None
        if f["t"] >= 2 and f["pct"] >= 1:
            r = max(0.0, f["t"] * (100.0 - f["pct"]) / f["pct"])
        out.append((f["t"], r))
    return out


VARIANTES = (("ventana 30 s", ventana), ("ventana 60 s", ventana60),
             ("pesimista (peor ritmo)", pesimista),
             ("por el porcentaje", techo_por_lo_hecho))


def a_tsv(rutas, destino="bench/eta.tsv"):
    """La tabla para el panel, con una columna por traza y el PEOR caso al final.

    El peor caso manda y no la media: un estimador que acierta en el arbol comodo y falla en el
    dificil no sirve, porque el usuario no elige que carpeta guarda. Y una traza no es un corpus
    —esa ley ya esta pagada en este proyecto—, asi que aqui van SIEMPRE al menos dos.
    """
    import json as _j
    trazas = []
    for r in rutas:
        d = _j.load(open(r))
        trazas.append((r.split("/")[-1].replace("traza-", "").replace(".json", ""), d))
    filas = []
    for nom, fn in (("media exponencial (el viejo)", exponencial), ("ritmo medio", media),
                    ("media + exponencial", mezcla), ("media amortiguada", media_amortiguada)) \
                   + VARIANTES:
        errs, hablas = [], []
        for _, d in trazas:
            fin, fs = d["fin"], d["filas"]
            con = [(t, r, fin - t) for t, r in fn(fs) if r is not None]
            if not con:
                errs.append(None); hablas.append(0); continue
            e = sorted(abs(r - real) / max(real, 0.5) for _, r, real in con)
            errs.append(e[len(e) // 2] * 100)
            hablas.append(100 * len(con) / len(fs))
        vivos = [e for e in errs if e is not None]
        filas.append((nom, errs, hablas, max(vivos) if vivos else None))
    filas.sort(key=lambda f: f[3] if f[3] is not None else 1e9)

    with open(destino, "w", encoding="utf-8") as fh:
        fh.write(
            "# EL TIEMPO QUE FALTA: OCHO ESTIMADORES SOBRE LAS MISMAS TRAZAS.\n"
            "# GENERADO POR bench/banco-eta.py --tsv — NO SE EDITA A MANO.\n"
            "#\n"
            "# Se captura la traza de avance UNA vez por arbol y se pasa por todos los\n"
            "# estimadores, para que todos vean exactamente los mismos datos: cinco corridas\n"
            "# distintas no son comparables entre si porque la maquina no repite.\n"
            "#\n"
            "# `error` es la MEDIANA del error absoluto contra el tiempo que de verdad faltaba.\n"
            "# `habla` es en que porcentaje de las lineas se atreve a decir un numero: un\n"
            "# estimador que calla no se equivoca, y tampoco sirve.\n"
            "# Manda `peor`, no la media de las columnas: el usuario no elige que carpeta guarda.\n")
        cab = ["estimador"]
        for nom, _ in trazas:
            cab += ["error_" + nom, "habla_" + nom]
        cab.append("peor")
        fh.write("\t".join(cab) + "\n")
        for nom, errs, hablas, peor in filas:
            fila = [nom]
            for e, h in zip(errs, hablas):
                fila += ["-" if e is None else "%.0f%%" % e, "%.0f%%" % h]
            fila.append("-" if peor is None else "%.0f%%" % peor)
            fh.write("\t".join(fila) + "\n")
    print("%s · %d estimadores · %d trazas" % (destino, len(filas), len(trazas)))


if __name__ == "__main__":
    if "--tsv" in sys.argv:
        a_tsv([x for x in sys.argv[1:] if not x.startswith("--")])
    elif "--captura" in sys.argv:
        i = sys.argv.index("--captura")
        j = sys.argv.index("--")
        captura(sys.argv[i + 1], sys.argv[j + 1:])
    else:
        compara(sys.argv[1])
