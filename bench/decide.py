# -*- coding: utf-8 -*-
"""QUE SE DECIDIO, Y SI LA MEDIDA LO SOSTIENE.

POR QUE EXISTE. Pregunta del dueno, 03-08: «tenemos que hacer algo para que todo lo que se haga y
se mida quede a su vez grabado pero BIEN grabado en el panel». La auditoria de ese dia encontro
que el instrumento estaba bien y la contabilidad no:

  · El mayor almacen de cifras del proyecto es EL CUERPO DE LOS MENSAJES DE COMMIT -52 tablas en
    los 67 commits del 02 y el 03 de agosto- y el panel lee %h, %s y %cI, nunca %b.
  · Una cifra publicada podia no cuadrar con el libro mayor sin que nadie lo notara. El commit
    4e2fa6b publica el negativo del hash compartido como «+3,1% de ciclos» sobre TRES muestras;
    en bench/historia.jsonl hay CUATRO pares y el descartado -csvjson, +7,86%- es el peor. El
    agregado real es +4,29%. El descarte no quedo anotado porque no habia campo donde anotarlo.
  · Se cerraron racimos incompletos: la ablacion que se APLICO POR DEFECTO (v6) y el negativo del
    hash se midieron sobre 4 de las 5 muestras de texto. Falta enwik8 en las dos.
  · No existia registro de lo NO aplicado. Habia bench/aplicado.tsv y no su contraparte, asi que
    los negativos vivian en comentarios de crates/ y en cuerpos de commit.

LA DECISION DE DISENO, que es la unica que importa: el humano declara la IDENTIDAD del experimento
y la DECISION; los numeros los pone esto, leyendolos de bench/historia.jsonl. Una cifra, un sitio.
Si la cifra se pudiera escribir a mano en dos sitios, tarde o temprano dirian cosas distintas — y
ya dijeron cosas distintas.

Es el mismo patron que bench/aplicado.py, un nivel mas arriba: alli no hay ni un dato a mano, solo
anclas y cuentas. Aqui tampoco.

LO QUE COMPRUEBA, y cada uno tapa un agujero real de ese dia:
  · COMPLETITUD  . faltan muestras del juego declarado en bench/muestras.tsv  -> INCOMPLETA
  · ANCLA        . el fuente sigue diciendo lo que la decision afirma          -> DESANCLADA
  · MEDIDA       . la etiqueta citada tiene filas de verdad                    -> SIN MEDIDA
  · FRESCURA     . el fichero anclado se ha tocado despues de medir            -> REVISAR

⚠️ NO INVENTA. Si una decision cita una etiqueta sin filas, sale SIN MEDIDA en rojo; no se rellena
con la cifra del mensaje de commit ni con una estimacion. Mismo criterio que `lee_ancla` en
bench/aplicado.py: un ancla que no aparece PARA el generador en vez de devolver un numero comodo.

  lo escribe: bench/decide.py -> bench/decisiones-medidas.tsv
  lo lee:     bench/panel.py, seccion «Que se decidio, y con que medida»
"""
import collections
import io
import os
import re
import subprocess
import sys

RAIZ = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
B = os.path.join(RAIZ, "bench")
sys.path.insert(0, B)
import comun_banco as CB  # noqa: E402

# El juego de muestras y las decisiones son TEXTO PLANO a proposito: se leen y se corrigen sin
# herramienta, igual que el resto del banco.
MODELO_HOY = ""
COLS = ("id", "que", "veredicto", "juego", "estado", "bytes_pct", "ciclos_pct", "medidas",
        "faltan", "etiqueta", "detalle", "arbol", "cuando", "ancla", "medido_en", "caduca_si")


def tsv(p):
    """Igual que el del panel: salta vacias y comentarios, y la primera util es la cabecera."""
    fs = []
    try:
        for l in io.open(os.path.join(B, p), encoding="utf-8"):
            l = l.rstrip("\n")
            if not l.strip() or l.lstrip().startswith("#"):
                continue
            fs.append(l.split("\t"))
    except OSError:
        pass
    return fs


