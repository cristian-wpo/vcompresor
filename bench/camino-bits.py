# -*- coding: utf-8 -*-
"""EL CAMINO A «TRABAJAR AL NIVEL DEL BIT, SIN FORMATOS».

Lo pregunta el dueño: «si todo es 0 y 1 que pasa al procesador, ¿que nos impide procesar a nivel
o cerca del nivel del procesador, tipo binario? ¿nos ahorrariamos algo o no sabriamos
comprimirlo? asi no nos saltariamos todo lo de los formatos».

LA PREGUNTA TIENE TRES IDEAS DENTRO Y HAY QUE SEPARARLAS, porque una ya esta hecha, otra ya esta
medida y solo la tercera esta abierta:

  A) «trabajar con bits» — YA SE HACE. vc-cm predice BIT A BIT y el codificador aritmetico es
     bit a bit. No hay nada mas cerca del bit que eso.
  B) «saltarse los formatos» — YA ESTA MEDIDO Y YA SE DECIDE SOLO. Ver abajo.
  C) «contextos NO ALINEADOS A BYTE» — ESTO SI ESTA ABIERTO, y es la parte valiosa.

LO DE (B), MEDIDO en bench/carriles.tsv:
    preflate (saber que es DEFLATE)  compra 7,073% de bytes  y cuesta 59,24% de la CPU
    jpeg     (saber que es JPEG)     compra 3,933%           y cuesta 35,37%
    A precio DISCO con 1 mes de retencion los dos PIERDEN: -1.984 y -1.186 €/PB.
`--price` ya los apaga ahi. «Saltarse los formatos» no es una idea nueva: es un carril que se
enciende y se apaga por dinero, y el arbitro ya existe.

⚠️ Y HAY QUE DECIRLO: esta idea y la del «indice de estructuras por formato» —tambien del dueño,
media hora antes— apuntan en direcciones OPUESTAS. Una dice aprovecha lo que el formato ya
calculo; la otra dice ignoralo y ve al bit. No hay que elegir en abstracto: decide el precio del
disco contra el de la CPU, que es exactamente lo que `--price` calcula.

Lo escribe bench/camino-bits.py.
"""
import csv, collections

