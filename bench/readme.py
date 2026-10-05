# -*- coding: utf-8 -*-
"""LAS CIFRAS PUBLICAS SE GENERAN DEL LIBRO MAYOR. Nadie las teclea.

POR QUE EXISTE. El README es lo unico que ve alguien de fuera, y el 03-08 publicaba dos cosas
falsas:

  · «Entre un 8 y un 11% mas lentos» que zpaq. Esa medida era del 28-07 (145,0 s contra 132,8) y
    el 01-08 el reloj se triplico para comprar un 1,5% de bytes. Medido hoy con el mismo
    instrumento: 740,8 s nuestros contra 137,9 de zpaq, o sea 5,4 VECES mas lentos. El README ya
    documenta una correccion anterior de ESE MISMO parrafo -«estuvo mal publicado»- y volvia a
    estar mal. Dos veces el mismo parrafo no es mala suerte: es que las cifras publicas escritas
    a mano caducan y nadie se entera.
  · `codigo 823.521 B` en la tabla de 10 MB. Hoy la misma muestra da 787.470. Y 823.521 no
    aparece ni UNA vez en las 660 filas del libro mayor.

La respuesta no es acordarse de repasar el README. Es que no se pueda teclear una cifra publica:
los bloques marcados los reescribe esto leyendo bench/historia.jsonl, y lo que no sale del libro
no se publica.

  uso:  python3 bench/readme.py            reescribe los bloques
        python3 bench/readme.py --seco     dice que cambiaria y no toca nada

⚠️ LOS RIVALES NO SALEN DEL LIBRO, salen de bench/rivales.tsv, y esta bien: el resultado de zpaq
sobre una entrada no cambia nunca, asi que se mide UNA vez y se cachea. Lo que se mueve es lo
nuestro. La excepcion es el RELOJ del rival, que si depende de la maquina y del dia: ese sale del
libro, de las filas «CAMPO:», porque comparar nuestro reloj de hoy con el suyo de hace nueve dias
es exactamente el error que este fichero viene a impedir.
"""
import io
import json
import os
import re
import sys

RAIZ = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
B = os.path.join(RAIZ, "bench")
sys.path.insert(0, B)
import comun_banco as CB  # noqa: E402

TEXTO5 = ["codigo.10m", "csvjson.10m", "enwik8.10m", "logs.10m", "sql.10m"]
NOMBRE = {"codigo.10m": "código", "csvjson.10m": "CSV/JSON", "enwik8.10m": "enwik8",
          "logs.10m": "logs", "sql.10m": "SQL"}
MARCA = re.compile(r"<!-- MEDIDO: ([a-z0-9-]+) -->.*?<!-- /MEDIDO[^>]*-->", re.S)

HIST, _, _ = CB.carga_libro(os.path.join(B, "historia.jsonl"))


def ultima(**cond):
    """La fila mas reciente que cumpla todo. Mas reciente, no la mejor: publicar la mejor de la
    historia en vez de la ultima es como se acaba publicando un numero que ya no existe."""
    fs = [r for r in HIST if all((r.get(k) or "") == v for k, v in cond.items())]
    return max(fs, key=lambda r: r.get("t", "")) if fs else None


def mil(n):
    return "{:,}".format(int(n)).replace(",", ".")


def seg(x):
    return ("%.1f" % x).replace(".", ",")


def pct(x):
    """Porcentaje en espanol. Se formatea AQUI, cifra a cifra: la primera version hacia un
    `.replace(".", ",")` sobre la fila entera y eso convierte tambien los puntos de millar —
    823.521 se publicaba como 823,521, que en ingles es ochocientos veintitres COMA algo."""
    return ("%+.1f%%" % x).replace(".", ",")


