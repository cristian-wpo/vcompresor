# -*- coding: utf-8 -*-
"""LO QUE COMPARTEN EL PANEL Y EL QUE DECIDE. Cuatro primitivas y ninguna mas.

POR QUE EXISTE. bench/panel.py compara el MISMO experimento entre DOS COMMITS; bench/decide.py
compara DOS CONFIGURACIONES en el MISMO commit. Son dos ejes distintos y no se pueden meter en la
misma funcion sin mentir. Pero las reglas de que cuenta como dato y que cuenta como mejora tienen
que ser LAS MISMAS en los dos, porque si no acabas con dos cifras distintas del mismo experimento
segun quien la calcule — que es exactamente el fallo que este trabajo viene a cerrar.

Asi que aqui vive lo comun y solo lo comun. Si algo lo usa uno solo, no entra.
"""
import re

# Un experimento medido varias veces lleva «(vuelta 2)» pegado a la etiqueta o a la config. Es la
# MISMA cosa medida otra vez, no otra cosa: para emparejar hay que quitarlo antes de comparar.
VUELTA = re.compile(r"\s*\(vuelta\s*\d+\)\s*$")


def sin_vuelta(s):
    return VUELTA.sub("", (s or "").strip())


def valida(f):
    """UNA MEDIDA QUE FALLO NO ES UN DATO, Y SE ESTABA DIBUJANDO COMO SI LO FUERA.

    `mide.sh` guarda TAMBIEN las ejecuciones que revientan -y hace bien: saber que algo fallo el
    martes vale- pero salen con `rc` distinto de 0, cero bytes y cero segundos. El panel las
    metia en la linea de tiempo como puntos normales. Visto el 03-08 al equivocarme en un guion
    de ablacion: dos filas con rc=1 y rc=255 iban a dibujar el archivador a 0 bytes, o sea el
    mejor resultado de la historia.

    Tambien caen las que salieron bien pero no midieron nada: sin bytes de entrada no hay
    experimento, solo una linea.
    """
    if f.get("rc", 0) != 0:
        return False
    if not f.get("bytes_in") and not f.get("bytes_out"):
        return False
    return True


# Dos medidas de la misma muestra tienen que traer la MISMA entrada. Para el reloj se admite un 2%
# -un arbol vivo cambia de tamano entre dos tandas-, pero para los BYTES no se admite nada: son
# deterministas, y comparar 10.485.760 contra 10.485.759 no es medir, es restar dos cosas
# distintas. Cambiar la etiqueta no basta para declarar otra muestra; lo que manda es el tamano.
HOLGURA_ENTRADA = 0.02


def misma_entrada(a, b, exacta=False):
    x, y = a.get("bytes_in") or 0, b.get("bytes_in") or 0
    if not x or not y:
        return False
    if exacta:
        return x == y
    return abs(x - y) / max(x, y) <= HOLGURA_ENTRADA


def arbol_de_commit(sh, commit):
    """El digesto de crates/ + Cargo.* de UN commit, sin abrir un solo fichero.

    Es EXACTAMENTE el mismo numero que bench/mide.sh sella como `arbol_sha` al medir, y por eso
    los dos lados se pueden casar: dos filas con el mismo digesto midieron el mismo codigo, lo
    hubiera commiteado alguien o no. Hace falta porque aqui se mide primero y se commitea despues
    con la cifra en el mensaje, asi que `mide.sh` sella el HEAD ANTERIOR y contar por `commit`
    deja vacio justo el commit que trae el cambio.

    🔑 ESTE CALCULO VIVIA COPIADO EN bench/panel.py Y EN bench/aplicado.py, y el comentario del
    panel ya avisaba: «si los dos divergieran, la tabla y la curva contarian cosas distintas».
    Estaba a punto de haber una tercera copia. Vive aqui y lo importan los tres.

    Se usa `git hash-object` -o sea el identificador que git le da al contenido- y no un sha256
    del fichero, para que el lado del commit pueda sacarlo de `git ls-tree` sin leer nada.
    """
    import hashlib
    lin = []
    for l in sh("git ls-tree -r %s -- crates Cargo.toml Cargo.lock" % commit).splitlines():
        if "\t" not in l:
            continue
        meta, ruta = l.split("\t", 1)
        pp = meta.split()
        if len(pp) >= 3:
            lin.append("%s %s" % (ruta, pp[2]))
    if not lin:
        return None
    lin.sort()
    return hashlib.sha256("".join(x + "\n" for x in lin).encode("utf-8")).hexdigest()[:12]


def carga_libro(ruta):
    """El libro mayor, ya filtrado. Devuelve (filas_validas, descartadas, corruptas).

    ⚠️ LAS DESCARTADAS SE CUENTAN, NO SE TIRAN EN SILENCIO. El panel llevaba un `except
    json.JSONDecodeError: pass` que hacia desaparecer una linea mala sin dejar rastro: si un
    volcado se corta a la mitad, el numero cambia y nadie se entera. Contarlas cuesta un entero.
    """
    import json
    ok, malas, corruptas = [], 0, 0
    try:
        for l in open(ruta, encoding="utf-8"):
            l = l.strip()
            if not l:
                continue
            try:
                f = json.loads(l)
            except json.JSONDecodeError:
                corruptas += 1
                continue
            if valida(f):
                ok.append(f)
            else:
                malas += 1
    except OSError:
        pass
    return ok, malas, corruptas