def sh(c):
    try:
        r = subprocess.run(c, shell=True, cwd=RAIZ, capture_output=True, text=True, timeout=30)
        return r.stdout.strip()
    except Exception:
        return ""


def pc(x, dec=3):
    """Porcentaje en espanol. `None` no se dibuja como 0: se dibuja como que no se midio."""
    if x is None:
        return "-"
    return ("%+.*f%%" % (dec, x * 100)).replace(".", ",")


def media(rs, campo):
    vs = [r[campo] for r in rs if r.get(campo)]
    return sum(vs) / len(vs) if vs else None


def _es_comentario(t):
    return t.startswith("//") or t.startswith("/*") or t.startswith("*") or t.startswith("#")


def cambios_reales(desde_t, fichero, arboles_medidos):
    """Commits que tocaron el fichero despues de medir Y cambiaron algo que NO es un comentario.

    🔑 LA PRIMERA VERSION DE ESTO ERA INUTIL Y LO DIJO EL PROPIO INSTRUMENTO. Marcaba REVISAR en 6
    de 7 decisiones, y al mirar por que, salieron DOS falsos positivos distintos:

      · 485ccca solo anadia un COMENTARIO a vc-cm/src/lib.rs. Un comentario no invalida una
        medida. Se mira el diff de verdad: si todas las lineas anadidas y quitadas son comentario
        o blanco, ese commit no cuenta.
      · 4dec9db es EL COMMIT DE LA PROPIA DECISION. Aqui se mide primero y se commitea despues,
        asi que el commit que recoge la medida trae exactamente el codigo que se midio y aparecia
        como si lo hubiera cambiado despues. Se compara por `arbol_sha`: si el arbol del commit
        es uno de los que se midieron, es el mismo codigo con otro nombre.

    Un aviso que salta siempre no es un aviso, es un adorno — y lo que pasa despues es que se
    desactiva, y con el se pierden los que si valian.
    """
    cs = sh("git log --format=%%H --since='%s' -- %s" % (desde_t, fichero)).split()
    reales = []
    for c in cs:
        if CB.arbol_de_commit(sh, c) in arboles_medidos:
            continue
        for l in sh("git show --format= --unified=0 %s -- %s" % (c, fichero)).splitlines():
            if l[:3] in ("+++", "---") or not l or l[0] not in "+-":
                continue
            t = l[1:].strip()
            if t and not _es_comentario(t):
                reales.append(c[:7])
                break
    return reales


def modelo_en(arboles, _cache={}):
    """En que MODELO de vc-cm se midio esto. Del fuente del commit, no de la memoria de nadie.

    🔑 POR QUE SUSTITUYE A «REVISAR». La primera version marcaba REVISAR cuando el fichero anclado
    se tocaba despues de medir. Con la v8 salto en LAS DIEZ decisiones a la vez —el commit toco
    vc-cm/src/lib.rs, y ahi viven todas— y volvio a ser lo mismo que ya se corrigio una vez: un
    aviso que sale siempre no es un aviso. Y lo que pasa despues es que se apaga.

    Lo que de verdad se quiere saber no es «se toco el fichero» sino «cambio la PREMISA», y aqui
    la premisa tiene nombre y numero: la version del modelo. Medido en la v7 y el defecto es la
    v8 es un dato util —dice cuanto hay que fiarse y de que—; «el fichero cambio» no dice nada,
    porque el fichero cambia todos los dias.

    No es una alarma: no pone la fila en rojo. Es una etiqueta.
    """
    for a in sorted(arboles):
        if a in _cache:
            if _cache[a]:
                return _cache[a]
            continue
        _cache[a] = ""
        for c in sh("git log --format=%H -n 150").split():
            if CB.arbol_de_commit(sh, c) != a:
                continue
            for l in sh("git show %s:crates/vc-cm/src/lib.rs" % c).splitlines():
                if l.startswith("pub const MODELO: u8 ="):
                    _cache[a] = "v" + l.split("=")[1].strip().rstrip(";")
                    break
            break
        if _cache[a]:
            return _cache[a]
    return ""


