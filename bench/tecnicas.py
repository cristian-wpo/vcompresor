#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""EL CENSO DE TECNICAS PARA IR MAS RAPIDO, ordenado por LO QUE ATACA cada una.

Encargo del dueño: «aparte del precomputo, ¿que otras tecnicas podemos aplicar, o que huecos de
tecnicas que no existan podemos inventar?».

NO ES UN CATALOGO DE TRUCOS. Cada familia ataca una cosa distinta, y cual sirve depende de
donde este el tiempo — que en este proyecto esta medido. Por eso cada linea lleva: que ataca,
si lo hacemos, cuanto vale segun lo medido, y que haria falta para cerrarla.

LAS CINCO FAMILIAS, de mas profunda a mas superficial:

  1. NO HACER EL TRABAJO      lo que no se ejecuta no cuesta. La unica con techo infinito.
  2. NO REPETIRLO             entre ejecuciones, entre ficheros, entre pasadas.
  3. HACERLO CON MENOS        menos instrucciones para el mismo resultado.
  4. HACERLO A LA VEZ         mas nucleos, o mas carriles dentro de una instruccion.
  5. HACERLO ANTES            mover trabajo al momento de compilar.

La 1 y la 2 tienen techos mucho mas altos que la 3 y la 4, y casi siempre se atacan al reves:
se empieza optimizando el bucle y se acaba descubriendo que el bucle sobraba.

LAS TRES REGLAS DEL PRECOMPUTO —que son un caso de la familia 5— y su cuarta pregunta:
  1. el dominio es pequeño (cabe en cache) · 2. las entradas se repiten
  3. el calculo cuesta MAS que un acceso a cache   <- la que decide
  4. ¿cuanto mueve?   las tres dicen si FUNCIONA, no si MERECE LA PENA