def bloque_velocidad():
    ns = ultima(etiqueta="RIVALES: corpus de codigo contra el campo", config="max")
    nd = ultima(etiqueta="RIVALES: corpus de codigo contra el campo", config="defecto")
    z = ultima(etiqueta="CAMPO: zpaq -m5 en el corpus de codigo", config="zpaq 7.15 -m5")
    if not (ns and z):
        return None, "faltan filas: mide el corpus de codigo con --max y zpaq con la bascula"
    veces = ns["seg"] / z["seg"]
    L = ["| | bytes | segundos | |", "|---|---|---|---|"]
    L.append("| **vcompresor `--max`** | **%s** | %s | **%s× más lento que zpaq** |"
             % (mil(ns["bytes_out"]), seg(ns["seg"]), seg(veces)))
    if nd:
        # El defecto va MAS RAPIDO que zpaq, asi que se dice del derecho. La primera version
        # ponia «0,2×», que se lee como si fuera cinco veces peor cuando es cinco veces mejor:
        # una razon menor que uno es exactamente la forma de escribir un numero al reves.
        L.append("| vcompresor, por defecto | %s | %s | %s× más **rápido** que zpaq |"
                 % (mil(nd["bytes_out"]), seg(nd["seg"]), seg(z["seg"] / nd["seg"])))
    L.append("| zpaq 7.15 `-m5` | %s | %s | |"
             % (mil(z["bytes_out"]) if z.get("bytes_out") else "—", seg(z["seg"])))
    L += ["",
          ("**Somos %s veces más lentos que zpaq**, y es reciente: el 28-07 la misma medida daba "
           "145,0 s contra 132,8. El **01-08 el reloj se triplicó** para comprar un 1,5%% de "
           "bytes, y está medido y explicado en "
           "`docs/f4-velocidad-por-que-son-mas-rapidos.md`.") % seg(veces)]
    return "\n".join(L), None


def bloque_texto5():
    riv = {}
    for l in io.open(os.path.join(B, "rivales.tsv"), encoding="utf-8"):
        if not l.strip() or l.lstrip().startswith("#"):
            continue
        p = l.rstrip("\n").split("\t")
        if len(p) >= 4:
            riv[(p[0].strip(), p[1].strip())] = p[3].strip()

    L = ["| muestra | vcompresor `--max` | zpaq -m5 | | paq8px -8 | |",
         "|---|---|---|---|---|---|"]
    tn = tz = tp = 0
    for m in TEXTO5:
        r = ultima(etiqueta="V1b racimos sobre v7", config="control v7", muestra=m)
        if not r:
            return None, "falta la medida por defecto de %s" % m
        z = riv.get((m, "zpaq -m5"))
        p = riv.get((m, "paq8px -8"))
        n = int(r["bytes_out"])
        tn += n
        fila = "| %s | **%s** |" % (NOMBRE[m], mil(n))
        if z:
            tz += int(z)
            fila += " %s | %s |" % (mil(z), pct((n / int(z) - 1) * 100))
        else:
            fila += " — | |"
        if p:
            tp += int(p)
            fila += " %s | **%s** |" % (mil(p), pct((n / int(p) - 1) * 100))
        else:
            fila += " — | |"
        L.append(fila)
    if not (tz and tp):
        return None, "faltan rivales en bench/rivales.tsv para el juego de texto"
    L.append("| **total** | **%s** | %s | **%s** | %s | **%s** |"
             % (mil(tn), mil(tz), pct((tn / tz - 1) * 100), mil(tp), pct((tn / tp - 1) * 100)))
    return "\n".join(L), None


BLOQUES = {"velocidad-codigo": bloque_velocidad, "texto5-max": bloque_texto5}


def main():
    seco = "--seco" in sys.argv
    p = os.path.join(RAIZ, "README.md")
    s = io.open(p, encoding="utf-8").read()
    vistos, fallos = [], []

    def rehaz(m):
        clave = m.group(1)
        if clave not in BLOQUES:
            fallos.append("bloque desconocido en el README: %s" % clave)
            return m.group(0)
        txt, err = BLOQUES[clave]()
        if err:
            fallos.append("%s: %s" % (clave, err))
            return m.group(0)
        vistos.append(clave)
        r = ultima(etiqueta="RIVALES: corpus de codigo contra el campo", config="max") or {}
        return ("<!-- MEDIDO: %s -->\n%s\n<!-- /MEDIDO · bench/readme.py desde "
                "bench/historia.jsonl · árbol %s · %s -->"
                % (clave, txt, r.get("arbol_sha", "?"), (r.get("t") or "?")[:10]))

    nuevo = MARCA.sub(rehaz, s)
    sin_marcar = [k for k in BLOQUES if k not in vistos]
    for k in sin_marcar:
        fallos.append("el bloque '%s' no esta marcado en README.md: envuelvelo con "
                      "<!-- MEDIDO: %s --> ... <!-- /MEDIDO -->" % (k, k))

    print("bloques reescritos: %s" % (", ".join(vistos) or "ninguno"))
    for f in fallos:
        print("   ✗ %s" % f, file=sys.stderr)
    if seco:
        print("(--seco: no se ha tocado el README)")
        return 1 if fallos else 0
    if nuevo != s:
        io.open(p, "w", encoding="utf-8").write(nuevo)
        print("README.md reescrito")
    return 1 if fallos else 0


if __name__ == "__main__":
    sys.exit(main())
