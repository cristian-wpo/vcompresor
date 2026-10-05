#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""LAS HERRAMIENTAS NUESTRAS, cada una con su ficha: que sustituye, contra que compite, que se
ha medido y que falta.

Encargo del dueño: «cada sub panel con su aguja, su piramide y todos sus datos, para mejorar
campo a campo, dato a dato, cada una de las herramientas que hagamos nuestras».

LA IDEA. El panel general dice como va EL PRODUCTO. Pero el producto son seis o siete piezas y
cada una es un proyecto con su propia competencia fuera, su propio banco y sus propias preguntas
abiertas. Meterlas todas en una sola vista promedia lo que hay que separar: `vc-cdc` gana a
fastcdc en las dos cosas y `vc-inflate` acaba de nacer, y eso no se ve si sus numeros se suman.

CADA HERRAMIENTA LLEVA:
    contra_quien   la pieza de fuera que sustituye o sustituira
    estado         nacida / en obra / completa / la usamos ya
    lo_medido      el numero que hay, con su fichero
    lo_que_falta   la siguiente fase, dicha como prueba que la cerraria
    banco          contra que se valida, y cuantos casos

LA REGLA DE LAS FICHAS: una herramienta sin banco propio NO tiene ficha. Si no hay contra que
medirla, no es una herramienta nuestra: es codigo nuestro, que es otra cosa.
"""
import csv

# id · nombre · que hace · contra quien · estado · lineas · lo medido · banco · lo que falta
H = [
("vc-cdc", "Troceador por contenido",
 "Corta el flujo por el contenido y no por posicion, para que un byte insertado no desplace todo lo demas.",
 "fastcdc (la caja de terceros que se uso primero)",
 "LA USAMOS YA — y gana",
 "en vc-core/cdc.rs",
 "22,97% de ratio contra 25,95% de fastcdc, y 18,7 s contra 33,0. Mejor en las DOS cosas a la vez.",
 "corpus dup: 377.165.690 B, 2.124 ficheros (bench/f2.7-deps.tsv)",
 "El sketch de similitud rueda el gear una SEGUNDA vez sobre los mismos bytes. Los cortes finos y los gruesos se pueden encontrar en UNA pasada comprobando dos mascaras por byte."),

("vc-formato", "Formato de indice y contenedor",
 "Como se guardan las rutas, los trozos, los tramos y el pie del archivo.",
 "bincode + serde",
 "LA USAMOS YA",
 "en vc-core/formato.rs",
 "96.992.182 B contra 96.991.227 de bincode: 955 bytes peor DE 97 MB (0,001%), y quita dos dependencias.",
 "mismo arbol de 1.200 jpg + 800 de codigo (bench/f2.7-deps.tsv)",
 "El indice se reescribe ENTERO en cada instantanea: ~24 MB por instantanea aunque no cambie un byte (bench/instantaneas-indice.tsv). Un indice que referencie al anterior lo quitaria."),

("vc-jpeg", "Parser JPEG propio",
 "Parsea un JPEG y lo vuelve a emitir byte a byte, para poder recomprimir su contenido.",
 "libjxl (150k lineas de C++, por FFI)",
 "EN OBRA — 99,45%",
 "en vc-core/jpeg.rs",
 "99,45% byte-identicos sobre 10.642 JPEG reales, 0 divergencias, a 28,0 MB/s contra los 6,4 del carril libjxl. CUATRO VECES MAS RAPIDO.",
 "10.642 JPEG reales del corpus (bench/f2.7-deps.tsv)",
 "El 0,55% que falla son 59 ficheros, y 58 de ellos son PNG con extension .jpg. De 10.584 JPEG de verdad, 10.583 salen. Falta cerrar el ultimo y cambiar el carril."),

("vc-inflate", "DEFLATE propio — descomprimir",
 "Deshace un flujo DEFLATE (RFC 1951): lo que hay dentro de PNG, ZIP y GZIP.",
 "preflate-rs 0.7.6 (Microsoft, 9.129 lineas, 14 crates)",
 "NACIDA HOY — fase 1 de 4, al 100%",
 "en vc-core/inflate.rs",
 "1.973 de 1.973 flujos reales IDENTICOS (100,00%), 357,5 MiB -> 5.791,3 MiB a 285,0 MB/s.",
 "1.973 flujos DEFLATE reales de los PNG del corpus (bench/saca-deflate.py)",
 "Fases 2-4: recomprimir y anotar la diferencia, estimar los ajustes del compresor original, y predecir sus decisiones para guardar solo correcciones. Motivo: preflate es el 58,5% de nuestra CPU y sostiene nuestro mayor margen."),

("vc-cm", "Codec de mezcla de contextos",
 "Comprime prediciendo el siguiente bit con muchos modelos a la vez y mezclando sus opiniones.",
 "zstd (58.395 lineas de C) y, arriba del todo, paq8px",
 "LA USAMOS YA en --max — A REHACER",
 "1.985 lineas",
 "Comprime MAS que zstd-19 (es todo el modo --max) pero va a ~1 MB/s. Su ablacion tiene un grupo, `sparse`, que EMPEORA el archivo un 0,34%.",
 "cinco muestras de 10 MB + Silesia (bench/ablacion.tsv, bench/codec-bpb.tsv)",
 "ORDEN DEL DUEÑO: rehacerlo. Y hoy hay dato nuevo: comprimir es el 72% de la CPU pero solo el 27% del reloj porque va en diez hilos, asi que el presupuesto de velocidad de un codec propio es MUCHO mayor de lo que decia la tabla vieja."),
]

with open("bench/herramientas.tsv", "w", newline="") as f:
    f.write("# LAS HERRAMIENTAS NUESTRAS. Cada una es un proyecto con su competencia fuera, su\n")
    f.write("# banco propio y sus preguntas abiertas. Meterlas en una sola vista promedia lo que\n")
    f.write("# hay que separar: vc-cdc ya gana a fastcdc y vc-inflate acaba de nacer.\n")
    f.write("# REGLA: una herramienta sin banco propio NO tiene ficha. Si no hay contra que\n")
    f.write("# medirla, no es una herramienta nuestra: es codigo nuestro, que es otra cosa.\n")
    f.write("# Lo escribe bench/herramientas.py.\n")
    w = csv.writer(f, delimiter="\t")
    w.writerow(["id", "nombre", "que_hace", "contra_quien", "estado", "tamano",
                "lo_medido", "banco", "lo_que_falta"])
    for r in H:
        w.writerow(r)

print(f"bench/herramientas.tsv: {len(H)} herramientas nuestras\n")
for r in H:
    print(f"  {r[0]:<12} {r[4]:<28} contra {r[3][:44]}")
print()
print("  Las cinco tienen banco propio. Las que no lo tengan no entran.")
