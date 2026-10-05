# -*- coding: utf-8 -*-
"""ANIADIR FILAS AL LIBRO MAYOR SIN PODER DUPLICARLAS.

POR QUE EXISTE. `bench/regresion-reloj.sh` mide en un worktree aparte y deja las filas en
`/tmp/v0-filas.jsonl`, y luego imprime esto para que las integres:

    cat /tmp/v0-filas.jsonl >> bench/historia.jsonl

Eso es una bomba con la espoleta puesta. El libro es append-only, no tiene clave unica y nadie lo
auditaba: correr esa linea dos veces -por costumbre, por releer el log de arriba abajo, por volver
a la sesion al dia siguiente- mete las filas repetidas y no pasa nada visible. Las filas repetidas
no rompen: PROMEDIAN. Y a partir de ese momento todas las cifras de ese experimento salen un poco
mal, para siempre, sin un solo aviso.

El 03-08 el fichero seguia en /tmp con sus nueve filas YA integradas y la orden seguia impresa.

QUE HACE. Lo mismo, pero sin poder duplicar: compara por la misma clave que usa
`bench/libro.py` -segundo, etiqueta, muestra, config, binario y comando-, salta lo que ya esta y
dice cuantas anadio y cuantas se salto. Correrlo dos veces deja el libro igual que correrlo una.

  uso:  python3 bench/integra.py /tmp/v0-filas.jsonl [...]
        python3 bench/integra.py --seco /tmp/v0-filas.jsonl   (dice que haria y no toca nada)
"""
import json
import os
import sys

RAIZ = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
B = os.path.join(RAIZ, "bench")
sys.path.insert(0, B)
from libro import CLAVE  # la MISMA clave que audita; dos definiciones de «duplicado» no valen


def clave(f):
    return tuple(str(f.get(c, "")) for c in CLAVE)


def main(argv):
    seco = "--seco" in argv
    fuentes = [a for a in argv if not a.startswith("--")]
    if not fuentes:
        print(__doc__.strip().splitlines()[-2].strip(), file=sys.stderr)
        return 2

    destino = os.path.join(B, "historia.jsonl")
    ya = set()
    try:
        for l in open(destino, encoding="utf-8"):
            l = l.strip()
            if l:
                try:
                    ya.add(clave(json.loads(l)))
                except json.JSONDecodeError:
                    pass
    except OSError:
        pass

    nuevas, saltadas, malas = [], 0, 0
    for f in fuentes:
        try:
            lineas = open(f, encoding="utf-8").read().splitlines()
        except OSError as e:
            print("no se puede leer %s: %s" % (f, e), file=sys.stderr)
            return 1
        for l in lineas:
            l = l.strip()
            if not l:
                continue
            try:
                r = json.loads(l)
            except json.JSONDecodeError:
                malas += 1
                continue
            k = clave(r)
            if k in ya:
                saltadas += 1
                continue
            ya.add(k)
            nuevas.append(l)

    print("%d nuevas · %d ya estaban · %d no parsean" % (len(nuevas), saltadas, malas))
    if seco:
        print("(--seco: no se ha tocado nada)")
        return 0
    if nuevas:
        with open(destino, "a", encoding="utf-8") as fh:
            for l in nuevas:
                fh.write(l + "\n")
        print("anadidas a bench/historia.jsonl")
    return 1 if malas else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