R = [
("P1", "¿Cuantas veces le preguntamos al nucleo lo mismo?", "NO APLICA",
 "Predecir y actualizar no hacen llamadas al sistema. Trabajar mas cerca del bit no cambia nada "
 "aqui: el nucleo no participa en el bucle caliente.",
 "la piramide de vc-cm: las cuatro fases son computo puro."),

("P2", "¿Cuantas veces leemos el mismo byte?", "HALLAZGO CONTRA la idea",
 "Un contexto NO alineado a byte lee cada byte en OCHO desplazamientos distintos. Hoy los "
 "contextos de orden ya se rehacen desde cero -35 pasos de hash por byte donde bastan 7- y "
 "pasar a bits lo multiplicaria por ocho antes de arreglar lo que ya sobra.",
 "set_byte_contexts, leido. Y la piramide dice que «Preparar» es solo el 2,6%, asi que ni el "
 "arreglo ni el empeoramiento mueven mucho."),

("P3", "¿Reservamos memoria por el caso peor o por el real?", "HALLAZGO CONTRA la idea",
 "Los contextos de bit tienen ~8 veces el espacio de estados de los de byte: el mismo contexto "
 "en ocho alineaciones. Y ya estamos en 484 MiB que no caben en 16 MiB de L3 — la regla 1 de la "
 "norma de precomputar ya esta rota a lo grande y esto la rompe ocho veces mas.",
 "bench/precomputar.tsv, la fila «tablas de modelos de vc-cm»: EL LIMITE."),

("P4", "¿Tiramos algo que ya habiamos calculado?", "HALLAZGO — y es el corazon de la pregunta",
 "Saltarse los formatos es TIRAR A PROPOSITO el analisis que el fichero ya trae hecho. Quien "
 "hizo ese PNG ya pago la busqueda de coincidencias. Ir al bit crudo significa volver a "
 "encontrarlas nosotros. La pregunta no es si se puede: es si sale mas barato aprenderlo que "
 "que te lo digan.",
 "MEDIDO: saber el formato compra 11,006% de bytes (7,073 preflate + 3,933 jpeg) y cuesta el "
 "94,61% de la CPU de transformada. A precio S3 se paga; a precio disco PIERDE. bench/carriles.tsv."),

("P5", "¿El ORDEN de las fases obliga a repetir trabajo?", "LIMPIO",
 "El orden ya esta: enrutar decide antes de transformar, y transformar antes de comprimir. "
 "Trabajar al nivel del bit no cambia el orden, cambia lo que ve la ultima fase.",
 "la piramide del archivador, 14 fases cronometradas."),

("P6", "¿Estamos optimizando la perilla que importa?", "HALLAZGO CONTRA la idea",
 "NO. El 83,7% del tiempo de vc-cm es predecir+actualizar sobre tablas que no caben en cache. Un "
 "modelo de bits multiplica los contextos y por tanto los fallos de cache. Ataca la perilla "
 "equivocada EN LA DIRECCION EQUIVOCADA.",
 "bench/piramide-vccm.tsv. Buscar y escribir el cubo son 14,3%; el resto de esas dos fases es la "
 "aritmetica de los 43 modelos, tambien proporcional al numero de contextos."),

("P7", "¿La justificacion se llego a medir?", "SOSPECHA — la intuicion del procesador no transfiere",
 "«Todo es 0 y 1 para el procesador» es cierto y no implica lo que parece. El procesador no "
 "trabaja en bits sueltos: su unidad es la PALABRA -64 bits- y sus instrucciones son de byte "
 "para arriba. Extraer un bit suelto cuesta un desplazamiento y una mascara MAS que leer un byte. "
 "Trabajar «a nivel del procesador» apunta a lo contrario: a trabajar con MAS bits a la vez, no "
 "con menos.",
 "Lo unico bit a bit que ya tenemos y va rapido es el codificador aritmetico, y va rapido porque "
 "procesa bits SIN tabla: 0,0022-0,11% de perdida y sobrecoste por bit CERO (M5, cerrada)."),

("P8", "¿Hacemos el mismo trabajo dos veces con parametros distintos?", "SIN MIRAR",
 "Si hubiera modelos de bit conviviendo con los de byte, serian dos vistas del mismo dato. "
 "Ahora mismo no hay ninguno de bit, asi que no aplica todavia.",
 "—"),

("P9", "¿Pagamos el caso general para servir al caso raro?", "HALLAZGO — a favor Y en contra",
 "Un modelo universal de bits ES el caso general para todo, y ese es a la vez su atractivo y su "
 "coste: no hay que escribir un lector por formato -ni mantenerlo, ni defenderlo de ficheros "
 "malformados- pero se paga el mismo precio en el 100% de los datos incluso donde el formato lo "
 "regalaba.",
 "MEDIDO por el lado del coste: abrimos TRES contenedores (gzip, png, zip) y con eso cubrimos "
 "1.825 PNG + 12.569 JPEG del corpus. Un lector nuevo por formato es trabajo; el modelo "
 "universal es tiempo de maquina en cada byte."),

("P10", "¿El numero viaja con su contexto?", "LIMPIO — hoy",
 "Las cifras de esta ficha salen de bench/carriles.tsv y bench/piramide-vccm.tsv, las dos "
 "medidas aqui y con su comando. El 83,7% se corrigio hoy: es predecir+actualizar, no busquedas.",
 "bench/carriles.py y bench/piramide-vccm.py."),

("P11", "¿Lo que medimos sigue siendo lo que creemos?", "AVISO",
 "«Saltarse los formatos» y «aprovechar el indice del formato» son ideas OPUESTAS y las dos son "
 "del dueño, con media hora de diferencia. No se contradicen: se arbitran. Pero medir una sin "
 "decir cual de las dos se esta midiendo daria un numero que apoya lo que uno quiera.",
 "El arbitro ya existe: `--price` compara bytes x precio de disco contra CPU x precio de maquina."),

("P12", "¿El instrumento conserva todo lo que la medida produjo?", "LIMPIO desde hoy",
 "mide.sh guarda la salida del comando desde esta mañana. Antes se comio tres tablas.",
 "bench/registros/salidas/."),

("P13", "¿Quien comprueba la prueba?", "SIN MIRAR",
 "Si se construye un modelo de bit, la comprobacion obligatoria es la de siempre: que la ida y "
 "vuelta sea bit a bit sobre los 1.973 flujos del banco. No hay atajo.",
 "—"),

("P14", "¿Corremos TODAS nuestras suites?", "NO APLICA todavia",
 "No hay codigo de esto que probar.", "—"),

("P15", "¿Es esta la MEJOR prueba posible?", "HALLAZGO — la prueba correcta es otra",
 "La pregunta «¿sirve trabajar en bits?» es demasiado grande para medirla de una vez. La version "
 "que SI se puede medir hoy, y es la parte de la idea que de verdad esta abierta: NUESTROS "
 "MODELOS SON TODOS ALINEADOS A BYTE -h[7..8], columnas, palabras- y hay datos que NO lo estan. "
 "Un flujo DEFLATE es bit-packed: un codigo Huffman ocupa 3 bits, luego 7, luego 11, y ningun "
 "limite de byte significa nada. Sobre ese dato nuestros modelos pueden estar viendo ruido.",
 "LA PRUEBA, barata: comprimir el mismo dato DESPLAZADO 1..7 bits. Si el ratio apenas cambia, "
 "los modelos ya son indiferentes a la alineacion y no hay nada que ganar. Si cambia mucho, "
 "estan atados al byte y sobre dato bit-packed estan dejando algo. bench/alineacion.sh."),

("P17", "¿Y un LENGUAJE PROPIO solo para el compresor?", "HALLAZGO — pero no el que la pregunta sugiere",
 "Hay que separar DOS cosas que suenan igual. (a) Una VM INTERPRETADA que ejecute el modelo, con "
 "el modelo viajando dentro del archivo: eso es ZPAQL, existe, esta maduro, lo hizo el inventor "
 "de zpaq — y YA LE GANAMOS. En codigo.10m deja 869.317 B contra nuestros 817.930, y en TODOS los "
 "corpus de arbol le sacamos entre 4% y 13%. Ademas un interprete mete despacho ENCIMA del cuello: "
 "el 83,7% es latencia de memoria sobre 484 MiB y ningun lenguaje hace que un fallo de cache tarde "
 "menos. (b) Una DESCRIPCION que GENERA el codigo en tiempo de compilacion: eso no es un lenguaje "
 "de programacion, es uno de descripcion, y cuesta CERO en ejecucion.",
 "LA FACTURA DE (b) ES DE HOY, contable: la definicion de UN modelo vive en DIEZ sitios —NOMBRES, "
 "set_byte_contexts, DUPLICADOS, PODA_V2, ISSE_CTX, NTAB, NCTX, las constantes I*, "
 "apagados_de_modelo y las familias—. Y de esa dispersion salieron CUATRO fallos el 31-07: "
 "(1) el interruptor de ablacion cubria 34 de 43 y dejaba fuera los dos de mas peso; "
 "(2) ISSE_CTX y DUPLICADOS son los mismos seis indices y nada lo dice —estuve a punto de romper "
 "la pieza que mas aporta, +3,91%—; (3) la dispersion saltaba los de tabla apagados pero no los "
 "derivados; (4) el instrumento de correlacion necesita un mapa de nombres a mano. Una "
 "declaracion unica las habria hecho imposibles las cuatro. "
 "LA PRUEBA QUE LO CIERRA, y es barata: coger los tres cambios de modelo mas recientes del "
 "historial y contar cuantos de los diez sitios hubo que tocar en cada uno. Si son uno o dos, "
 "la dispersion no duele y esto es apetito de reescribir. Si son seis o mas, la factura es real."),

("P16", "¿Se puede medir SIN construir?", "LIMPIO — y ya estaba medido",
 "Si. «Saltarse los formatos» se mide apagando las transformadas, y la bandera existe desde "
 "antes: VC_SIN_PREFLATE. El resultado esta en bench/carriles.tsv y es el que decide la "
 "pregunta (B). Y lo de la alineacion se mide desplazando el fichero, sin tocar el codec.",
 "VC_SIN_PREFLATE + bench/carriles.sh, ya corridos. bench/alineacion.sh, nuevo."),
]

