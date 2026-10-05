# -*- coding: utf-8 -*-
"""EL CAMINO A NUESTRA PROPIA ARQUITECTURA.

Lo pide el dueño, y corrigiendome: «tenemos una arquitectura pero no podemos darla por perfecta,
tenemos que cuestionarla». Y tenia razon en el acto — media hora antes yo habia medido que las
vistas extra de paq8px EMPEORAN en nuestro codigo y habia concluido «sus vistas no se transfieren
a NUESTRA arquitectura». Eso DA NUESTRA ARQUITECTURA POR BUENA. La misma medida es igual de
compatible con «nuestra arquitectura es el problema», y yo elegi la lectura que no nos pregunta
nada.

Lo escribe bench/camino-arquitectura.py.
"""
import csv, collections

R = [
("P1", "¿Cuantas veces le preguntamos al nucleo lo mismo?", "NO APLICA — pero la version util SI",
 "Aqui no hay llamadas al sistema; la pregunta equivalente es cuantos ACCESOS A MEMORIA "
 "PRINCIPAL cuesta un contexto, que es el fallo de cache. Su ContextMap2 lo tiene medido y "
 "escrito: «3 accesos a memoria principal por contexto y byte, todos los demas a cache», y la "
 "estructura esta diseñada alrededor de eso —ranura de 7 bytes, cubos de 64—.",
 "NOSOTROS NO TENEMOS ESE NUMERO. bench/coste.tsv da `fallos_pct` al quitar cada modelo, que es "
 "otra cosa: dice cuanto BAJAN los fallos, no cuantos hay por contexto y byte."),

("P2", "¿Cuantas veces leemos el mismo byte?", "LIMPIO en el codec",
 "El bucle de contextos toca cada estado una vez por bit. `busca_cubo` hace un memcmp de firma "
 "por contexto y byte, no por bit.", "leido."),

("P3", "¿Reservamos memoria por el caso peor o por el real?", "HALLAZGO",
 "Nuestro cubo son 16 bytes: 1 de firma + 15 nodos del arbol del nibble. Se reservan los 15 "
 "SIEMPRE, y en un contexto poco visitado la mayoria no se estrena nunca. No esta medido cuantos "
 "nodos se usan de verdad.",
 "SIN MEDIR. Es barato: contar nodos con estado != 0 al acabar."),

("P4", "¿Tiramos algo que ya habiamos calculado?", "🚨 HALLAZGO — Y ES EL GRANDE",
 "SI, Y ES LA INFORMACION MAS BARATA QUE HAY. Cuando un contexto se visita, sabemos QUE BYTE "
 "vino detras. No lo guardamos. Guardamos solo la historia de BITS: 8 bits por nodo del arbol.\n"
 "Ellos guardan por contexto, en 7 bytes: los estados de bits Y la CUENTA DE RACHA del ultimo "
 "byte Y LOS TRES ULTIMOS BYTES DISTINTOS. De sus siete vistas al mezclador, TRES salen de esos "
 "seis bytes que nosotros no tenemos — y son las que enganchan en el caso mas comun: «aqui "
 "siempre viene una coma», «aqui vino 'e' cuarenta veces seguidas».",
 "Nuestra ranura: 16 bytes por contexto y nibble, TODO historia de bits. La suya: 7 bytes por "
 "ranura de los que 4 NO son historia de bits."),

("P5", "¿El ORDEN de las fases obliga a repetir trabajo?", "HALLAZGO — el enrutador llega tarde para el codec",
 "El archivador clasifica por carril ANTES de comprimir, y el codec no usa esa informacion: "
 "vc-cm corre la MISMA cadena de 5 APM y los MISMOS 4 selectores para texto, binario y todo lo "
 "demas. Ellos especializan la cadena SSE por tipo de bloque y tuvieron que construir el "
 "clasificador para poder hacerlo; nosotros lo tenemos y no se lo pasamos.",
 "⚠️ Y ojo con la prioridad: su ablacion (`-forcetext` contra `-forcebinary` contra auto) dice "
 "que esa especializacion vale 1,4% en codigo. Es real y es pequeño."),

("P6", "¿Estamos optimizando la perilla que importa?", "🚨 HALLAZGO — Y ME PILLA A MI",
 "Acabo de gastar una hora en añadir DOS VISTAS del estado de bits, y salieron peor (+0,86% y "
 "+0,71%). Pero es que las dos vistas que añadi son vistas de UN BYTE — y en un byte no hay mas "
 "informacion por mirarlo de otra forma. Sus dos vistas VALIOSAS salen de los otros seis bytes.",
 "Medido: VC_VISTAS 0 -> 132.546 · 1 -> 133.690 · 2 -> 133.487 · 3 -> 134.520. La conclusion "
 "correcta no es «sus vistas no sirven» sino «he mirado el sitio donde no habia nada que ver»."),

("P7", "¿La justificacion se llego a medir?", "🚨 HALLAZGO — cuatro constantes de arquitectura sin barrer",
 "NMIX=4 y sus cuatro selectores · NSEL=[512,256,32,256] · NSEL2=16 · LIM=[41,40,12,6,5,4,4,3] "
 "de la tabla de estados · ISSE_CTX=[2,3,4,5,6,7,14]. Ninguna esta barrida. LIM viene de lpaq y "
 "NUNCA se re-midio aqui, que es justo lo que la regla de leer codigo de fuera prohibe.",
 "bench/constantes.tsv las tiene todas en clase «A OJO» o «HEREDADA». La arquitectura no es solo "
 "la forma: son tambien esos numeros, y estan sin contestar."),

("P8", "¿Hacemos el mismo trabajo dos veces con parametros distintos?", "YA CAZADO UNA VEZ, y por eso hay que volver",
 "El fichero ya documenta dos casos: los modelos que la cadena ISSE refina se tenian ADEMAS como "
 "modelo directo —pagar dos veces por la misma informacion— y la cadena de 13 etapas EMPEORABA. "
 "O sea que la arquitectura YA ha demostrado tener redundancia interna que costaba ratio.",
 "Si paso dos veces, la pregunta «¿que mas esta duplicado?» no esta cerrada."),

("P9", "¿Pagamos el caso general para servir al caso raro?", "HALLAZGO — al reves de lo que parece",
 "El 88,3% de nuestros ficheros no llega ni al trozo minimo, y la arquitectura esta afinada como "
 "si el caso normal fuera un bloque grande: 43 entradas al mezclador, 5 etapas de APM, cadena "
 "ISSE de 7. Todo eso tiene que CONVERGER, y converger cuesta datos.",
 "Es el mismo error que cometemos al comparar con paq8px: ellos miden en enwik8, un fichero "
 "grande. Nosotros no somos eso y nuestra arquitectura esta construida como si lo fueramos."),

("P10", "¿El numero viaja con su contexto?", "AVISO",
 "Las cifras de arquitectura de hoy son de 2 MB de codigo. El hueco contra paq8px va del 15,2% "
 "al 65,9% segun el tipo de dato: cualquier conclusion de arquitectura sacada de una sola "
 "muestra vale para esa muestra.", "bench/comparativa-completa.tsv."),

("P11", "¿Lo que medimos sigue siendo lo que creemos?", "🚨 HALLAZGO — LA PRUEBA DE LAS VISTAS ESTA CONFUNDIDA",
 "Añadir vistas hace DOS cosas a la vez: mete informacion nueva Y multiplica los pesos que hay "
 "que aprender. En un corpus donde el 88,3% de los ficheros es pequeño, lo segundo puede tapar "
 "lo primero enteramente. Lo que medi no es «¿aportan las vistas?» sino «¿aportan las vistas MAS "
 "el coste de arrancar sus pesos en frio?».",
 "Los dos efectos NO estan separados. Y el numero que salio (+0,86%) es del orden del coste de "
 "convergencia, no de una idea mala."),

("P12", "¿El instrumento conserva todo lo que la medida produjo?", "HALLAZGO — y van CINCO",
 "Ni las vistas, ni el anidamiento, ni la ablacion de paq8px, ni la memoria igualada pasaron por "
 "bench/mide.sh.",
 "⚠️ Cinco seguidas. Ya no es descuido: el carril del banco se escribe al final y tiene que "
 "escribirse al principio."),

("P13", "¿Quien comprueba la prueba?", "🚨 EL CONTROL QUE FALTA, Y EXISTE LA MAQUINA",
 "Para separar «la idea es mala» de «los pesos no han tenido tiempo de aprender» hay un control "
 "exacto: CORRER LAS VISTAS CON LOS PESOS YA ENTRENADOS. Si con pesos calientes las vistas pasan "
 "a ganar, el diagnostico es convergencia y el problema es de arquitectura —somos demasiado "
 "grandes para nuestros ficheros—, no de la idea.",
 "🔑 Y LA MAQUINA YA ESTA: `VC_LOAD_W` carga pesos pre-entrenados, y el propio fichero presume de "
 "ello («nadie lo hace: paq8px y zpaq inicializan a una constante»). Nunca se ha usado para "
 "contestar una pregunta de diseño."),

("P14", "¿Corremos TODAS nuestras suites?", "PENDIENTE",
 "MODELO paso de 4 a 5 con el anidamiento. La ida y vuelta esta comprobada en tres muestras; la "
 "suite del producto no.", "—"),

("P15", "¿Es esta la MEJOR prueba posible?", "NO — la mejor es UN BYTE, no dos vistas",
 "Lo que de verdad contesta si su diseño de ranura es mejor que el nuestro es añadir UN BYTE por "
 "contexto: la CUENTA DE RACHA del ultimo byte visto. Es la mas barata de sus tres vistas "
 "valiosas y la que engancha en el caso mas comun. Si un byte por contexto compra mas que dos "
 "vistas del byte que ya teniamos, la respuesta es que el problema era QUE RECORDAMOS, no COMO "
 "LO MIRAMOS.",
 "Cuesta 1 byte por contexto y nibble sobre 16 que ya hay: +6% de memoria de tablas."),

("P16", "¿Se puede medir SIN construir?", "SI, LAS DOS COSAS",
 "(1) El control de P13 no necesita codigo: `VC_LOAD_W` ya existe. (2) P3 —cuantos nodos del "
 "cubo se estrenan— se cuenta con un contador al final, no con un rediseño.",
 "Y las dos contestan preguntas de ARQUITECTURA, que es lo que el dueño acaba de pedir que "
 "dejemos de dar por bueno."),
]

