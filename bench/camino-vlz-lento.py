# -*- coding: utf-8 -*-
"""EL CAMINO A «vlz COMPRIME MAS QUE zstd -9 PERO VA MUCHISIMO MAS LENTO».

Lo pide el dueño dos veces: «¿como eliminamos la lentitud?» y «aplica un camino a lo que esta
causando problemas». Se aplica aqui, y lo primero que hace el instrumento es corregir la premisa
de la pregunta — que la habia puesto yo mal.

LA PREMISA ERA FALSA. Se venia diciendo «21 veces mas lento», comparando NUESTRO ajuste de maxima
calidad (esfuerzo 64) contra el ajuste POR DEFECTO de zstd (-9). Comparado a calidad parecida:
    vlz esfuerzo 6   8.094.669 B   71,5 MB/s
    zstd -9          7.740.539 B  286,6 MB/s
Un 4,6% peor en bytes y CUATRO veces mas lento. Sigue siendo mucho, pero no es 21.

Lo escribe bench/camino-vlz-lento.py sobre 84 MB de codigo+csvjson+logs.
"""
import csv, collections

R = [
("P1", "¿Cuantas veces le preguntamos al nucleo lo mismo?", "NO APLICA",
 "`comprime` recibe un &[u8] y devuelve un Vec. No hay una sola llamada al sistema en el bucle.",
 "leido: crates/vlz/src/lib.rs, la funcion entera."),

("P2", "¿Cuantas veces leemos el mismo byte?", "HALLAZGO — ARREGLADO",
 "Cada candidato de la cadena se comparaba DESDE EL PRINCIPIO, aunque no tuviera ninguna "
 "posibilidad de superar a la mejor coincidencia ya encontrada. Basta mirar el byte que esta EN "
 "esa longitud: si no coincide, ese candidato no puede ganar y se descarta con UNA comparacion "
 "en vez de con `ml`.",
 "Aplicado con la comparacion de 8 en 8: de 8,2 a 13,7 MB/s a esfuerzo 64, +67%, y con los "
 "bytes IDENTICOS o mejores -la criba desempata hacia la coincidencia mas cercana, que se "
 "codifica mas barata: 7.433.960 -> 7.433.874-."),

("P3", "¿Reservamos memoria por el caso peor o por el real?", "ARREGLADO HOY",
 "`prev` era `vec![u32::MAX; VENTANA]` con VENTANA constante de 4 MiB, y al subirla a 32 MiB "
 "fija habria reservado 128 MB para cualquier entrada. Ahora la ventana se dimensiona a la "
 "ENTRADA con 32 MiB de tope: los bytes salen identicos y un fichero de 5 MB reserva 32 MB de "
 "cadena en vez de 128.",
 "bench: los ocho casos con bytes identicos entre ventana fija y ventana a la entrada. commit 7341e4d"),

("P4", "¿Tiramos algo que ya habiamos calculado?", "HALLAZGO — ARREGLADO",
 "El paso perezoso buscaba en pos+1 y pos+2 y, al aplazar, avanzaba UNO y volvia a buscar pos+1 "
 "DESDE CERO. Dos tercios de las busquedas iban a la basura.",
 "Se guarda la de pos+1. Va dentro del +67% de P2; separarlas exigiria una tercera compilacion "
 "y las dos son gratis en bytes, asi que no se separo. ⚠️ ESO ES UNA DEUDA DE ATRIBUCION: no se "
 "cuanto puso cada una."),

("P5", "¿El ORDEN de las fases obliga a repetir trabajo?", "LIMPIO",
 "Una sola pasada: buscar, emitir, indexar. No hay fases que se pisen.",
 "leido."),

("P6", "¿Estamos optimizando la perilla que importa?", "HALLAZGO — Y LA RESPUESTA SORPRENDE",
 "Llevaba toda la sesion optimizando LA BUSQUEDA por suposicion. Ablacionando con la perilla que "
 "ya existia -esfuerzo 0 no busca nada, todo literales- sobre 84 MB:\n"
 "    esfuerzo 0   31.273.574 B   2,1 s    <- ¡MAS LENTO que buscar!\n"
 "    esfuerzo 1    9.587.066 B   0,8 s\n"
 "    esfuerzo 6    8.094.669 B   1,2 s\n"
 "    esfuerzo 64   7.433.874 B   6,2 s\n"
 "NO BUSCAR ES MAS LENTO QUE BUSCAR POCO. Sin coincidencias, cada byte pasa por el codificador de "
 "literales -ocho decisiones binarias por byte, 672 millones en total-; con coincidencias, la "
 "mayoria de los bytes van dentro de copias y no lo tocan. Y la FORMA de la curva es lo que "
 "manda: de 1 a 6 cuesta 0,4 s y compra 1,49 MB; de 6 a 64 cuesta 5 s y compra 0,66 MB.",
 "La ablacion de arriba. El sitio donde trabajar no es «que la busqueda vaya mas rapido» sino "
 "«que a esfuerzo 6 se encuentre lo que hoy pide esfuerzo 64»."),

("P7", "¿La justificacion se llego a medir?", "HALLAZGO — la premisa de la pregunta era mia y era falsa",
 "«21 veces mas lento» comparaba NUESTRO maximo (esfuerzo 64, 13,6 MB/s) contra el DEFECTO de "
 "zstd (-9, 286,6 MB/s). A calidad parecida —esfuerzo 6, 71,5 MB/s— son CUATRO veces, no 21. "
 "El numero que asusta salio de comparar dos puntos de operacion distintos.",
 "vlz esf 6: 8.094.669 B a 71,5 MB/s · zstd -9: 7.740.539 B a 286,6 MB/s, mismos 84 MB."),

("P8", "¿Hacemos el mismo trabajo dos veces con parametros distintos?", "CERRADA con P4", "Ver P4.", "—"),

("P9", "¿Pagamos el caso general para servir al caso raro?", "HALLAZGO — SIN ARREGLAR",
 "`MAX_COPIA` se subio a 65.536 y `largo_comun` puede recorrer hasta ahi por CADA candidato. Las "
 "copias larguisimas son rarisimas y el tope se paga en todas. zstd lo resuelve con "
 "`targetLength` -16 en su nivel 9-: en cuanto la coincidencia es bastante buena, deja de buscar.",
 "MEDIDO, y por eso NO se aplica tal cual: cortar en 64 mas no indexar los interiores da "
 "+34% de velocidad pero cuesta +4,72% de bytes, y eso nos devuelve por detras de zstd -9. Es "
 "una PERILLA, no un arreglo: tiene que ir con su interruptor, no en el defecto."),

("P10", "¿El numero viaja con su contexto?", "AVISO — dos contextos que hay que decir siempre",
 "Los numeros de vlz cambian a la MITAD segun se midan concatenados o fichero a fichero, y el "
 "banco lo hacia sin decirlo. Y todos los de esta ficha son de 84 MB de codigo+csvjson+logs, "
 "concatenados y ordenados por ruta.",
 "bench/registros: uno a uno 10.493.824 contra concatenado 5.196.532 en codigo. commit b4dc979"),

("P11", "¿Lo que medimos sigue siendo lo que creemos?", "AVISO",
 "Dos veces hoy un arreglo invalido en silencio la medida de otra parte: la ventana=frame se "
 "llevo el -0,972% del CDC y dio la vuelta a la curva de los frames. Cualquier numero de esta "
 "ficha caduca en cuanto se toque el analizador.",
 "commits 704a490 y eb22537."),

("P12", "¿El instrumento conserva todo lo que la medida produjo?", "LIMPIO",
 "vlz-banco imprime bytes, ratio y las dos velocidades, y ABORTA sin imprimir tamaños si la ida "
 "y vuelta falla.",
 "crates/vlz/src/bin/vlz-banco.rs"),

("P13", "¿Quien comprueba la prueba?", "LIMPIO",
 "Cada medida hace la ida y vuelta y compara el Vec entero. Ninguna cifra de esta ficha viene de "
 "una compresion que no se haya descomprimido y comparado byte a byte.",
 "vlz-banco, y aborta con el offset del primer byte distinto."),

("P14", "¿Corremos TODAS nuestras suites?", "SIN MIRAR",
 "vlz no esta enchufado al archivador, asi que su suite no entra en la del producto. Cuando se "
 "enchufe hay que meterlo o pasara lo de siempre: verde en un sitio y roto en el otro.",
 "—"),

("P15", "¿Es esta la MEJOR prueba posible?", "HALLAZGO",
 "NO. Todo esta medido en UN corpus concatenado de 84 MB. Falta el reparto real del tiempo DENTRO "
 "de la busqueda: cuanto es recorrer la cadena, cuanto `largo_comun` y cuanto los fallos de "
 "cache al saltar por `prev`. Con eso se sabria si el siguiente paso es una cadena mejor o un "
 "indice distinto; sin eso se sigue optimizando a ojo, que es lo que P6 acaba de castigar.",
 "La prueba que falta: `perf stat` y `perf record` sobre vlz en el servidor, que es donde hay perf."),

("P16", "¿Se puede medir SIN construir?", "SI, y fue lo que desatasco esto",
 "La ablacion de P6 no necesito escribir nada: `esfuerzo` ya era una perilla y ponerla a 0 apaga "
 "la busqueda entera. Contesto la pregunta mas importante de la ficha con una compilacion que ya "
 "existia.",
 "esfuerzo 0/1/6/64 sobre los mismos 84 MB."),
]

