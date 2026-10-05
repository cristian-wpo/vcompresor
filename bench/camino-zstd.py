# -*- coding: utf-8 -*-
"""EL CAMINO APLICADO A LAS TECNICAS DE zstd: que tienen, que tenemos, y que se coge.

═══════════════════════════════════════════════════════════════════════════════════════════════
LA REGLA QUE ESTABLECE ESTE FICHERO (dueño, 01-08-2026):

  «Pon como regla no tener sesgo para codigos de terceros y aprovecharlo.
   Tambien viendo como lo hacen ellos lo mejoramos.»

Y viene de una observacion suya que era CIERTA: «siento que tienes algun tipo de sesgo para no
revisar codigos de terceros». Lo era. El patron, mirandolo:
  · con DEFLATE se empezo por la licencia en vez de por el codigo
  · con zstd se empezo por «¿es su premisa la nuestra?» antes de abrir un solo fichero
  · cuando pidio un camino, se escribio un instrumento de INTROSPECCION -camino-vlz-lento- en
    vez de abrir zstd_lazy.c
  · y hubo que insistir tres veces para que se leyera el bucle

LO QUE COSTO ESE SESGO, en numeros de esta misma sesion:
  · la ventana de vlz llevaba 4 MiB clavados con un comentario que decia «hay que medirla»
  · la longitud topaba en 515 porque nadie miro como lo resuelve todo el mundo
  · vlz estaba en «greedy» y llevabamos la sesion comparandolo contra «lazy2» sin saberlo
  · la criba de candidatos se escribio de UN byte cuando la suya es de CUATRO al mismo coste
Cuatro cosas que estaban a la vista en 745-2.199 lineas de codigo abierto.

LA REGLA, entonces, en tres partes:
  1. LEER EL CODIGO DE FUERA ES LA PRIMERA PARADA, no la ultima. Antes de diseñar nada que
     alguien ya haya resuelto, se abre su fichero.
  2. LA LICENCIA SE COMPRUEBA, NO SE TEME. zstd es BSD-3 o GPLv2 y elegimos BSD: permisiva,
     compatible con nuestra AGPL y con la comercial. Comprobarlo son treinta segundos.
  3. Y LA PARTE QUE HACE QUE NO SEA COPIAR: se coge la ESTRUCTURA y se re-mide la CONSTANTE.
     Sus numeros estan afinados para SU codificador. La prueba de que esto no es teoria: pasar
     TODA la longitud a logaritmo -que es literalmente lo que ellos hacen- PIERDE un 1,0% en sql;
     con un escape gana en los cuatro tipos. Mirandolos aprendimos algo que ellos no hacen.
═══════════════════════════════════════════════════════════════════════════════════════════════

Fuente leida: zstd 1.5.7 vendorizado en zstd-sys-2.0.16, BSD-3-Clause (Meta Platforms).
Todo lo medido va sobre 84 MB de codigo+csvjson+logs concatenados, salvo donde se diga otra cosa.

Lo escribe bench/camino-zstd.py.
"""
import csv, collections