with open("bench/camino-bits.tsv", "w", newline="") as f:
    f.write("# EL CAMINO A «TRABAJAR AL NIVEL DEL BIT, SIN FORMATOS».\n")
    f.write("# La pregunta lleva TRES ideas dentro: (A) trabajar con bits, que YA se hace —vc-cm\n")
    f.write("# predice bit a bit—; (B) saltarse los formatos, que YA esta medido y ya lo decide\n")
    f.write("# `--price` por dinero; y (C) contextos NO ALINEADOS A BYTE, que es lo unico abierto.\n")
    f.write("# Ninguna respuesta entra sin decir QUE se miro y CON QUE.\n")
    w = csv.writer(f, delimiter="\t")
    w.writerow(["id", "la_pregunta", "veredicto", "que_se_encontro", "con_que_se_midio"])
    w.writerows(R)

c = collections.Counter(r[2].split()[0] for r in R)
print(f"\nbench/camino-bits.tsv: {len(R)} preguntas a «trabajar al nivel del bit»\n")
for r in R:
    print(f"  [{r[2]:<34}] {r[0]:<4} {r[1]}")
print()
print("  " + " · ".join(f"{k}: {v}" for k, v in c.most_common()))
print()
print("  EL RESUMEN: tres de las preguntas van EN CONTRA de la idea tal cual suena —P2, P3 y P6:")
print("  contextos de bit multiplican por ocho el espacio de estados, y el 83,7% del tiempo YA es")
print("  tabla que no cabe en cache—. Y P7 desmonta la intuicion de origen: el procesador NO")
print("  trabaja en bits sueltos, su unidad es la palabra de 64. «Mas cerca del procesador»")
print("  apunta a trabajar con MAS bits a la vez, no con menos.")
print()
print("  PERO P15 rescata la parte que si vale: todos nuestros modelos estan ATADOS AL BYTE y hay")
print("  datos que no lo estan. Un flujo DEFLATE es bit-packed y ningun limite de byte significa")
print("  nada ahi. Eso no lo ha mirado nadie y se mide desplazando el fichero.")
print()
