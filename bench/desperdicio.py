#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""EL CENSO DEL DESPERDICIO: donde el resultado es correcto y el camino no.

La norma del dueño, dicha para todo el proyecto:

    «Porque algo este hecho y de buen resultado no significa que este bien hecho. Una
    calculadora que sume 2 + 2 y de 4 no significa que los procesos que se utilizaron de
    principio a fin sean la mejor forma de hacer esa calculadora, porque aunque el resultado si
    debe ser 4, capaz se esta gastando tiempo, RAM y CPU de forma innecesaria.»

POR QUE HACE FALTA UNA TABLA APARTE. Ninguna de estas lineas es un fallo. Todas las pruebas
pasan, la ida y vuelta es byte a byte, el archivo es correcto. Por eso NO las encuentra nada de
lo que ya tenemos: los tests miran la salida, la aguja mira si algo ha empeorado respecto a
ayer, y la piramide mira donde se va el tiempo —pero un gasto que lleva ahi desde el primer dia
y nunca ha empeorado es invisible para las tres—. Hace falta preguntarlo a proposito.

LA REGLA DE ADMISION. Una linea entra aqui solo si se puede decir las tres cosas:
    1. que hace bien (porque si no, es un fallo y va a la lista de fallos)
    2. que gasta de mas, con una medida o con un «sin medir» dicho en voz alta
    3. cual seria el camino corto
Sin las tres es una opinion sobre el codigo de otro, y de eso no se saca nada.