# tecnica · donde en su codigo · que hace · ¿la tenemos? · veredicto · lo medido
T = [
("cuenta de 8 en 8", "zstd_internal.h · ZSTD_count",
 "Comparar los bytes iguales con cargas de u64 y trailing_zeros en vez de byte a byte.",
 "NO la teniamos", "COGIDA",
 "MISMOS BYTES EXACTOS y +44% de reloj a esfuerzo 64 (8,2 -> 11,8 MB/s). No es un atajo que "
 "aproxime: es la misma cuenta con menos instrucciones."),

("criba de 4 bytes", "zstd_lazy.c:715",
 "Antes de comparar un candidato entero, mirar los CUATRO bytes que terminan en la longitud ya "
 "encontrada: MEM_read32(match+ml-3) == MEM_read32(ip+ml-3). Si no cuadran, ese candidato no "
 "puede ganar.",
 "teniamos una de UN byte", "COGIDA — y era mejor que la nuestra",
 "El mismo coste -una carga, una comparacion- y un filtro CUATRO veces mas fuerte. La nuestra la "
 "escribimos nosotros el mismo dia y era la version pobre de la suya."),

("salto creciente", "zstd_lazy.c:1614",
 "Cuando no se encuentra nada, avanzar MAS DE UN BYTE, y mas cuanto mas lleves en seco: "
 "step = ((ip-anchor) >> kSearchStrength) + 1. «jump faster over incompressible sections».",
 "NO — avanzabamos de uno en uno siempre", "COGIDA",
 "Y su comentario decia donde luce, que es donde yo NO estaba mirando. En codigo/csv/logs da "
 "+9%. En dato incompresible: VIDEO +30,8% de reloj por +1,53% de bytes · PNG +18,2% por "
 "+0,068% · JPG +10,0% por +0,014% · BIN +0%. Elegi mal el corpus antes de leer su comentario."),

("lazySkipping", "zstd_lazy.c:1623 · kLazySkippingStep 8",
 "Cuando el salto pasa de 8, dejar TAMBIEN de indexar las posiciones que se saltan.",
 "NO", "COGIDA con el salto",
 "Va dentro del salto creciente: solo se indexa la posicion que se ha buscado."),

("lazy2 (mirar a +1 y +2)", "clevels.h:37 · ZSTD_lazy2 en el nivel 9",
 "Antes de aceptar una coincidencia, preguntar si una posicion o dos mas alla hay otra MEJOR. "
 "Si la hay, emitir un literal y esperar.",
 "NO — estabamos en «greedy»", "COGIDA — LA MAS GRANDE DE LA SESION",
 "Su escala: fast=1, dfast=2, greedy=3, lazy=4, lazy2=5. Llevabamos la sesion comparando nuestro "
 "greedy contra su lazy2 sin saberlo. Al aplicarlo: codigo -4,6% · sql -9,7% · logs -14,0% · "
 "csvjson -7,7%. Monotono. Son treinta lineas."),

("longitud con codigo + bits extra", "zstd_internal.h:103 · MaxML 52, MaxMLBits 16",
 "La longitud no va en un campo plano: va como codigo pequeño mas bits extra, y por eso llega a "
 "131.074 sin gastar en las cortas.",
 "NO — arbol plano de 9 bits, tope 515", "COGIDA Y MEJORADA",
 "AQUI ES DONDE MIRARLOS NOS ENSEÑO ALGO QUE ELLOS NO HACEN. Copiado tal cual -toda la longitud "
 "a logaritmo- gana en csvjson (-9,2%) y logs (-4,6%) pero PIERDE en sql (+1,0%): ahi las "
 "longitudes son cortas y los bits crudos van sin modelar. Con un ESCAPE -arbol plano en 0..510, "
 "511 dice «viene un logaritmo»- gana en los CUATRO. Ellos no pueden hacerlo asi porque su "
 "codificador es FSE; el nuestro es aritmetico adaptativo y modela cada simbolo."),

("repcodes", "zstd_internal.h:64 · ZSTD_REP_NUM 3 · zstd_lazy.c «check immediate repcode»",
 "Guardar los TRES desplazamientos mas recientes y, tras cada copia, comprobar si lo que sigue "
 "repite uno: si repite, se codifica casi gratis en vez de mandar la distancia entera. Y se "
 "intercambian al usarlos, como una lista de recien usados.",
 "NO — mandamos la distancia completa SIEMPRE", "PENDIENTE — la mayor que queda",
 "SIN MEDIR. En dato con estructura -columnas, registros, logs- el mismo desplazamiento se "
 "repite constantemente y hoy lo pagamos entero cada vez. Ademas es rapido: comprobar un repcode "
 "es UNA comparacion de u32, sin recorrer la cadena."),

("indice esparcido de largo alcance", "zstd_ldm.c · LDM_MIN_MATCH_LENGTH 64 · LDM_HASH_RLOG 7 · "
 "LDM_BUCKET_SIZE_LOG 4",
 "Un segundo indice que muestrea 1 de cada 128 posiciones POR CONTENIDO -hash rodante gear- y "
 "solo acepta coincidencias de 64 bytes o mas.",
 "se implemento con NUESTRO gear", "PROBADA Y NO ENVIADA, con su numero",
 "-0,168% a esfuerzo 6 y -0,007% a 64, y eso con la entrada a 2,5 veces el tope de ventana, que "
 "es donde deberia lucir. A cambio de una pasada entera y 4 MB. ⚠️ PERO LO REFUTADO ES MI "
 "VERSION: la mia sondea UNA ranura y la suya usa cubos de 16. Quien lo retome, por ahi."),

("targetLength", "clevels.h:37 · el 16 del nivel 9",
 "Cuando la coincidencia ya es bastante buena, dejar de buscar aunque queden candidatos.",
 "NO", "ES PERILLA, NO ARREGLO",
 "MEDIDO: cortar en 64 mas no indexar los interiores da +34% de reloj y cuesta +4,72% de bytes, "
 "y eso nos devuelve por detras de zstd -9. Si entra, con interruptor y no en el defecto."),

("hash doble (dfast)", "zstd_double_fast.c · 778 lineas",
 "Dos tablas a la vez, una de 4 bytes y otra de 8: la larga engancha menos y casi nunca miente, "
 "la corta engancha siempre.",
 "NO — una sola tabla de 4 bytes", "SIN MIRAR",
 "SIN MEDIR. vc-cm ya tiene esta idea dentro -match-6 y match-12- asi que sabemos que funciona "
 "en nuestro dato; en vlz no se ha probado."),

("analisis optimo (btopt)", "zstd_opt.c · 1.580 lineas",
 "En vez de decidir codicioso o perezoso, calcular el camino de coste minimo sobre el bloque.",
 "NO", "SIN MIRAR — y es el mayor salto de ratio que les queda",
 "SIN MEDIR. Es lo que separa su nivel 12 de su nivel 13 (btlazy2) y de sus niveles 16+."),

("buscador por filas con SIMD", "zstd_lazy.c · search_rowHash",
 "Una tabla organizada en filas para comparar varias etiquetas de golpe con instrucciones "
 "vectoriales.",
 "NO", "SIN MIRAR",
 "SIN MEDIR."),

("FSE + Huffman para la entropia", "fse_compress.c + huf_compress.c · 2.500 lineas",
 "Codificar simbolos con tablas transmitidas en vez de con un modelo adaptativo.",
 "NO, y a proposito", "RECHAZADA POR PREMISA — con su medida al lado",
 "ESTA ES LA UNICA QUE SE RECHAZA SIN PROBARLA, y no por prejuicio: porque su premisa esta "
 "medida y no es la nuestra. zstd usa FSE+Huffman porque necesita DESCOMPRIMIR a gigabytes por "
 "segundo. Nuestra medida del 31-07 con vc-zstd-perfil: barriendo sus nueve estrategias a nivel "
 "19 el reloj va de 0,049 s a 7,3 s -144 veces- y lo unico que cambia es cuanto se esfuerza el "
 "BUSCADOR: codificar la entropia es menos del 1% del tiempo. Nosotros podemos gastar en un "
 "codificador que comprime mejor y no notarlo."),
]

