#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""EL BANCO DE PRUEBAS DE PREFLATE PROPIO, y va ANTES que una sola linea del codigo.

La leccion de esta semana es que una prueba escrita despues mide lo que el codigo hace, no lo
que deberia hacer. Aqui el contrato es duro y se puede escribir sin haber empezado: dado un
flujo DEFLATE real, hay que poder devolver EXACTAMENTE los mismos bytes. Ni uno mas ni uno menos.

Se sacan flujos DEFLATE de verdad de los PNG, ZIP y GZIP del corpus —no sinteticos: los
sinteticos los genera zlib con los ajustes por defecto y el problema entero es que ahi fuera hay
media docena de codificadores con ajustes distintos—.

Para cada flujo se guarda:
    .raw    los bytes DEFLATE tal cual salen del fichero
    .plano  lo que sale al descomprimirlo (la verdad contra la que comparar)
Asi cualquier fase del proyecto se puede validar contra el mismo banco.
"""
import zlib, os, struct, sys, hashlib

CORPUS = "/tmp/compresor-f1/corpus"
SALIDA = "/tmp/deflate-banco"
TOPE = 3000

def png_idat(d):
    """Los IDAT de un PNG concatenados son UN flujo zlib."""
    if d[:8] != b"\x89PNG\r\n\x1a\n": return None
    i, trozos = 8, []
    while i + 8 <= len(d):
        ln = struct.unpack(">I", d[i:i+4])[0]; tipo = d[i+4:i+8]
        if tipo == b"IDAT": trozos.append(d[i+8:i+8+ln])
        if tipo == b"IEND": break
        i += 12 + ln
    return b"".join(trozos) or None

def gzip_cuerpo(d):
    if d[:2] != b"\x1f\x8b": return None
    i = 10
    fl = d[3]
    if fl & 4: i += 2 + struct.unpack("<H", d[i:i+2])[0]
    if fl & 8:
        while i < len(d) and d[i]: i += 1
        i += 1
    if fl & 16:
        while i < len(d) and d[i]: i += 1
        i += 1
    if fl & 2: i += 2
    return d[i:]

os.makedirs(SALIDA, exist_ok=True)
n = 0
tipos = {}
for raiz, _, fich in os.walk(CORPUS):
    for f in fich:
        if n >= TOPE: break
        p = os.path.join(raiz, f)
        try: d = open(p, "rb").read()
        except OSError: continue
        if len(d) < 64 or len(d) > 8 << 20: continue
        crudo, tipo = None, None
        if d[:8] == b"\x89PNG\r\n\x1a\n":
            crudo, tipo = png_idat(d), "png"
            if crudo: crudo = crudo[2:-4] if len(crudo) > 6 else None  # quitar cabecera y adler de zlib
        elif d[:2] == b"\x1f\x8b":
            crudo, tipo = gzip_cuerpo(d), "gz"
        if not crudo: continue
        try:
            plano = zlib.decompressobj(-15).decompress(crudo)
        except zlib.error:
            continue
        if not plano: continue
        h = hashlib.blake2b(crudo, digest_size=8).hexdigest()
        open(f"{SALIDA}/{tipo}-{h}.raw", "wb").write(crudo)
        open(f"{SALIDA}/{tipo}-{h}.plano", "wb").write(plano)
        tipos[tipo] = tipos.get(tipo, 0) + 1
        n += 1
    if n >= TOPE: break

print(f"banco de flujos DEFLATE reales: {n}")
for t, c in sorted(tipos.items()): print(f"   {t}: {c}")
tot_raw = sum(os.path.getsize(f"{SALIDA}/{x}") for x in os.listdir(SALIDA) if x.endswith(".raw"))
tot_pla = sum(os.path.getsize(f"{SALIDA}/{x}") for x in os.listdir(SALIDA) if x.endswith(".plano"))
print(f"   comprimido: {tot_raw:,} B   ·   plano: {tot_pla:,} B   ·   ratio {100*tot_raw/max(tot_pla,1):.1f}%")