with open("bench/camino-arquitectura.tsv", "w", newline="") as f:
    f.write("# EL CAMINO A NUESTRA PROPIA ARQUITECTURA (dueño, 01-08: «no podemos darla por perfecta»).\n")
    f.write("# 🚨 P4: guardamos 8 BITS por contexto y ellos 56. De sus siete vistas al mezclador, TRES\n")
    f.write("# salen de los seis bytes que NOSOTROS NO GUARDAMOS — y son las que enganchan en el caso\n")
    f.write("# mas comun. Yo añadi vistas del byte que YA teniamos: no habia nada mas que ver ahi.\n")
    f.write("# 🚨 P11/P13: la prueba de las vistas esta CONFUNDIDA —mide la idea Y el coste de arrancar\n")
    f.write("# sus pesos en frio, juntos— y el control existe sin escribir codigo: VC_LOAD_W.\n")
    w = csv.writer(f, delimiter="\t")
    w.writerow(["id", "la_pregunta", "veredicto", "que_se_encontro", "con_que_se_midio"])
    w.writerows(R)

c = collections.Counter(r[2].split("—")[0].strip() for r in R)
print(f"\nbench/camino-arquitectura.tsv: {len(R)} preguntas\n")
for r in R:
    print(f"  [{r[2][:42]:<42}] {r[0]:<4} {r[1]}")