"""
import csv

# familia · tecnica · que ataca · estado · lo medido · que falta
T = [
("1 · no hacerlo", "Saltar los ficheros que no han cambiado desde la instantanea anterior",
 "El trabajo entero de un fichero: leer, inflar, transformar, trocear y hashear.",
 "HECHO 2026-07-31",
 "MEDIDO: reloj 9,8x, CPU 24,3x, memoria 5,07x sobre el arbol de 2,7 GB, con los archivos byte "
 "a byte iguales. Es la mejora mas grande de toda la campaña, y salio de la familia con el "
 "techo mas alto y no de la que mas horas se llevo. La leccion, con numero: el mismo dia se "
 "invirtieron horas en la familia 3 —el bucle de copia, la tabla de 9 bits— para un +48% en una "
 "pieza que ni siquiera esta enchufada al producto.",
 "—"),

("1 · no hacerlo", "No expandir a plano lo que no se va a deduplicar",
 "Los 6 GB de plano que se escriben desde 357 MB de DEFLATE.",
 "SIN EXPLORAR",
 "El plano se produce SIEMPRE, aunque luego ningun trozo dedupique contra nada.",
 "Saber de antemano si va a dedupicar es el problema, y probablemente no se puede. Queda "
 "anotado como idea con pinta de callejon."),

("2 · no repetirlo", "Transcodificar los tokens en vez de expandir y volver a buscar",
 "La BUSQUEDA DE COINCIDENCIAS, que hoy se hace CUATRO VECES.",
 "IDEA NUESTRA, sin explorar",
 "CUATRO INDICES INDEPENDIENTES de «¿donde vi estos bytes antes?» conviven en UNA pasada del archivador, sobre LOS MISMOS BYTES: vc-cm con 5 tablas de posiciones (~72 MB), zstd/vlz con las suyas, vdeflate con cadenas de hash sobre el mismo plano, y vc-cdc con un hash rodante byte a byte. Ninguno se lo dice a los demas. MEDIDO 31-07 lo que cuesta la duplicacion DENTRO de uno de los cuatro: el racimo de 5 modelos match de vc-cm vale 1,8974% en codigo y 4,5512% en sql, y DEJANDO UNO SOLO encendido se recupera el 66% y el 73%. Las cinco tablas no compran cinco cosas: compran una cinco veces. Y de uno en uno cada modelo media entre 0,01% y 0,15% -o sea que la ablacion de uno en uno NO PODIA VERLO: no distingue «inutil» de «redundante»-. El camino de hoy es DEFLATE -> plano (6 GB) -> zstd. Pero el DEFLATE YA TRAE las "
 "coincidencias encontradas: son los tokens que capturamos desde hoy. Quien hizo el PNG ya pago "
 "la busqueda; nosotros la tiramos, expandimos 17x y hacemos que zstd la pague otra vez.",
 "Pasar los tokens de la estructura LZ77 de DEFLATE a la del compresor. No se si funciona; se "
 "que la informacion esta ahi y que nadie lo hace. La prueba que lo abre: medir que fraccion "
 "del tiempo de zstd es buscar coincidencias."),

("2 · no repetirlo", "Cachear los arboles de Huffman fijos",
 "Dos construcciones de arbol por flujo, a partir de arrays constantes.",
 "HECHO 2026-07-30",
 "Eran 3.946 construcciones identicas sobre el banco. Lo señalo el dueño.",
 "—"),

("2 · no repetirlo", "Los codigos canonicos del emisor, una vez por arbol y no por bloque",
 "Un recorrido de 288 simbolos, 3 veces por bloque comprimido.",
 "SIN MEDIR",
 "11.840 bloques comprimidos. Falta saber cuantos comparten arbol: si casi ninguno, no hay "
 "nada que cachear.",
 "Contar arboles repetidos entre bloques."),

("3 · con menos", "Tabla de 9 bits en el decodificador de Huffman",
 "Los 7,70 pasos por simbolo de caminar el arbol bit a bit.",
 "HECHO 2026-07-30",
 "MEDIDO: 279,0 -> 171,8 ciclos por simbolo. Parte del +48% del dia.",
 "—"),

("3 · con menos", "Acumulador de 64 bits en el lector",
 "Las 10,42 llamadas a bit() por simbolo, cada una con su comprobacion de limites.",
 "SIGUIENTE",
 "Una carga de 8 bytes alimenta ~7 simbolos en vez de una carga por bit. Es la continuacion "
 "natural de la tabla.",
 "Escribirlo y comprobar que los 1.973 siguen identicos."),

("3 · con menos", "El bucle de copia por lotes",
 "Los 22,80 bytes por simbolo copiados de uno en uno.",
 "HECHO 2026-07-30",
 "MEDIDO: el 72,1% de los bytes eran distancia 1 —un memset— y el 25,6% copias sin solape. "
 "El 97,7% tenia atajo y todos iban byte a byte.",
 "—"),

("4 · a la vez", "El carril preflate en paralelo",
 "El 69,9% del reloj, que corria en un solo hilo.",
 "HECHO 2026-07-30",
 "try_deflate partido en prepara (pura) + mete (muta). Archivo byte a byte identico. La "
 "ganancia esta ENCOLADA: la maquina lleva rato a carga 91.",
 "Medirla cuando la flota baje."),

("4 · a la vez", "SIMD entre flujos independientes",
 "La serialidad de decodificar un flujo, decodificando varios a la vez.",
 "SIN EXPLORAR",
 "Un flujo es serial por el formato, pero hay 1.973 independientes. Y MEDIDO: repartir flujos "
 "enteros da 100,0% de eficiencia hasta 16 hilos, asi que esto solo aportaria por encima de "
 "~28 hilos o dentro de un nucleo.",
 "Tecnica real y dificil. Baja prioridad mientras el reparto por flujos siga al 100%."),

("5 · antes", "const fn: calcular al compilar",
 "Todo el coste de inicializacion.",
 "HECHO (tabla GEAR)",
 "La forma mas pura: coste CERO en ejecucion, ni siquiera inicializacion perezosa.",
 "Buscar mas candidatos: cualquier tabla que no dependa de la entrada."),

("EL LIMITE", "Cuando precomputar EMPEORA",
 "—",
 "MEDIDO EN CASA",
 "vc-cm esta construido entero sobre tablas precomputadas y su cuello es justo eso: el 83,7% "
 "de su tiempo se va en PREDECIR (43,1%) + ACTUALIZAR (40,6%), las dos fases donde se toca la "
 "tabla de 484 MiB. La regla 1 rota a lo grande. No es que se pierda la ganancia: la tabla se "
 "convierte en el problema. ⚠️PRECISION, porque la cifra se citaba mal en seis sitios: 83,7% NO es «buscando». Ahi dentro va tambien la aritmetica de los 43 modelos. Explicitamente buscar y escribir el cubo son 14,3% (busca_cubo 3,0 + fija_cubos 11,3). Y «no caben en cache» esta INFERIDO del tamaño y del reparto del tiempo: no hay contadores de fallos de cache por funcion en ningun sitio del banco.",
 "Tenerlo delante antes de precomputar nada. Por eso la tabla del decodificador es de 9 bits "
 "y no de 15."),
]

with open("bench/tecnicas.tsv", "w", newline="") as f:
    f.write("# EL CENSO DE TECNICAS PARA IR MAS RAPIDO, por LO QUE ATACA cada una.\n")
    f.write("# 1 no hacerlo · 2 no repetirlo · 3 hacerlo con menos · 4 a la vez · 5 antes\n")
    f.write("# La 1 y la 2 tienen techos mucho mas altos que la 3 y la 4, y casi siempre se\n")
    f.write("# atacan al reves: se empieza optimizando el bucle y se acaba viendo que sobraba.\n")
    f.write("# Lo escribe bench/tecnicas.py.\n")
    w = csv.writer(f, delimiter="\t")
    w.writerow(["familia", "tecnica", "que_ataca", "estado", "lo_medido", "que_falta"])
    w.writerows(T)

import collections
c = collections.Counter(r[3].split()[0] for r in T)
print(f"bench/tecnicas.tsv: {len(T)} tecnicas\n")
fam = None
for r in T:
    if r[0] != fam:
        print(f"\n  ── {r[0]} " + "─" * (58 - len(r[0])))
        fam = r[0]
    print(f"  [{r[3]:<22}] {r[1]}")
print()
print("  " + " · ".join(f"{k}: {v}" for k, v in c.most_common()))
print()
print("  EL HUECO MAS GRANDE ERA de la familia 1 y se cerro el 2026-07-31: no saltabamos los")
print("  ficheros que no habian cambiado. Reloj 9,8x, CPU 24,3x, memoria 5,07x — mas que todo")
print("  lo demas de la campaña junto, y salio de la familia con el techo mas alto, no de la")
print("  que mas horas se llevo. El orden de este censo es el orden en el que hay que mirar.")