SE INCLUYEN NUESTRAS PROPIAS HERRAMIENTAS. La norma dice «todo el proyecto», y el banco de
pruebas es parte del proyecto. Dos de las lineas de aqui son instrumentos que daban numeros
correctos por un camino malo, y se arreglaron hoy.
"""
import csv
import os
import collections

# id · donde · que hace BIEN · que gasta de mas · cuanto · el camino corto · estado
D = [
("D1", "vc-core/src/lib.rs:229,1891,1902,1907 + 133,139",
 "Los metadatos que se guardan son los correctos: modo, fecha, dueño y tamaño del fichero.",
 "SEIS llamadas al nucleo por fichero que devuelven LA MISMA estructura. recorrer() la pide y "
 "se queda solo con un enum de un byte; PROG_TOTAL_BYTES la pide otra vez; PROG_BYTES otra; "
 "`let size` otra en la misma iteracion; meta_de() otra para el modo; duenos_de() otra para el "
 "uid. Sobre 43.298 ficheros son ~260.000 viajes al nucleo donde bastarian 43.298.",
 "@syscalls",
 "recorrer() YA tiene el struct entero. Devolverlo junto al Tipo en vez de tirarlo, y pasarlo "
 "por el resto del bucle. Es un cambio de tipo de retorno, no un algoritmo nuevo.",
 "ARREGLADO A MEDIAS 2026-07-30 — quitados dos de los seis (meta_de+duenos_de fundidas, y los "
 "dos stat de la misma iteracion a uno). Quedan el de recorrer(), que sigue tirando el struct, "
 "y el de PROG_TOTAL_BYTES, que depende del anterior."),

("D2", "vc-core/src/lib.rs:133-142",
 "meta_de devuelve modo y fecha; duenos_de devuelve uid y gid. Las dos aciertan.",
 "Son dos symlink_metadata sobre la MISMA ruta, escritas una debajo de otra y llamadas una "
 "debajo de otra, para leer dos mitades del mismo struct. Es el caso mas puro de todo el censo: "
 "no hay ninguna razon, solo que se escribieron en dos momentos distintos.",
 "2 de los 6 stat de D1",
 "Una sola funcion que devuelva lo que haga falta, o directamente el Metadata.",
 "ARREGLADO 2026-07-30 — atributos_de() devuelve las cinco cosas de UNA llamada, y las nueve "
 "parejas de llamadas estan fundidas."),

("D3", "vc-core/src/lib.rs:1030 (lee_prefijo) desde 1069 (agrupa_por_similitud)",
 "El agrupamiento por similitud funciona y esta medido: junta los ficheros que comparten "
 "trozos y deja el resto donde estaba.",
 "Para calcular CUATRO numeros por fichero, reserva un buffer de 1 MiB —vec![0u8; max], no del "
 "tamaño del fichero— lee, y tira los bytes. La mediana del arbol real es de 1.972 B: se pide "
 "un mega para leer dos kilos, y eso son ~26 GB de memset a lo largo del arbol. Ademas el "
 "fichero se vuelve a abrir y a leer entero veinte lineas mas abajo. (CORRECCION: la primera "
 "version de esta linea decia que cada fichero pagaba un mmap y un munmap por pasar del umbral de glibc. El contador dice 92 mmap EN TOTAL para 27.162 ficheros: glibc sube el umbral solo. El arreglo era correcto y la razon que se dio, falsa.)",
 "@syscalls",
 "take + read_to_end crece segun lo que llega, asi que no hace falta saber el tamaño de "
 "antemano y el problema del orden de fases se esquiva sin tocar el orden.",
 "ARREGLADO A MEDIAS 2026-07-30 — la reserva ya va por lo que hay, no por el tope. Sigue "
 "abierto que el fichero se lee DOS veces: una para la firma y otra entera."),

("D4", "vc-core/src/cdc.rs:114 (sketch) contra la fase leer+trocear",
 "La firma minhash identifica bien los parecidos, y esta medida: ordenar por ella a secas "
 "empeora, y como refuerzo del orden por ruta funciona.",
 "sketch() trocea con gear y hashea con blake3 el primer MiB. Ese mismo primer MiB se vuelve a "
 "trocear con gear y a hashear con blake3 en la fase de leer+trocear. Los parametros son "
 "distintos (1024/4096/16384 contra los del CDC principal), asi que no es literalmente el mismo "
 "trabajo — pero son dos pasadas de rodadura sobre los mismos bytes, y para la mayoria del "
 "arbol (mediana 1.972 B) ese «primer MiB» es el fichero entero.",
 "sin medir por separado",
 "Una sola pasada que saque las dos cosas: el mismo hash rodante con DOS mascaras, una fina "
 "para la firma y otra gruesa para los trozos de verdad.",
 "ABIERTO"),

("D5", "vc-core/src/lib.rs:1108 (firma_de) y 1915 (fs::read)",
 "Enrutar por la FIRMA del contenido y no por la extension es una decision correcta y medida: "
 "en el corpus real hay 58 PNG llamados .jpg que irian al carril equivocado.",
 "firma_de abre el fichero, lee 8 bytes y lo cierra. Si acierta, fs::read lo abre otra vez y lo "
 "lee entero. Dos open y dos recorridos de la tabla de inodos para un fichero que se iba a leer "
 "de todas formas.",
 "@syscalls",
 "Leer una vez y mirar la cabecera del buffer que ya se tiene. El caso que lo justificaria "
 "—descartar sin cargar— no aplica aqui, porque el que acierta se carga entero igual.",
 "ABIERTO"),

("D6", "bench/mide.sh",
 "La bascula registra bien: commit, maquina, carga, bytes, segundos, ciclos, fallos de cache. "
 "Y detecta el caso de «rc=0 pero el fichero no esta».",
 "Mandaba la salida de error del programa a un temporal y lo BORRABA al acabar. Una medida de "
 "siete minutos producia dos cosas —los numeros y el desglose por fases que el propio programa "
 "escribe— y el instrumento se quedaba con una. Para volver a ver el desglose habia que repetir "
 "los siete minutos.",
 "hasta 7 min de maquina por cada vez que hiciera falta el desglose",
 "Una opcion -s que lo guarda. Cuatro lineas.",
 "ARREGLADO 2026-07-30"),

("D7", "bench/veredictos.py",
 "La tabla de veredictos hace bien lo dificil: traduce tiempo y bytes a euros, que es la unica "
 "unidad en la que se pueden comparar, y dice el modelo entero para que se pueda discutir.",
 "Tenia las medidas COPIADAS A MANO dentro del guion. Al copiar el numero se dejaba atras todo "
 "lo que iba con el —la carga de la maquina, el commit, la hora—, y sin la carga no hay forma "
 "de saber si las dos mitades de una comparacion son comparables. No lo eran: publicaba una "
 "medida de carga 56,3 contra una de carga 6,5.",
 "dos de los cinco veredictos publicados eran falsos",
 "Leer de bench/historia.jsonl, emparejar por reticulo, y rechazar la pareja si las cargas no "
 "se parecen. De paso aparecieron parejas limpias que nadie habia mirado.",
 "ARREGLADO 2026-07-30"),

("D9", "bench/panel.py + el arranca.sh del Mac",
 "El panel es UTF-8 correcto de principio a fin y siempre lo fue.",
 "No lo DECLARABA. arranca.sh lo sirve con python3 -m http.server, que manda Content-Type sin "
 "charset, asi que el navegador adivinaba latin-1 y salian «cAdigo» y «29.6A—» en toda la "
 "pagina. Abriendolo como fichero local se veia bien, y eso es justo lo que hacia que no se "
 "notara: la forma PREVISTA de abrirlo era la unica rota.",
 "una linea de HTML contra una pagina entera ilegible",
 "<meta charset=\"utf-8\"> lo primero del fichero.",
 "ARREGLADO 2026-07-30"),

("D10", "crates/vc-core/src/inflate.rs — el test copia_solapada",
 "El descompresor acierta: para el flujo que le daban devolvia exactamente lo mismo que zlib.",
 "El VECTOR del test estaba tecleado a mano y mal: 4b 4c 84 00 00, que codifica ocho aes, "
 "cuando el test esperaba seis. El test llevaba rojo desde por la mañana y se dio el dia por "
 "bueno porque la suite que se miraba era otra (bench/producto.sh, 29 verdes).",
 "un test rojo en el arbol + la sospecha de un fallo en la pieza mas delicada",
 "Los bytes que zlib produce de verdad, y anotado en el propio test el comando que los "
 "regenera. Un vector a mano en una prueba de formato binario es una segunda implementacion "
 "del formato escrita sin querer.",
 "ARREGLADO 2026-07-30"),

("D11", "bench/syscalls.sh",
 "La idea es la correcta: contar syscalls en vez de cronometrar, porque un conteo no tiene "
 "ruido y esta pregunta no necesita reloj.",
 "La primera version leia LA COLUMNA EQUIVOCADA. En `strace -c` la columna «errors» va vacia "
 "cuando no hay errores, asi que coger el campo justo antes del nombre da «calls» en unas filas "
 "y «errors» en otras. Publico «openat: 1 llamada» —que era 1 ERROR— junto a 3,74 close por "
 "fichero, una combinacion imposible con toda la pinta de un dato. Y ademas comparaba --group "
 "contra «por defecto» justo despues de que el defecto volviera a ser --group: las dos ramas "
 "eran la misma, y salieron cifras identicas al syscall.",
 "dos numeros falsos que iban derechos a este mismo censo",
 "Parsear por NOMBRE DE COLUMNA leyendo la cabecera, y comprobar que las dos ramas sacan bytes "
 "distintos antes de publicar nada.",
 "ARREGLADO 2026-07-30"),

("D12", "vc-core/src/lib.rs — el camino incremental",
 "El dedup por trozos funciona y funciona muy bien: una segunda instantanea del MISMO arbol "
 "añade 200.297 B sobre 314.110.498, o sea el 0,06%.",
 "PERO REHACE TODO EL TRABAJO. No hay ninguna comprobacion de «este fichero no ha cambiado»: "
 "cada respaldo vuelve a leer, inflar, transformar, trocear y hashear el arbol entero. MEDIDO "
 "en el Mac: la 1a instantanea 29,89 s, la 2a del mismo arbol 30,72 s — un +2,8% que esta "
 "DENTRO del suelo de ruido de 10,8%. El dedup ahorra el 99,94% del espacio y el 0% del tiempo.",
 "el 78% del reloj, rehecho en cada respaldo diario",
 "HECHO el 2026-07-31. Un fichero cuya (talla, mtime) no ha cambiado se reusa entero. MEDIDO "
 "sobre el arbol de 2,7 GB y 43.298 ficheros, en serie y a cargas comparables (4,4–5,1): reloj "
 "173,1 s -> 17,7 s (9,8x), CPU 450,8 -> 18,6 CPU-s (24,3x), memoria 1.722 -> 340 MiB (5,07x). "
 "Los cuatro archivos salen byte a byte iguales con el salto y sin el, y los dos arboles "
 "extraidos son identicos. EL PRIMER INTENTO ENSEÑA MAS QUE EL SEGUNDO: con la comprobacion "
 "dentro del bucle de ficheros el trabajo por fichero desaparecia entero —leer+trocear, hash, "
 "comprimir y escribir a 0,00 s sobre los 43.298— y aun asi el reloj solo bajaba un 14%, porque "
 "las dos pasadas caras (JXL y preparacion del DEFLATE) corren sobre el LOTE ENTERO antes del "
 "bucle: 134,6 s de 136,2 preparando ficheros que un instante despues se daban por buenos sin "
 "mirarlos. Saltarse el trabajo despues de haberlo preparado no es saltarselo.",
 "ARREGLADO 2026-07-31 — y la apuesta esta dicha: (talla, mtime) no es una huella, asi que un "
 "fichero reescrito con la misma talla exacta y el mtime devuelto a mano se escapa. La prueba 6 "
 "de bench/incremental-verifica.sh lo hace A PROPOSITO para que el agujero quede medido y no "
 "supuesto. Tampoco se salta nada cuyo mtime sea el segundo en curso (guardia «racily clean»)"),

("D13", "toda la tuberia — cada pieza hace su propia pasada",
 "Cada etapa esta bien separada y se puede apagar, medir y sustituir por su cuenta. Esa "
 "separacion es lo que ha permitido ablacionar pieza a pieza todo el mes.",
 "PERO CADA UNA VUELVE A RECORRER LOS MISMOS BYTES, y la cara no es la que parece. Las COPIAS "
 "del plano son baratas: 6,03 GB copiados tres veces a 17,2 GB/s medidos son ~1 s de 173, el "
 "0,6%. Lo caro es la RE-COMPUTACION. El DEFLATE de entrada YA TRAE las coincidencias "
 "encontradas -son los tokens que capturamos desde la fase 1b-, y el camino de hoy las tira, "
 "expande 357 MB a 6,03 GB (17x) y le paga a zstd para que las vuelva a buscar. Y preflate "
 "ademas re-deflata el plano entero solo para verificarse.",
 "MEDIDO en la piramide: «comprimir» son 133,28 s de los 197,46 s de CPU del programa entero, "
 "el 67,5%. Y el re-deflate de verificacion de preflate son 41,58 s de sus 172,97 s, el 24%. "
 "Las copias, en cambio, son el 0,6%: la intuicion de «lecturas repetidas» apunta al sitio "
 "correcto y al coste equivocado. "
 "Y MEDIDO EL 2026-07-31 con bin/vc-zstd-perfil sobre 128 MB de plano REAL del banco, y la "
 "respuesta no es si ni no, es «depende del precio del disco». Tres cifras. (a) BUSCAR ES >=99% "
 "DEL TIEMPO DE ZSTD: barriendo las nueve estrategias a nivel 19, de `fast` a `btultra2`, el "
 "reloj va de 0,049 s a 7,3 s —144 veces— y lo unico que cambia entre ellas es cuanto se "
 "esfuerza el buscador. (b) PERO LAS COINCIDENCIAS DEL DEFLATE NO SON LAS DE ZSTD: DEFLATE mira "
 "32 KB y zstd -19 mira 8 MB, y limitando zstd a la ventana de DEFLATE PIERDE EL 30,37% del "
 "tamaño. La mayoria de esa perdida ya se recupera a 512 KB (18,44%), o sea que no hace falta "
 "irse a 8 MB. (c) EL TRATO, por PB de plano: +43,6% de bytes guardados a cambio de 15.373,7 -> "
 "99,4 horas de CPU.",
 "USAR LOS TOKENS COMO PUNTO DE PARTIDA Y NO COMO SUSTITUTO: buscar solo lo que queda lejos, "
 "que es de donde viene el 30,37%. Eso no esta medido todavia y es lo que hay que medir. Y LA "
 "OTRA MITAD DE D13 sigue en pie y es independiente: que el inflate entregue los bytes SEGUN "
 "LOS PRODUCE y que el troceador, el hasheador y el router se enganchen a ese chorro, en vez de "
 "recibir un bufer de 6 GB ya hecho. Las dos cosas solo se pueden hacer con el codec propio.",
 "MEDIDO — no es una mejora, es una PALANCA DE PRECIO: a precio S3 pierde 2.252 €/PB-año y a "
 "precio disco gana 794. Justo la decision que D8 dice que --price deberia estar tomando y no "
 "toma: hoy --price elige el nivel de zstd, que mueve un 0,8%, y no el carril, que mueve esto"),

("D8", "vc-cli/src/main.rs — la opcion --price",
 "El planteamiento es el correcto y no lo hace nadie mas: elegir el ajuste por lo que te cuesta "
 "a TI, sondeando tus datos y poniendo precio al disco y a la CPU.",
 "Sondea para elegir el NIVEL de zstd, que entre el 17 y el 19 mueve un 0,8% del tamaño. Y no "
 "toca los carriles, que es donde esta todo: entre 0,15x y 5,92x de retorno segun el precio del "
 "almacenamiento. Optimiza la perilla que no importa.",
 "MEDIDO el 2026-07-31 con bench/carriles.sh y bench/carriles.py, apagando UN carril cada vez "
 "sobre el arbol de 2,7 GB, dos vueltas, con la ida y vuelta comprobada en cada archivo. La "
 "economia de los cinco carriles, y con la retencion POR DEFECTO de 12 meses: "
 "preflate compra el 7,073% de los bytes y cuesta 298,0 CPU-s (el 59,2% del total); JXL compra "
 "el 3,933% y cuesta 177,9 CPU-s (35,4%). A precio S3 los dos se pagan de sobra (+5.163 y "
 "+2.788 €/PB-año); a precio DISCO los dos PIERDEN (-1.613 y -980). Y son justo los dos que "
 "estan SIEMPRE encendidos: en disco a un año se estan tirando ~2.593 €/PB-año, que es el 38% "
 "del coste total de nivel 9 en disco (6.870). Los otros tres carriles -e8e9, agrupar y "
 "log-ts- tienen un coste POR DEBAJO DEL SUELO DE RUIDO (2,96%), asi que no se decide sobre "
 "ellos: se dice que estan sin medir.",
 "Que --price elija CARRILES con esta tabla. La decision ya no hay que inventarla: esta en "
 "bench/carriles.tsv, precio a precio y retencion a retencion. Falta cablearla a la sonda, que "
 "ya existe y ya acierta (-2,3% de error medido).",
 "MEDIDO — la tabla de decision existe; falta cablearla a --price"),
]

# Los numeros medidos se LEEN de bench/syscalls.tsv, no se copian aqui. Es exactamente el
# error de D7, y seria feo repetirlo en la tabla que lo denuncia.
med = {}
if os.path.exists("bench/syscalls.tsv"):
    with open("bench/syscalls.tsv") as f:
        filas = [l.rstrip("\n").split("\t") for l in f if not l.startswith("#")]
    cab = filas[0]
    for r in filas[1:]:
        if len(r) != len(cab):
            continue
        d = dict(zip(cab, r))
        med.setdefault((d["muestra"], d["config"]), {})[d["llamada"]] = float(d["por_fichero"])

if med:
    trozos = []
    for (m, c), v in sorted(med.items()):
        st = v.get("newfstatat", 0) + v.get("statx", 0)
        op = v.get("openat", 0)
        trozos.append(f"{m}/{c}: {st:.1f} stat y {op:.1f} open por fichero")
    resumen = " · ".join(trozos)
else:
    resumen = "SIN MEDIR TODAVIA — corre bench/syscalls.sh"

filas = []
for id_, donde, bien, gasta, cuanto, corto, estado in D:
    filas.append((id_, donde, bien, gasta,
                  resumen if cuanto == "@syscalls" else cuanto, corto, estado))

with open("bench/desperdicio.tsv", "w", newline="") as f:
    f.write("# EL CENSO DEL DESPERDICIO: donde el resultado es correcto y el camino no.\n")
    f.write("# Ninguna de estas lineas es un fallo. Todas las pruebas pasan y el archivo es\n")
    f.write("# correcto. Por eso no las encuentra nada de lo que ya tenemos y hay que preguntarlo\n")
    f.write("# a proposito. Regla de admision: hay que poder decir que hace BIEN, que gasta de\n")
    f.write("# mas (con medida o con un «sin medir» dicho en voz alta) y cual es el camino corto.\n")
    f.write("# Lo escribe bench/desperdicio.py. Los numeros los lee de bench/syscalls.tsv.\n")
    w = csv.writer(f, delimiter="\t")
    w.writerow(["id", "donde", "que_hace_bien", "que_gasta_de_mas", "cuanto", "camino_corto",
                "estado"])
    w.writerows(filas)

def estado(e):
    if e.startswith("ARREGLADO A MEDIAS"): return "A MEDIAS"
    if e.startswith("ARREGLADO"):          return "ARREGLADO"
    return "ABIERTO"

c = collections.Counter(estado(r[6]) for r in filas)
print(f"bench/desperdicio.tsv: {len(filas)} lineas · {c['ABIERTO']} abiertas · "
      f"{c['A MEDIAS']} a medias · {c['ARREGLADO']} arregladas\n")
for r in filas:
    print(f"  [{r[0]}] {r[6]:<22} {r[3][:96]}")
print()
print("  medida de syscalls:", resumen)