print()
print("  " + " · ".join(f"{k}: {v}" for k, v in c.most_common()))
print()
print("  LO QUE DECIDE, y son tres:")
print()
print("  P4 — GUARDAMOS 8 BITS POR CONTEXTO Y ELLOS 56. No es que miren mejor el mismo dato: es")
print("  que recuerdan seis bytes mas —cuenta de racha y los tres ultimos bytes distintos— y de")
print("  ahi salen TRES de sus siete vistas. Las dos que yo añadi eran vistas del unico byte que")
print("  ya teniamos, o sea las dos mas flojas de las siete.")
print()
print("  P11/P13 — Y MI PRUEBA ESTABA CONFUNDIDA: añadir vistas mete informacion Y multiplica los")
print("  pesos por aprender, y el 88,3% de nuestros ficheros es pequeño. El +0,86% que sali')")
print("  publicando es del orden del coste de convergencia, no de una idea mala. El control")
print("  existe y no hay que escribir nada: VC_LOAD_W, pesos ya entrenados.")
print()
print("  P15 — LA MEJOR PRUEBA ES UN BYTE: la cuenta de racha del ultimo byte visto en ese")
print("  contexto. +6% de memoria de tablas. Si un byte compra mas que dos vistas del byte que")
print("  ya teniamos, el problema era QUE RECORDAMOS, no COMO LO MIRAMOS.")
