#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""TODO LO DE FUERA QUE USAMOS O HEMOS USADO, Y CON QUE LO HEMOS SUSTITUIDO.

Encargo del dueño: «en el panel quiero ver tambien todas las herramientas de terceros que usamos
o hemos usado, y tambien quiero ver las alternativas que hemos creado nosotros».

LAS QUE YA NO ESTAN SON LA PARTE IMPORTANTE. La tabla de dependencias dice de que dependemos
HOY; esta dice ademas de que dependiamos y dejamos de depender, y con que resultado. Sin esa
columna la norma del dueño —preguntarse siempre si podemos hacerlo nosotros— parece una
intencion. Con ella es un historial: seis piezas sustituidas y en dos de ellas lo nuestro GANA
con numeros.

ESTADOS:
    REEMPLAZADA   ya no esta en el arbol. Lo nuestro hace su trabajo.
    EN OBRA       existe la nuestra y funciona, pero todavia no ha sustituido a la de fuera.
    CONVIVEN      las dos en el binario, cada una en su sitio, y es una decision consciente.
    SE QUEDA      la de fuera, y hacerlo nosotros seria MEDIBLEMENTE peor.
    A AISLAR      se queda pero no deberia estar en el camino de todo el mundo.
"""
import csv

# de fuera · que hacia · nuestra · estado · resultado medido · por que
T = [
("fastcdc", "Trocear el flujo por contenido", "vc-cdc", "REEMPLAZADA",
 "22,97% de ratio contra 25,95%, y 18,7 s contra 33,0. Mejor en las DOS cosas a la vez.",
 "bench/f2.7-deps.tsv · corpus dup de 377 MB y 2.124 ficheros"),

("bincode + serde", "Serializar el índice del archivo", "vc-formato", "REEMPLAZADA",
 "96.992.182 B contra 96.991.227: 955 bytes peor DE 97 MB (0,001%) y quita dos dependencias.",
 "bench/f2.7-deps.tsv · mismo árbol de 1.200 jpg + 800 de código"),

("clap", "Parsear los argumentos de la línea de comandos", "parseo a mano en vc-cli", "REEMPLAZADA",
 "El árbol bajó de 60 crates a 47 sólo con esto. Son seis subcomandos: no justificaban 13 cajas.",
 "bench/f2.7-deps.tsv"),

("walkdir", "Recorrer el árbol de directorios", "recorrer() en vc-core", "REEMPLAZADA",
 "Parte del salto de 47 a 43 crates. Un recorrido recursivo con readdir son treinta líneas.",
 "bench/f2.7-deps.tsv"),

("filetime", "Leer y escribir las fechas de los ficheros", "libc directo", "REEMPLAZADA",
 "Parte del salto de 47 a 43 crates. Son dos llamadas al sistema.",
 "bench/f2.7-deps.tsv"),

("flate2", "Comprimir y descomprimir DEFLATE", "deflate.rs + inflate.rs", "REEMPLAZADA",
 "Parte del salto de 43 a 33 crates.",
 "bench/f2.7-deps.tsv"),

("libjxl (C++)", "Transcodificar JPEG a JXL sin pérdida", "vc-jpeg", "EN OBRA",
 "99,45% byte-idénticos sobre 10.642 JPEG reales, 0 divergencias, a 28,0 MB/s contra los 6,4 del carril libjxl. CUATRO VECES MÁS RÁPIDO.",
 "bench/f2.7-deps.tsv · 10.642 JPEG reales del corpus"),

("preflate-rs 0.7.6", "Deshacer el DEFLATE de PNG/ZIP/GZIP para recomprimirlo", "vc-inflate", "EN OBRA",
 "Fase 1 de 4 al 100%: 1.973 de 1.973 flujos DEFLATE reales salen IDÉNTICOS, a 285,0 MB/s. Faltan recomprimir, estimar y predecir.",
 "bench/saca-deflate.py · 1.973 flujos reales de los PNG del corpus"),

("zstd (58.395 líneas de C)", "Comprimir en el modo por defecto", "vc-cm", "CONVIVEN",
 "vc-cm comprime MÁS que zstd-19 (es todo el modo --max) pero va a ~1 MB/s. Hoy se sabe que comprimir es el 72% de la CPU y sólo el 27% del reloj, así que el presupuesto de un códec propio es mucho mayor de lo que se creía.",
 "bench/ablacion.tsv · bench/piramide.tsv"),

("blake3", "La huella que garantiza que lo que sale es lo que entró", "—", "SE QUEDA",
 "Hacerlo nosotros sería MEDIBLEMENTE peor: años de criptoanálisis público y una implementación SIMD de las más optimizadas que existen. Aquí la regla se cumple mirándola y la respuesta es no.",
 "6 crates de árbol"),

("chacha20poly1305 + argon2", "Cifrar y derivar la clave", "—", "SE QUEDA",
 "Mismo motivo que blake3, y no por dogma: en criptografía el fallo NO SE VE en las pruebas. Nuestra ida y vuelta pasaría igual con un cifrado roto. 33 crates es caro, pero equivocarse aquí pierde los datos del cliente.",
 "33 crates de árbol — la dependencia más gorda"),

("libc", "Las llamadas al sistema", "—", "SE QUEDA",
 "Son declaraciones de tipos, no código. Reescribirlas sería copiar a mano las cabeceras del sistema. 1 crate.",
 "1 crate"),

("anyhow", "Ergonomía de los errores", "—", "SE QUEDA",
 "Azúcar. Se quita en una tarde y no cambia ninguna cifra porque no corre en ningún camino caliente. 1 crate.",
 "1 crate"),

("fuser", "El `mount` para navegar dentro de un archivo", "—", "A AISLAR",
 "13 crates en el binario de TODO EL MUNDO por una función que sólo existe en Linux y que casi nadie usa. Detrás de una característica apagada por defecto, se van los 13 sin perder nada.",
 "13 crates"),
]

with open("bench/terceros.tsv", "w", newline="") as f:
    f.write("# TODO LO DE FUERA QUE USAMOS O HEMOS USADO, y con que lo hemos sustituido.\n")
    f.write("# LAS QUE YA NO ESTAN SON LA PARTE IMPORTANTE: sin esa columna, «preguntarse siempre\n")
    f.write("# si podemos hacerlo nosotros» parece una intencion. Con ella es un historial.\n")
    f.write("# Lo escribe bench/terceros.py.\n")
    w = csv.writer(f, delimiter="\t")
    w.writerow(["de_fuera", "que_hacia", "la_nuestra", "estado", "resultado", "banco"])
    for r in T:
        w.writerow(r)

from collections import Counter
c = Counter(r[3] for r in T)
print(f"bench/terceros.tsv: {len(T)} piezas de fuera\n")
for e in ["REEMPLAZADA", "EN OBRA", "CONVIVEN", "SE QUEDA", "A AISLAR"]:
    if c[e]:
        print(f"  {e:<14} {c[e]}")
        for r in T:
            if r[3] == e:
                print(f"      {r[0]:<28} -> {r[2]}")
print()
print(f"  Historial: {c['REEMPLAZADA']} sustituidas, {c['EN OBRA']} en obra.")
print("  Y en dos de las sustituidas lo nuestro GANA con numeros, no empata.")
