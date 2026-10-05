# -*- coding: utf-8 -*-
"""EL LIBRO MAYOR, AUDITADO. Integridad de bench/historia.jsonl.

POR QUE EXISTE. El libro es APPEND-ONLY, no tiene clave unica y nadie comprobaba nada. El 03-08
habia una bomba armada y a la vista: `/tmp/v0-filas.jsonl` con nueve filas YA integradas, y en
`/tmp/v0.log` la instruccion impresa `cat /tmp/v0-filas.jsonl >> bench/historia.jsonl`. Ejecutarla
otra vez -por costumbre, o por leer el log de arriba abajo- mete nueve filas repetidas en un libro
que nadie audita. Nueve filas duplicadas no rompen nada de golpe: promedian, y las cifras salen un
poco mal para siempre.

  uso:  python3 bench/libro.py            -> informe, siempre exit 0
        python3 bench/libro.py --verifica -> exit 1 si hay duplicados, corrupcion o esquema roto

⚠️ LOS DOS CANDADOS QUE SE DESCARTARON, Y CON QUE NUMEROS. Un negativo de diseño vale lo mismo que
uno de medida, asi que quedan escritos para que no se vuelvan a proponer:

  · VOCABULARIO DE ETIQUETAS -exigir que toda `etiqueta` empiece por un prefijo declarado-. Se
    conto: en 641 filas hay MAS DE 28 prefijos distintos (`vlz` 76, `F2.` 41, `fase3` 35, `DMC`
    33, `ISSE` 30, `F13` 24, `LIM` 20, `CERO` 18, `ranura:` 18...). La etiqueta no es una
    taxonomia, es un nombre libre cuyo trabajo es AGRUPAR un experimento — y agrupando funciona.
    El candado marcaria cientos de filas historicas en rojo el primer dia, y un aviso que sale
    seiscientas veces se apaga. Donde SI tiene dientes es en bench/decisiones.tsv, que cita una
    etiqueta exacta: si te equivocas al teclearla, bench/decide.py canta SIN MEDIDA. Ese candado
    ya existe y es el que hacia falta.

  · MUESTRA OBLIGATORIA EN bench/muestras.tsv. Hay 70 muestras distintas en el libro y el juego
    canonico declara 12: `arbol2.7g` (105 filas), `logs.8m` (28), `codigo.2m` (17)... son
    exploracion legitima de otras epocas. Como error seria falso 58 veces; como AVISO en mide.sh
    -«esto no esta en el juego, seguro?»- es util y no estorba. Ahi se queda.

Lo que si se comprueba es lo que solo puede ser un fallo: dos filas identicas, una linea que no
parsea, y una fila sin los campos con los que se identifica una medida.
"""
import collections
import json
import os
import sys

RAIZ = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
B = os.path.join(RAIZ, "bench")
sys.path.insert(0, B)
import comun_banco as CB  # noqa: E402

# Con lo que se identifica una medida. Sin esto no se sabe QUE se midio ni COMO repetirlo, que es
# la mitad del valor de una fila -R4: ni una cifra sin su comando.
OBLIGATORIOS = ("t", "etiqueta", "muestra", "cmd", "rc")

# QUE ES UN DUPLICADO. `mide.sh` toma un `flock` antes de escribir, asi que las medidas van
# forzosamente en serie; y `t` tiene resolucion de SEGUNDO. Dos medidas de verdad no pueden
# compartir el segundo Y la etiqueta Y la muestra Y la config Y el binario Y el comando: para eso
# tendrian que haber corrido a la vez, que es justo lo que el cerrojo impide. Si coinciden en las
# seis, la fila se escribio dos veces.
CLAVE = ("t", "etiqueta", "muestra", "config", "bin_sha", "cmd")


def main():
    ruta = os.path.join(B, "historia.jsonl")
    filas, corruptas, n = [], [], 0
    try:
        for i, l in enumerate(open(ruta, encoding="utf-8"), 1):
            l = l.strip()
            if not l:
                continue
            n += 1
            try:
                filas.append((i, json.loads(l)))
            except json.JSONDecodeError as e:
                corruptas.append((i, str(e)[:60]))
    except OSError as e:
        print("no se puede leer %s: %s" % (ruta, e), file=sys.stderr)
        return 1

    vistas, dups = {}, []
    for i, f in filas:
        k = tuple(str(f.get(c, "")) for c in CLAVE)
        if k in vistas:
            dups.append((vistas[k], i, f.get("etiqueta", ""), f.get("muestra", "")))
        else:
            vistas[k] = i

    sin_campo = collections.defaultdict(list)
    for i, f in filas:
        for c in OBLIGATORIOS:
            if f.get(c) in (None, ""):
                sin_campo[c].append(i)

    # Informativo, no error: son datos de verdad y su ausencia se explica sola.
    malas = sum(1 for _, f in filas if not CB.valida(f))
    sin_arbol = sum(1 for _, f in filas if not f.get("arbol_sha"))
    sucio_cod = sum(1 for _, f in filas if f.get("sucio_codigo"))

    # Muestras que no estan en el juego canonico. AVISO, nunca error — ver la cabecera.
    juego = set()
    try:
        for l in open(os.path.join(B, "muestras.tsv"), encoding="utf-8"):
            if l.strip() and not l.lstrip().startswith("#"):
                p = l.rstrip("\n").split("\t")
                if len(p) > 1:
                    juego.add(p[1].strip())
    except OSError:
        pass
    fuera = collections.Counter((f.get("muestra") or "").strip() for _, f in filas
                                if (f.get("muestra") or "").strip() not in juego)

    print("bench/historia.jsonl · %d lineas · %d filas" % (n, len(filas)))
    print("   %-22s %d" % ("duplicadas", len(dups)))
    print("   %-22s %d" % ("corruptas", len(corruptas)))
    print("   %-22s %d" % ("sin campo obligatorio", sum(len(v) for v in sin_campo.values())))
    print("   %-22s %d  (rc!=0 o sin bytes: no son dato, pero saber que fallaron vale)"
          % ("no validas", malas))
    print("   %-22s %d  (anteriores al 02-08, cuando nacio el sello)" % ("sin arbol_sha", sin_arbol))
    print("   %-22s %d  (se mide antes de commitear: lo cubre arbol_sha)"
          % ("codigo sucio", sucio_cod))
    print("   %-22s %d de %d en el juego canonico"
          % ("muestras fuera", sum(fuera.values()), len(filas)))

    for a, b, et, mu in dups[:10]:
        print("   ✗ lineas %d y %d IDENTICAS · %s · %s" % (a, b, et[:44], mu))
    for i, e in corruptas[:10]:
        print("   ✗ linea %d no parsea: %s" % (i, e))
    for c, ls in sorted(sin_campo.items()):
        print("   ✗ %d filas sin '%s' (primera: linea %d)" % (len(ls), c, ls[0]))
    if fuera:
        print("   · fuera del juego: %s" % ", ".join(
            "%s(%d)" % (k or "(vacia)", v) for k, v in fuera.most_common(8)))

    roto = bool(dups or corruptas or sin_campo)
    if "--verifica" in sys.argv:
        # Solo estas tres paran. Lo demas es la realidad del banco, y la realidad se cuenta, no se
        # bloquea: un verificador que falla por cosas normales acaba desactivado, y entonces
        # tampoco caza las anormales.
        return 1 if roto else 0
    return 0


if __name__ == "__main__":
    sys.exit(main())