with open("bench/camino-zstd.tsv", "w", newline="") as f:
    f.write("# EL CAMINO APLICADO A LAS TECNICAS DE zstd 1.5.7 (BSD-3-Clause, Meta Platforms).\n")
    f.write("# LA REGLA (dueño, 01-08): leer el codigo de fuera es la PRIMERA parada, no la ultima.\n")
    f.write("# Se coge la ESTRUCTURA y se re-mide la CONSTANTE — sus numeros estan afinados para SU\n")
    f.write("# codificador. Y solo se rechaza por premisa si la premisa esta MEDIDA.\n")
    w = csv.writer(f, delimiter="\t")
    w.writerow(["tecnica", "donde_en_su_codigo", "que_hace", "la_teniamos", "veredicto", "lo_medido"])
    w.writerows(T)

c = collections.Counter(x[4].split("—")[0].strip().split(",")[0] for x in T)
print(f"\nbench/camino-zstd.tsv: {len(T)} tecnicas de zstd pasadas por el camino\n")
for x in T:
    print(f"  [{x[4][:38]:<38}] {x[0]}")
print()
for k, v in c.most_common():
    print(f"  {v} {k}")
print()
print("  LA MAYOR QUE QUEDA: los REPCODES. Guardan tres desplazamientos recientes y tras cada")
print("  copia comprueban si el siguiente trozo repite uno; si repite, casi gratis. Nosotros")
print("  mandamos la distancia entera SIEMPRE. En dato con estructura eso se repite constantemente,")
print("  y ademas comprobarlo es UNA comparacion de u32 sin recorrer la cadena: ratio Y reloj.")
print()
print("  Y LA QUE DEMUESTRA QUE ESTO NO ES COPIAR: la longitud. Copiada tal cual PIERDE en sql;")
print("  con un escape que ellos no pueden usar -su codificador es FSE, el nuestro aritmetico")
print("  adaptativo- gana en los cuatro tipos.")
