#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""DE QUE DEPENDEMOS QUE NO ES NUESTRO, y si convendria que lo fuera.

NORMA DEL DUEÑO: «no nos podemos fiar de nada por mas años que tengan de desarrollo. Todo lo que
usemos tenemos que preguntarnos si podemos hacerlo nosotros y si seria mejor hacerlo nosotros».

Esto no es nuevo aqui: F2.7 ya bajo el arbol de 60 crates a 33 escribiendo lo nuestro, y en dos
casos lo propio GANO a lo de fuera con numeros —el troceador contra fastcdc mejora ratio Y
tiempo; el formato de indice empata con bincode perdiendo 955 bytes de 97 MB—. Lo que faltaba
era la tabla entera: que queda prestado, cuanto de nuestra ventaja depende de ello, y que
pasaria si lo escribieramos.

TRES COLUMNAS QUE NO SON OPINION:
  ¿podemos?    si hay evidencia de que sabemos hacerlo (un prototipo, una pieza equivalente ya
               escrita) o si es un problema acotado. No «en teoria todo se puede».
  ¿mejor?      SI / NO / MEDIR. «MEDIR» es una respuesta honesta y es la mayoria.
  riesgo       que perdemos si esa pieza desaparece, se rompe o resulta estar mal.

Y DOS COSAS QUE SALEN DE MEDIRLO, no de opinar:

  1. LA PIEZA QUE MAS TRABAJO HACE ES LA QUE MENOS SUPERFICIE CUESTA. zstd, que hace TODA la
     compresion del modo por defecto, arrastra 3 crates. El cifrado arrastra 33. `fuser`, para un
     `mount` que solo existe en Linux, arrastra 13. La intuicion decia lo contrario.

  2. EL ARBOL HA CRECIDO DE 33 A 59 CRATES SIN QUE NADIE LO NOTARA. F2.7 lo bajo a 33 el 27-07
     escribiendo lo nuestro; hoy son 59. Nada lo vigilaba, asi que crecio. Un numero que se
     mejora una vez y no se vigila vuelve solo a donde estaba.