def mira_ancla(fichero, patron):
    """Igual de estricta que bench/aplicado.py: el ancla tiene que salir EXACTAMENTE UNA VEZ.

    Un ancla ambigua es como leer `isse_lr = 3` porque coincidio con la declaracion del campo en
    vez de con el valor. Mejor decir «ambigua» que devolver el primero y quedarse tan tranquilo.
    """
    fichero, patron = (fichero or "").strip(), (patron or "").strip()
    if not fichero or not patron:
        return "sin ancla", ""
    try:
        s = io.open(os.path.join(RAIZ, fichero), encoding="utf-8").read()
    except OSError:
        return "fichero ausente", fichero
    try:
        m = re.findall(patron, s, re.M)
    except re.error as e:
        return "regex invalida", str(e)
    if len(m) == 0:
        return "no casa", ""
    if len(m) > 1:
        return "ambigua", "%d veces" % len(m)
    return "casa", ""


def calcula():
    """Devuelve (salida, errores, (filas, no_validas, corruptas)).

    Vive en una funcion, y no en el cuerpo de main(), para que bench/cierra.py -el que
    redacta el mensaje de commit- use ESTE calculo y no otro parecido. Si cierra.py volviera
    a parsear el TSV ya formateado, habria dos caminos hasta la misma cifra, y a la tercera
    semana discreparian por un redondeo. Que es, exactamente, el problema que este fichero
    existe para cerrar.
    """
    errores = []

    # ── 1. El juego canonico ──────────────────────────────────────────────────────────────────
    mu = tsv("muestras.tsv")
    if not mu:
        errores.append("no se puede leer bench/muestras.tsv: sin el juego canonico no hay "
                       "completitud que comprobar, y comprobar a medias es peor que no "
                       "comprobar")
        return [], errores, (0, 0, 0)
    JUEGOS = collections.OrderedDict()
    for f in mu[1:]:
        if len(f) < 2:
            continue
        JUEGOS.setdefault(f[0].strip(), []).append(f[1].strip())

    # ── 2. Las decisiones declaradas ──────────────────────────────────────────────────────────
    de = tsv("decisiones.tsv")
    if not de:
        errores.append("no se puede leer bench/decisiones.tsv")
        return [], errores, (0, 0, 0)
    ix = {c.strip(): i for i, c in enumerate(de[0])}
    for c in ("id", "veredicto", "etiqueta", "juego", "control", "tratamiento"):
        if c not in ix:
            errores.append("a bench/decisiones.tsv le falta la columna obligatoria "
                           "'%s'" % c)
            return [], errores, (0, 0, 0)

    def col(f, c):
        i = ix.get(c)
        return f[i].strip() if i is not None and len(f) > i else ""

    global MODELO_HOY
    for _l in io.open(os.path.join(RAIZ, "crates/vc-cm/src/lib.rs"), encoding="utf-8"):
        if _l.startswith("pub const MODELO: u8 ="):
            MODELO_HOY = "v" + _l.split("=")[1].strip().rstrip(";")
            break

    # ── 3. El libro mayor ─────────────────────────────────────────────────────────────────────
    HIST, DESC, CORR = CB.carga_libro(os.path.join(B, "historia.jsonl"))
    if CORR:
        errores.append("%d lineas corruptas en historia.jsonl" % CORR)

    # Cuando se toco por ultima vez cada fichero anclado. Sirve para la frescura: no es «ha
    # cambiado el repo» -eso pasa cada commit y seria ruido- sino «ha cambiado EL FICHERO DONDE
    # VIVE ESTO despues de haberlo medido», que es cuando de verdad hay que volver a mirar.
    toque = {}

    salida = []
    for f in de[1:]:
        if len(f) < 6:
            continue
        et = col(f, "etiqueta")
        juego = col(f, "juego")
        ctrl_c, trat_c = col(f, "control"), col(f, "tratamiento")

        if juego not in JUEGOS:
            errores.append("la decision '%s' cita el juego '%s', que no esta en muestras.tsv"
                           % (col(f, "id"), juego))
            continue

        filas = [r for r in HIST if CB.sin_vuelta(r.get("etiqueta")) == et]
        lado_c, lado_t = collections.defaultdict(list), collections.defaultdict(list)
        for r in filas:
            c = CB.sin_vuelta(r.get("config"))
            m = (r.get("muestra") or "").strip()
            if c == ctrl_c:
                lado_c[m].append(r)
            elif c == trat_c:
                lado_t[m].append(r)

        det, medidas, faltan, crudo = [], [], [], []
        sb_c = sb_t = sc_c = sc_t = 0.0
        cuando, arbol = "", set()
        for m in JUEGOS[juego]:
            a, b = lado_c.get(m), lado_t.get(m)
            if not a or not b:
                faltan.append(m)
                continue
            # Los BYTES exigen entrada IDENTICA. No es tiquismiquis: comparar 10.485.760 contra
            # 10.485.759 no es medir una mejora, es restar dos cosas distintas. Cambiar la
            # etiqueta no basta para declarar otra muestra; manda el tamano.
            if not CB.misma_entrada(a[0], b[0], exacta=True):
                det.append("%s ENTRADA DISTINTA" % m)
                faltan.append(m)
                continue
            ba, bb = media(a, "bytes_out"), media(b, "bytes_out")
            ca, cb = media(a, "ciclos"), media(b, "ciclos")
            medidas.append(m)
            db = (bb - ba) / ba if ba else None
            dc = (cb - ca) / ca if (ca and cb) else None
            det.append("%s %s / %s" % (m, pc(db), pc(dc, 1)))
            # El MISMO dato sin formatear, para quien tenga que componer otra tabla -el que
            # redacta el commit-. Volver a parsear la cadena de arriba seria fabricar un
            # segundo camino hasta la misma cifra: dos caminos, dos cifras, tarde o temprano.
            crudo.append({"muestra": m, "bytes": db, "ciclos": dc,
                          "ctrl_bytes": ba, "trat_bytes": bb})
            if ba and bb:
                sb_c += ba
                sb_t += bb
            if ca and cb:
                sc_c += ca
                sc_t += cb
            for r in a + b:
                cuando = max(cuando, r.get("t") or "")
                if r.get("arbol_sha"):
                    arbol.add(r["arbol_sha"])

        ag_b = (sb_t - sb_c) / sb_c if sb_c else None
        ag_c = (sc_t - sc_c) / sc_c if sc_c else None

        # ── El veredicto del instrumento, que no es el del humano ─────────────────────────────
        estado = []
        if not medidas:
            estado.append("SIN MEDIDA")
        elif faltan:
            estado.append("INCOMPLETA")

        anc, extra = mira_ancla(col(f, "ancla_fichero"), col(f, "ancla_patron"))
        if anc == "no casa":
            estado.append("DESANCLADA")
        elif anc == "sin ancla":
            estado.append("SIN ANCLA")
        elif anc != "casa":
            estado.append("ANCLA %s" % anc.upper())
            errores.append("ancla de '%s': %s %s" % (col(f, "id"), anc, extra))

        fich = col(f, "ancla_fichero")
        # La version en la que se midio, contra la de hoy. Ver `modelo_en`: esto NO pone la fila
        # en rojo, es una etiqueta. La alarma la reservan las cuatro que si significan «esto no
        # esta sostenido»: SIN MEDIDA, INCOMPLETA, DESANCLADA y ANCLA ROTA.
        medido_en = modelo_en(arbol) if arbol else ""
        # ⚠️ Y SI NO SE ENCUENTRA EL ARBOL, SE DICE. Caer al valor de hoy seria afirmar que
        # se midio en la version actual, que es exactamente lo que no se sabe. Pasa cuando la
        # medida se tomo con el arbol sucio y ese codigo no llego a ser ningun commit: es
        # informacion util -esa fila es menos fiable que las demas- y taparla con un numero
        # que suena bien es la forma mas facil de que nadie se entere.
        if not medido_en:
            tocado = ["arbol sin commit"]
        elif medido_en != MODELO_HOY:
            tocado = [medido_en + " → " + MODELO_HOY]
        else:
            tocado = []
        if not estado:
            estado.append("OK")

        salida.append({
            "id": col(f, "id"), "que": col(f, "que"), "veredicto": col(f, "veredicto"),
            "juego": juego, "estado": " + ".join(estado),
            "bytes_pct": pc(ag_b), "ciclos_pct": pc(ag_c, 1),
            "medidas": "%d/%d" % (len(medidas), len(JUEGOS[juego])),
            "faltan": ", ".join(faltan) or "-",
            "etiqueta": et, "detalle": " · ".join(det) or "-",
            "arbol": ", ".join(sorted(arbol)) or "-", "cuando": cuando[:16] or "-",
            "ancla": anc + ((" (" + extra + ")") if extra else ""),
            "medido_en": (tocado[0] if tocado else MODELO_HOY),
            "caduca_si": col(f, "caduca_si"),
            # Lo que NO va al TSV pero si a quien importe este modulo. Con guion bajo para
            # que se vea de un vistazo que no es columna del fichero.
            "_filas": crudo, "_por_que": col(f, "por_que"),
            "_ancla_fichero": fich, "_control": ctrl_c, "_tratamiento": trat_c,
        })

    return salida, errores, (len(HIST), DESC, CORR)