with open("bench/camino-vlz-lento.tsv", "w", newline="") as f:
    f.write("# EL CAMINO A «vlz comprime mas que zstd -9 pero va muchisimo mas lento».\n")
    f.write("# ⚠️ LO PRIMERO QUE HIZO EL INSTRUMENTO FUE CORREGIR LA PREMISA: el «21 veces» comparaba\n")
    f.write("# nuestro maximo contra su defecto. A calidad parecida son CUATRO veces.\n")
    f.write("# Todo medido sobre 84 MB de codigo+csvjson+logs, CONCATENADOS y en orden de ruta.\n")
    w = csv.writer(f, delimiter="\t")
    w.writerow(["id", "la_pregunta", "veredicto", "que_se_encontro", "con_que_se_midio"])
    w.writerows(R)

c = collections.Counter(r[2].split()[0] for r in R)
print(f"\nbench/camino-vlz-lento.tsv: {len(R)} preguntas\n")
for r in R:
    print(f"  [{r[2][:34]:<34}] {r[0]:<4} {r[1]}")
print()
print("  " + " · ".join(f"{k}: {v}" for k, v in c.most_common()))
print()
print("  LO QUE DECIDE, y es P6: NO BUSCAR ES MAS LENTO QUE BUSCAR POCO -esfuerzo 0 tarda 2,1 s y")
print("  esfuerzo 1 tarda 0,8-. Y de 1 a 6 cuesta 0,4 s y compra 1,49 MB; de 6 a 64 cuesta 5 s y")
print("  compra 0,66 MB. El trabajo no es «que la busqueda vaya mas rapido»: es que a esfuerzo 6")
print("  se encuentre lo que hoy exige esfuerzo 64.")
print()
print("  Y LO QUE FALTA, P15: el reparto del tiempo DENTRO de la busqueda. Sin eso se sigue")
print("  optimizando a ojo — que es exactamente lo que P6 acaba de castigar.")