Y LA NORMA, dicha entera: los años de desarrollo de una pieza de terceros no demuestran que este
bien. Demuestran que su ritmo de defectos nunca llego a cero y que su propio historial de
versiones es la prueba. «Lleva veinte años en produccion» y «lleva veinte años publicando
parches» son la misma frase.
"""
import csv, os

# pieza · que hace · lineas/tamaño · podemos · mejor · estado · riesgo si falla · evidencia
D = [
("zstd 0.13",
 "El compresor del modo POR DEFECTO. Toda la compresión que se vende sale de aquí.",
 "3 crates", "SÍ, demostrado", "MEDIR",
 "prestado",
 "Sin él no hay modo por defecto. Es la pieza más grande y la que más trabajo hace.",
 "vc-cm es nuestro y COMPRIME MÁS que zstd (es el modo --max). Sabemos hacerlo; lo que no sabemos es si conviene, porque vc-cm va a 1 MB/s. El hallazgo de hoy cambia la cuenta: si el archivador va a 5,5 MB/s pase lo que pase, el presupuesto de velocidad del códec es mucho mayor de lo que decía la tabla vieja."),

("preflate-rs 0.7.6",
 "Deshace el DEFLATE de PNG, ZIP y GZIP para recomprimir el contenido plano. Es de donde sale el 39,8% sobre 7-Zip en PNG.",
 "14 crates", "SÍ, acotado", "MEDIR",
 "prestado — EL MÁS ARRIESGADO",
 "Es NUESTRA MAYOR VENTAJA y es una caja pre-1.0 de terceros. Si tiene un fallo, se lo comemos entero; si se abandona, nos quedamos sin el margen mayor de la tabla comparativa.",
 "Reconstruir los parámetros del codificador DEFLATE original es difícil pero está acotado y ya lo hicimos para JPEG (F2.8, 99,45% byte-idénticos). La ablación VC_SIN_PREFLATE mide cuánto compra."),

("libjxl (C++, por FFI)",
 "Transcodifica JPEG a JXL sin pérdida. ~16% mejor sobre .jpg.",
 "FFI a C++", "SÍ, ya empezado", "SÍ, probablemente",
 "REEMPLAZO EN MARCHA (F2.8)",
 "Es la dependencia más gorda del binario y la única que no es Rust. Complica la construcción en Mac (JXL_LIB_DIR, pkg-config, Homebrew).",
 "Nuestro parser propio ya reemite 99,45% de 10.642 JPEG reales byte a byte, con 0 divergencias, a 28,0 MB/s contra los 6,4 del carril libjxl. Cuatro veces más rápido y sin la dependencia."),

("blake3 1.x",
 "La huella de cada trozo. Es lo que garantiza que lo que sale es lo que entró.",
 "6 crates", "SÍ, pero", "NO",
 "prestado, y bien",
 "Un fallo aquí sería catastrófico y silencioso: archivos que 'verifican' y están mal.",
 "Escribir un hash criptográfico propio es de las pocas cosas donde hacerlo nosotros es MEDIBLEMENTE peor: blake3 lleva años de criptoanálisis público y su implementación SIMD es de las más optimizadas que existen. Aquí la regla se cumple mirándola y la respuesta es no."),

("chacha20poly1305 + argon2",
 "El cifrado y la derivación de clave.",
 "33 crates", "SÍ, pero", "NO",
 "prestado, y bien",
 "Un canal lateral por tiempos en un cifrado propio pierde los datos del cliente sin que nadie se entere hasta que es tarde.",
 "Misma respuesta que blake3 y por la misma razón, no por dogma: en criptografía el fallo no se ve en las pruebas. Nuestra ida y vuelta pasaría igual con un cifrado roto."),

("preflate + zstd + libjxl juntos",
 "Cuánto de nuestra ventaja es prestada.",
 "—", "—", "MEDIR",
 "LA PREGUNTA GORDA",
 "Si la mayor parte del margen sobre 7-Zip viene de preflate y de libjxl, entonces lo que vendemos es un envoltorio muy bueno alrededor del trabajo de otros.",
 "La ablación F3.2 (VC_SIN_PREFLATE, --no-jpeg) da el número exacto: cuántos bytes compra cada pieza prestada. Está encolada."),

("libc 0.2",
 "Las llamadas al sistema: stat, xattr, flock, mknod.",
 "1 crate", "SÍ, trivial", "NO",
 "prestado, sin riesgo",
 "Ninguno: son declaraciones de tipos, no código. Reescribirlas sería copiar las cabeceras del sistema a mano.",
 "No hay nada que ganar."),

("anyhow 1",
 "Ergonomía de errores.",
 "1 crate", "SÍ, trivial", "MEDIR",
 "prestado, sin riesgo",
 "Ninguno funcional. Es azúcar.",
 "Se quita en una tarde y bajaría el árbol de dependencias. No corre en ningún camino caliente, así que no cambia una cifra."),

("fuser (sólo Linux)",
 "El `mount` para navegar dentro de un archivo.",
 "13 crates", "SÍ", "SÍ — quitarlo del camino por defecto",
 "prestado, y CARO para lo que da",
 "Sólo afecta a `mount`, que además es sólo Linux. Pero mete 13 crates en el binario de todo el mundo por una función que casi nadie usa.",
 "Ya está aislado por cfg(target_os=\"linux\"). Ponerlo detrás de una característica opcional apagada por defecto quita 13 crates del árbol sin perder nada: quien quiera montar, compila con ella."),

("EL ÁRBOL ENTERO",
 "Código que no hemos escrito y que corre dentro de nuestro binario.",
 "59 crates", "—", "MEDIR",
 "CRECIÓ DE 33 A 59 SIN VIGILANCIA",
 "Cada crate es código que no podemos auditar y que puede fallar o abandonarse. 59 es la superficie real.",
 "F2.7 lo bajó a 33 el 27-07 escribiendo lo nuestro. Hoy son 59, sobre todo por el cifrado (33) y preflate (14). Nada lo vigilaba. Va a la aguja: si sube en un commit, se ve."),
]

os.makedirs("bench", exist_ok=True)
with open("bench/dependencias.tsv", "w", newline="") as f:
    f.write("# DE QUE DEPENDEMOS QUE NO ES NUESTRO, y si convendria que lo fuera.\n")
    f.write("# Norma: no fiarse de nada por muchos años de desarrollo que tenga; preguntarse\n")
    f.write("# siempre si podemos hacerlo nosotros y si seria MEJOR hacerlo nosotros.\n")
    f.write("# Lo escribe bench/dependencias.py. La columna «mejor» admite MEDIR, que es la\n")
    f.write("# respuesta honesta cuando no hay numero — y es la mayoria.\n")
    w = csv.writer(f, delimiter="\t")
    w.writerow(["pieza", "que_hace", "tamano", "podemos", "mejor", "estado", "riesgo", "evidencia"])
    for r in D:
        w.writerow(r)

print(f"bench/dependencias.tsv: {len(D)} piezas")
from collections import Counter
print("  ¿podemos?:", dict(Counter(r[3] for r in D)))
print("  ¿mejor?:  ", dict(Counter(r[4] for r in D)))
print()
for r in D:
    print(f"  {r[0]:<28} podemos={r[3]:<16} mejor={r[4]:<18} {r[5]}")