def main():
    salida, errores, (nfilas, DESC, CORR) = calcula()

    # ── 4. Se escribe ─────────────────────────────────────────────────────────────────────────
    cab = ("# QUE SE DECIDIO, Y SI LA MEDIDA LO SOSTIENE. GENERADO POR bench/decide.py — NO SE "
           "EDITA A MANO.\n"
           "#\n"
           "# Ni una de estas cifras esta escrita por nadie: todas salen de bench/historia.jsonl.\n"
           "# Lo que se declara a mano es la IDENTIDAD del experimento, y vive en\n"
           "# bench/decisiones.tsv. Si un numero de aqui no te gusta, la respuesta es medir otra\n"
           "# vez, no editarlo.\n"
           "#\n"
           "# `estado` es el veredicto DEL INSTRUMENTO, no el del humano: OK, INCOMPLETA (faltan\n"
           "# muestras del juego), SIN MEDIDA (la etiqueta no tiene filas), DESANCLADA (decidido y\n"
           "# sin aplicar), SIN ANCLA (no se puede comprobar) y REVISAR (el fichero se toco\n"
           "# despues de medir).\n"
           "#\n"
           "# `bytes_pct` y `ciclos_pct` son el agregado: suma del tratamiento contra suma del\n"
           "# control sobre las muestras emparejadas. NEGATIVO ES MEJOR en las dos.\n"
           "# `detalle` lleva muestra a muestra: «muestra bytes / ciclos».\n")
    with io.open(os.path.join(B, "decisiones-medidas.tsv"), "w", encoding="utf-8") as fh:
        fh.write(cab)
        fh.write("\t".join(COLS) + "\n")
        for r in salida:
            fh.write("\t".join(str(r[c]).replace("\t", " ") for c in COLS) + "\n")

    # ── 5. Y se cuenta lo que ha pasado ───────────────────────────────────────────────────────
    print("bench/decisiones-medidas.tsv · %d filas · %d del libro mayor (%d descartadas por "
          "rc, %d corruptas)" % (len(salida), nfilas, DESC, CORR))
    an = collections.Counter(e for r in salida for e in r["estado"].split(" + "))
    for k, v in an.most_common():
        print("   %-12s %d" % (k, v))
    for r in salida:
        if r["estado"] != "OK":
            print("   ⚠ %-24s %-10s %s%s" % (r["id"], r["juego"], r["estado"],
                                             ("  faltan: " + r["faltan"])
                                             if r["faltan"] != "-" else ""))
    for e in errores:
        print("   ✗ %s" % e, file=sys.stderr)
    # Un error de DECLARACION -un juego que no existe, una regex invalida- es culpa del fichero y
    # se para. Un estado feo -INCOMPLETA, DESANCLADA- es culpa de la REALIDAD y se pinta: para eso
    # esta el panel. Confundir las dos cosas es lo que hace que se desactiven los avisos.
    return 1 if errores else 0


if __name__ == "__main__":
    sys.exit(main())
