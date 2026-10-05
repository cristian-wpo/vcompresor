# -*- coding: utf-8 -*-
"""TODAS LAS CONSTANTES, Y POR QUE CADA UNA ES ESA Y NO OTRA.

NORMA DEL DUEÑO (01-08): «necesitamos una tabla con todas las constantes que utilizamos y
respondido por que esa constante es esa constante y no es otra, o por que no es variable».

POR QUE HACIA FALTA, y no es teorico: en UN SOLO DIA cuatro constantes resultaron estar mal
puestas y las cuatro por el mismo motivo — nadie habia contestado esta pregunta.
    · la ventana de vlz llevaba 4 MiB clavados con un comentario que decia «hay que medirla»
    · el carril media iba a nivel 3 por una premisa que nadie habia comprobado
    · PESO_LONG estaba en 5 porque lo barri sobre un corpus donde sql no estaba
    · el barrido del CDC solo probaba tallas MAS PEQUEÑAS que la vigente
Ninguna se caza leyendo el codigo: se cazan preguntandole a cada numero de donde sale.

ESTO ES UN INSTRUMENTO, NO UN DOCUMENTO. Extrae las constantes DE LA FUENTE y las cruza con la
tabla de respuestas; las que no tengan respuesta SALEN LISTADAS AL FINAL. Asi la tabla no puede
quedarse callada sobre una constante nueva: en cuanto alguien añada una, el instrumento la pide.

LAS SEIS CLASES DE «DE DONDE SALE»:
  FORMATO    la impone una especificacion de fuera (tablas de DEFLATE, enums de JXL). No es
             nuestra y cambiarla es romper el formato.
  IDENTIDAD  es una etiqueta: numeros magicos, ids de carril, indices de fase. Cualquier valor
             distinto vale igual; lo unico que importa es que no colisione.
  DERIVADA   sale de otra constante por aritmetica. No se elige: se calcula.
  MEDIDA     se barrio y hay numero. La fila dice cual y donde esta.
  HEREDADA   viene de otra implementacion y NO se ha re-medido aqui. ⚠️ Clase de riesgo: sus
             numeros estan afinados para SU codificador.
  A OJO      se eligio razonando y nunca se midio. ⚠️⚠️ La clase peligrosa: es donde estaban las
             cuatro de arriba.

Y LAS CUATRO DE «POR QUE NO ES VARIABLE»:
  DETERMINISMO  cambiarla cambia los bytes de salida, y el archivo tiene que ser IDENTICO en
                todas las maquinas. Es la razon dura.
  EN EL FLUJO   viaja dentro del archivo; cambiarla rompe lo ya archivado.
  ES PERILLA    de hecho SI es variable, por entorno. La fila dice cual.
  SIN MOTIVO    ⚠️ podria ser perilla y no lo es. Nadie lo ha decidido, simplemente se quedo fija.

Lo escribe bench/constantes.py y lee de crates/*/src/*.rs.
"""
import csv, os, re, subprocess, sys, collections

RAIZ = "."
FUENTES = []
for c in ("vc-core", "vc-cm", "vlz", "vc-cli", "vc-formato"):
    d = os.path.join(RAIZ, "crates", c, "src")
    if os.path.isdir(d):
        for r, _, fs in os.walk(d):
            for f in fs:
                if f.endswith(".rs"): FUENTES.append(os.path.join(r, f))
FUENTES.sort()

# ⚠️ PERILLAS TAMBIEN, Y ESTO ERA UN AGUJERO CON INCENTIVO PERVERSO.
#
# El regex solo cogia `const`. Pero cuando una constante hay que MEDIRLA se saca a perilla, y una
# perilla en Rust es `static X: LazyLock<..>` o `static X: AtomicUsize`, no `const`. O sea que el
# gesto de ponerse a medir una constante la BORRABA de la tabla que exige medirla.
#
# El 01-08 habia NUEVE fuera por esto —LIM, DMC_UMBRAL, DMC_UMBRAL2, VISTAS_EXTRA, RACHA_ON,
# TRES_ON, ESTIMA_HASH, TOOFAR_EN_VORAZ, ALCANCE_ZLIB— y todas eran precisamente las que se
# habian tocado ese dia. La tabla decia «166 de 166 contestadas» sin ellas dentro.
# Y de `static`, SOLO las que son «se calcula una vez y ya no cambia» —LazyLock y OnceLock—.
# Un `static X: AtomicU64` NO es una constante: es un contador o un estado que cambia en marcha
# (los C_* del inflate, los PROG_* del progreso, HASH_LD que el oraculo mueve por flujo). Meterlos
# llenaria la tabla de filas que no tienen ningun «por que ese valor» que contestar, y una tabla
# con ruido se deja de leer.
RE = re.compile(
    r"^\s*(?:pub\s+)?(?:"
    r"const\s+([A-Z][A-Z0-9_]*)\s*:\s*([^=]+?)"
    r"|static\s+([A-Z][A-Z0-9_]*)\s*:\s*([^=]*(?:LazyLock|OnceLock)[^=]*?)"
    r")\s*=\s*(.+?);?\s*$")
halladas = {}
for f in FUENTES:
    for i, l in enumerate(open(f, encoding="utf-8", errors="ignore"), 1):
        m = RE.match(l)
        if m:
            nom = m.group(1) or m.group(3)
            tipo = (m.group(2) or m.group(4) or "").strip()
            val = m.group(5).strip()
            if len(val) > 46: val = val[:43] + "…"
            # ⚠️ COLISIONES DE NOMBRE. Esto era `halladas.setdefault(nom, ...)`, o sea indexado
            # SOLO POR NOMBRE, y con `setdefault` la primera ocurrencia ganaba y las demas
            # desaparecian sin decir nada. `MIN_COPIA` existe en vlz (=4) y en vc-fase3 (=3), y
            # como los ficheros van ordenados salia la POSICION de vc-fase3 con el VALOR 3 pegada
            # a la respuesta escrita para el 4 de vlz. Una fila que se lee como autoridad y esta
            # cruzada. Justo el fallo que este instrumento existe para no cometer.
            halladas[(os.path.relpath(f, RAIZ), nom)] = (os.path.relpath(f, RAIZ), i, tipo, val)

# ── LAS RESPUESTAS ──────────────────────────────────────────────────────────────────────────
# (nombre, de_donde_sale, por_que_ese_valor, por_que_no_es_variable)
R = {
# ---------- las que HOY han resultado estar mal puestas, primero ----------
"PESO_LONG": ("MEDIDA",
 "4. Barrida el 01-08 sobre CINCO tipos: 2→6.899.695 · 3→6.886.454 · 4→6.886.153 · 5→6.888.826 "
 "en codigo. ⚠️ Estuvo en 5 doce horas porque la primera barrida uso codigo+csvjson+logs y SQL NO "
 "ESTABA DENTRO: en sql el 4 y el 5 se llevan un ESCALON del 10,3% (3.239.982 contra 3.572.586). "
 "El 3 es el de zstd (zstd_lazy.c:1635) y no es el nuestro.",
 "SIN MOTIVO — podria ser perilla. No se ha hecho porque gobierna DOS decisiones a la vez "
 "(elegir coincidencia y aceptar repcode) y separarlas se probo y no compensa."),
"NREPS": ("MEDIDA",
 "8. zstd usa 3 y LZMA 4; barrido el 01-08: 3→10.008.461, 4→9.982.977, 6→9.951.570, 8→9.938.487, "
 "16→9.913.084 (esf 6). Sigue bajando hasta 16 pero de 8 a 16 solo compra 0,25% y cuesta 2,9% de "
 "reloj. La rodilla es 8. EL PORQUE DE QUE PODAMOS MAS QUE ELLOS ES ESTRUCTURAL: su FSE transmite "
 "tabla y cada repcode le ocupa sitio; nuestro aritmetico adaptativo cobra lo que la probabilidad "
 "diga, y un repcode que no se usa no cuesta casi nada.",
 "DETERMINISMO — el descompresor tiene que tener las mismas ranuras."),
"VENTANA_TOPE": ("MEDIDA",
 "32 MiB. Estuvo en 4 MiB clavados con un comentario que decia «es una perilla y hay que "
 "medirla» — los MISMOS 4 MiB que zstd, encontrados el mismo dia en su codigo. Subirla y "
 "dimensionar la ventana A LA ENTRADA dio −6,077%.",
 "DERIVADA de la memoria: `prev` es un u32 por posicion, o sea 32 Mi × 4 B = 128 MB por hilo. "
 "🔑 MEDIDO el 01-08 (bin/vlz-saltos) tras preguntarlo el dueño: 4 bits es imposible -16 valores "
 "no direccionan 33 millones de posiciones- y 16 TAMPOCO SIRVE, porque solo conserva entre el "
 "50,43% (csvjson) y el 81,48% (sql) de los BYTES AHORRADOS. Con 24 bits se conserva entre el "
 "98,26% y el 99,92% en los cinco tipos, y la cadena pasa de 128 a 96 MB por hilo. Esa es la "
 "unica reduccion viable, y su coste de RELOJ (ranuras empaquetadas de 3 bytes, lecturas no "
 "alineadas) NO esta medido."),
"CDC_MIN_DEF": ("MEDIDA",
 "256 KiB. Barrido el 01-08 sobre 755 MB: 256/1024/4096 KiB da 316.715.399 B contra 316.808.436 "
 "del 64/256/1024 anterior (−0,0294%) y −0,7% de reloj. ⚠️ El barrido SOLO BAJABA hasta ese dia: "
 "las cinco tallas que probaba eran mas pequeñas que la vigente, asi que solo podia confirmarla, "
 "nunca refutarla por arriba — y el optimo estaba justo por arriba.",
 "ES PERILLA (VC_CDC_MIN). ⚠️ Pero cambiarla rompe la dedup contra lo ya archivado: dos "
 "instantaneas con cortes distintos no deduplican entre si."),
"CDC_AVG_DEF": ("DERIVADA", "1024 KiB = 4 × CDC_MIN_DEF. La proporcion 1:4:16 se mantuvo en todo "
 "el barrido; lo que se barrio fue la ESCALA, no la forma.", "ES PERILLA (VC_CDC_AVG)."),
"CDC_MAX_DEF": ("DERIVADA", "4096 KiB = 16 × CDC_MIN_DEF. Ver CDC_AVG_DEF.", "ES PERILLA (VC_CDC_MAX)."),
"MIN_COPIA": ("A OJO",
 "⚠️ 4, con el comentario «por debajo de 4 casi nunca compensa». NUNCA SE HA BARRIDO. Y NO ES "
 "INOCENTE: `hash4` lee 4 bytes, asi que bajar a 3 obliga a cambiar el hash — que es justo lo que "
 "distingue a zlib (3 bytes) de libdeflate (4). El censo dice que el 33,6% de nuestro banco de "
 "DEFLATE lo hizo un codificador de 4.",
 "SIN MOTIVO — nadie lo ha decidido."),
"MAX_COPIA": ("MEDIDA A MEDIAS",
 "65.536. Se subio desde 515 al meter el escape de longitud. ⚠️ P9 del camino de vlz lo señala "
 "sin arreglar: `largo_comun` puede recorrer hasta ahi por CADA candidato y las copias larguisimas "
 "son rarisimas. Cortar en 64 da +34% de velocidad y cuesta +4,72% de bytes: es una perilla que "
 "no existe, no un arreglo.",
 "SIN MOTIVO."),
"CUBOS": ("A OJO",
 "⚠️ 2^20 en vlz. Nunca barrido. Lo que SI se arreglo el 31-07 es que el desplazamiento del hash "
 "estaba escrito a mano (17) en vez de derivarse de CUBOS, asi que subir CUBOS reservaba memoria "
 "y no usaba ni un bit mas — se veia en que la salida salia IDENTICA byte a byte.",
 "SIN MOTIVO."),
"LOG_CUBOS": ("DERIVADA", "`CUBOS.trailing_zeros()`. Se derivo a proposito el 31-07: antes era un "
 "17 escrito a mano y por eso CUBOS no hacia nada.", "DERIVADA."),
# ---------- el archivador ----------
"MAGIC": ("IDENTIDAD", "b\"VCMP0016\". Cualquier ocho bytes valen; lo que importa es que no "
 "colisione y que la version suba cuando cambia la DISPOSICION del archivo.", "EN EL FLUJO."),
"CONTENEDORES_LEIBLES": ("IDENTIDAD", "Las cinco versiones que sabemos abrir. El 0012 se conserva "
 "a proposito: un archivo de entonces se abre igual, solo pierde poder recibir instantaneas encima.",
 "EN EL FLUJO."),
"CODEC_STORE": ("IDENTIDAD", "0/1/2 en el flujo. Etiquetas.", "EN EL FLUJO."),
"CODEC_ZSTD": ("IDENTIDAD", "Ver CODEC_STORE.", "EN EL FLUJO."),
"CODEC_CM": ("IDENTIDAD", "Ver CODEC_STORE.", "EN EL FLUJO."),
"LANE_TEXT": ("IDENTIDAD", "Ver CODEC_STORE.", "EN EL FLUJO."),
"LANE_MEDIA": ("IDENTIDAD", "Ver CODEC_STORE.", "EN EL FLUJO."),
"LANE_OTHER": ("IDENTIDAD", "Ver CODEC_STORE.", "EN EL FLUJO."),
"HUECO_DEFLATE": ("IDENTIDAD", "Etiqueta de tipo de hueco en el flujo.", "EN EL FLUJO."),
"HUECO_CRUDO": ("IDENTIDAD", "Ver HUECO_DEFLATE.", "EN EL FLUJO."),
"HUECO_JXL": ("IDENTIDAD", "Ver HUECO_DEFLATE.", "EN EL FLUJO."),
"M_SIN": ("IDENTIDAD", "Marca de metodo en el flujo.", "EN EL FLUJO."),
"M_ESC": ("IDENTIDAD", "Ver M_SIN.", "EN EL FLUJO."),
"FRAME_TARGET": ("MEDIDA",
 "64 MiB. Barrido el 31-07 y ⚠️ LA CURVA SE DIO LA VUELTA al arreglar la ventana: 256 MiB YA GANA "
 "en bytes (−0,493%). Se queda en 64 por OTRA razon —+43% de RAM y 4× la granularidad de la "
 "dedup—, no porque comprima mejor. Eso hay que decirlo cada vez que se cite.",
 "ES PERILLA (--solid)."),
"LANES": ("DERIVADA", "3, que son los carriles que hay. No es una eleccion.", "DERIVADA."),
"STORE_PCT": ("A OJO", "⚠️ Umbral de «esto no comprime, guardalo tal cual». Nunca barrido.", "SIN MOTIVO."),
"PREFLATE_MAX": ("A OJO", "⚠️ Tope de tamaño para intentar preflate. Nunca barrido. Su motivo es "
 "de reloj, no de bytes, y el reloj de preflate cambio −25,9% el 31-07 al quitarle la doble "
 "verificacion: el tope se eligio ANTES de eso.", "SIN MOTIVO."),
"JXL_MAX_JPEG": ("A OJO", "⚠️ Tope de tamaño para transcodificar a JXL. Nunca barrido.", "SIN MOTIVO."),
"JXL_ESFUERZO": ("HEREDADA", "⚠️ El esfuerzo del codificador JXL. Es SU escala, no la nuestra, y "
 "no se ha barrido aqui. El carril jpeg compra 3,933% y cuesta el 35,37% de la CPU de "
 "transformada: es el sitio donde una perilla mal puesta se nota.", "SIN MOTIVO."),
"JXL_LOTE": ("A OJO", "⚠️ Cuantos JPEG por lote. Nunca barrido.", "SIN MOTIVO."),
"MIN_TRAMO": ("A OJO", "⚠️ Tramo minimo para que un hueco merezca la pena. Nunca barrido.", "SIN MOTIVO."),
"SKETCH_K": ("HEREDADA", "⚠️ K de la huella min-hash del agrupador. De la literatura de "
 "deduplicacion, no re-medido. El agrupador entero vale 0,108% y cuesta 8% de reloj, asi que "
 "afinar su K tiene poco techo — pero eso tambien es una medida que falta.", "SIN MOTIVO."),
"SKETCH_MUESTRA": ("HEREDADA", "⚠️ Cuanto se muestrea para la huella. Ver SKETCH_K.", "SIN MOTIVO."),
"GEAR": ("HEREDADA", "La tabla gear del troceador por contenido, de la literatura de FastCDC. "
 "Es una tabla de 256 valores pseudoaleatorios: lo que importa es que esten bien repartidos, no "
 "sus valores concretos.", "DETERMINISMO — cambiarla cambia TODOS los cortes."),
"TROZOS_MUESTRA": ("A OJO", "⚠️ Cuantos trozos se muestrean para decidir el carril. Nunca barrido.", "SIN MOTIVO."),
"TS_LARGO": ("FORMATO", "Longitud del sello de tiempo en el flujo.", "EN EL FLUJO."),
"CABECERA": ("FORMATO", "Tamaño de la cabecera del contenedor.", "EN EL FLUJO."),
"X86_PERMITE": ("A OJO", "⚠️ Que opcodes x86 acepta el filtro de codigo maquina. Elegido leyendo "
 "el manual, nunca barrido contra el corpus.", "DETERMINISMO."),
"X86_BIT": ("DERIVADA", "Mascara que acompaña a X86_PERMITE.", "DETERMINISMO."),
"VELOCIDAD_NIVEL": ("MEDIDA", "Tabla de reloj por nivel, usada por --price. Medida en esta maquina.",
 "SIN MOTIVO — deberia leerse del banco, no estar clavada."),
"EUR_HORA_DEFECTO": ("A OJO", "⚠️ Precio de la hora de maquina para el calculo en euros. Es una "
 "suposicion de mercado, no una medida — y el propio metodo dice que el dinero es quien decide.",
 "ES PERILLA (--eur-hora)."),
"FASES": ("IDENTIDAD", "Los nombres de las 14 fases del desglose.", "DERIVADA."),

# ── CUARTA TANDA: las perillas, que hasta hoy la tabla no veia ──────────────────────────────
# El regex solo cogia `const`, asi que sacar una constante a perilla —que es LO QUE SE HACE PARA
# MEDIRLA— la borraba de la tabla que exige medirla. Nueve estaban fuera por eso, y todas eran
# justo las que se habian tocado el 01-08.
"VISTAS_EXTRA": ("MEDIDA",
    "0 = las dos vistas extra por contexto APAGADAS. Se midieron y perdieron: +1,386% en codigo a "
    "2 MB, y la penalizacion se estabiliza en 0,7-0,8% al crecer el dato (2/8/32 MB dan "
    "+1,386/+0,719/+0,828%), o sea que pierden por la idea y no por arrancar en frio. ⚠️ El "
    "defecto estuvo en 3 —encendidas— DESPUES del veredicto, y el producto comprimia 1,387% peor.",
    "ES PERILLA (VC_VISTAS), pero el defecto es una decision medida."),
"RACHA_ON": ("MEDIDA",
    "true. La ranura recuerda el ultimo byte y su racha: -0,542% sobre seis tipos, y en ninguno "
    "pierde. Cuesta +37,95% de ciclos. La confianza NO la fija un multiplicador sino un MAPA "
    "APRENDIDO, que gano al optimo del barrido (130.901 contra 131.110).",
    "ES PERILLA (VC_RACHA)."),
"TRES_ON": ("MEDIDA",
    "true. Los dos bytes alternos (2o y 3o distintos): -0,117% adicional sobre seis tipos, seis "
    "de seis en negativo, y en logs compran LO MISMO que la racha. ⚠️ No cuestan ni un byte de "
    "RAM —los cuatro bytes de la ranura se reservan juntos y el interruptor solo apaga el uso— "
    "pero si +17,26% de ciclos: 26,5 Mciclos por byte ganado contra 8,2 de la racha, un trato 3,2 "
    "veces peor. Si algun dia hay que recortar reloj, se empieza por aqui.",
    "ES PERILLA (VC_TRES)."),
"ESTIMA_HASH": ("MEDIDA",
    "true. El hash lo estima preflate por flujo en vez de suponer el de zlib para todos: +6,04 "
    "puntos en los flujos de libdeflate y +0,00 EXACTOS en los de zlib (el control). Cuesta cero "
    "de reloj y 227 MB de RSS. Manda 609 de 1.973 flujos a libdeflate, que son exactamente los "
    "609 que el censo identifica.",
    "ES PERILLA (VC_ESTIMA_HASH)."),
"TOOFAR_EN_VORAZ": ("FORMATO",
    "false, o sea que TOO_FAR se aplica SOLO al carril perezoso, que es donde lo tiene zlib "
    "(deflate.c:1965, dentro de deflate_slow). Aplicarlo tambien a los tres niveles voraces "
    "costaba 1,99 puntos en ese carril. La perilla existe para poder medir la diferencia.",
    "ES PERILLA (VC_TOOFAR), pero el valor lo dicta zlib."),
"ALCANCE_ZLIB": ("FORMATO",
    "true: se usa MAX_DIST = 32.506 y no la ventana entera, que es lo que hace zlib "
    "(deflate.h:293-298). Vale +0,04 p en perezoso y +0,03 en voraz — poco, pero mueve los DOS "
    "carriles, que es la firma que confirma el mecanismo.",
    "ES PERILLA (VC_MAXDIST), pero el valor lo dicta zlib."),
"E": ("IDENTIDAD",
    "El OnceLock local de una perilla: guarda el valor leido del entorno para no volver a "
    "consultarlo. No es una constante con valor propio, es el sitio donde se cachea otra.",
    "DERIVADA."),
"P": ("IDENTIDAD",
    "OnceLock de una perilla, igual que E.", "DERIVADA."),
"FIJOS": ("FORMATO",
    "Las tablas de Huffman FIJAS de DEFLATE (RFC 1951, seccion 3.2.6). Estan escritas en la "
    "especificacion: no hay nada que elegir.", "EN EL FLUJO."),

# ── TERCERA TANDA: las veinte que quedaban ──────────────────────────────────────────────────
"MIN_MIRADA": ("FORMATO",
    "MAX_COPIA + MIN_COPIA + 1 = 262, el MIN_LOOKAHEAD de zlib (deflate.h:293). Es lo que zlib "
    "necesita tener por delante: una coincidencia maxima entera (258) mas los tres bytes que "
    "insertar despues.", "DERIVADA."),
"MAX_DIST": ("FORMATO",
    "VENTANA - MIN_MIRADA = 32.506, el MAX_DIST de zlib (deflate.h:298). ⚠️ Hasta el 01-08 "
    "usabamos la ventana entera (32.768) y prediciamos coincidencias que el codificador NO PODIA "
    "producir. El primer candidato llega a 32.506 y los demas a 32.505, porque la cabeza se "
    "comprueba con <= y el recorrido para con un > ESTRICTO.", "DERIVADA."),
"POL_DE_NIVEL": ("FORMATO",
    "La politica de diccionario de cada nivel, sacada de deflate.h:189 —«max_insert_length is "
    "used only for compression levels <= 3»— y de la rama incondicional de deflate.c:1872. No se "
    "elige: la determina el nivel. Niveles 1-3 AddFirst(4/5/6), 4-9 AddAll.",
    "DERIVADA de CFGS."),
"TAB_BITS": ("A OJO",
    "⚠️ Los bits de direccion de cada tabla de contexto, o sea CUANTA MEMORIA se le da a cada "
    "modelo. Nunca barridos uno a uno; el 20 es el valor por defecto que se repite en casi todos "
    "y el 10/14/12 son los pocos que alguien decidio achicar. Es la constante con mas memoria "
    "detras de todo el codec y esta sin medir.", "ES PERILLA (VC_MEM la escala entera)."),
"TAB_TOPE": ("A OJO",
    "⚠️ El tope de cada tabla, en pareja con TAB_BITS y con el mismo problema.",
    "ES PERILLA (VC_MEM)."),
"DMC_NODOS": ("A OJO",
    "⚠️ 1<<22 estados del DMC, unos 48 MB. Elegido para que quepa; nunca se ha medido si con la "
    "mitad pierde algo.", "DETERMINISMO."),
"DMC_UMBRAL": ("MEDIDA",
    "4. Barrida el 01-08 en once configuraciones (de 128/256 a 2/4) y verificada en los seis "
    "tipos: -0,121% sobre el total y -0,703% en sql contra el 64 de la literatura clasica. "
    "🔑 Lo que valida el barrido no es el numero sino que LA CURVA SE DA LA VUELTA: baja de forma "
    "monotona hasta 4/8 y en 2/4 vuelve a subir en sql. Sin ese punto de retorno solo diria «mas "
    "abajo es mejor». Tiene explicacion fisica: con umbrales muy bajos el arbol clona sin parar y "
    "se queda sin nodos (DMC_NODOS=4M) antes de acabar el fichero. ⚠️ Pierde en logs (+0,163%), "
    "donde el DMC no aporta nada: el rango entero del barrido son 236 bytes de 119.000.",
    "DETERMINISMO — sale a perilla (VC_DMC_U1) solo para barrerla."),
"DMC_UMBRAL2": ("MEDIDA",
    "8, barrida en pareja con DMC_UMBRAL. Y la pareja IMPORTA: (32/128) es peor que (32/64) en "
    "codigo y sql pero mejor en logs, asi que barrer una sola no vale.",
    "DETERMINISMO — perilla VC_DMC_U2 solo para barrer."),
"DUPLICADOS": ("DERIVADA",
    "La mascara de los modelos que la cadena ISSE ya refina y que por tanto no hace falta tener "
    "ADEMAS sueltos. Sale de ISSE_CTX, no se elige aparte.", "DERIVADA."),
"PODA_V2": ("DERIVADA",
    "La mascara de que modelos poda la version 2. Sale de la lista de modelos.", "EN EL FLUJO."),
"PODA_DESDE": ("A OJO",
    "⚠️ 8 MiB: a partir de que tamano de entrada se poda. Elegido a ojo; nunca se ha mirado si "
    "la rodilla esta ahi.", "DETERMINISMO."),
"SONDA": ("A OJO",
    "⚠️ 128 KiB de sonda para decidir el carril. Nunca barrida: si sondear menos decide igual, "
    "es reloj tirado; si sondear mas decide mejor, son bytes tirados.",
    "SIN MOTIVO."),
"MINIMO_PARA_SONDA": ("A OJO",
    "⚠️ 64 MiB: por debajo de esto no se sondea, se decide directo. Razonado (sondear un fichero "
    "pequeno cuesta una fraccion grande de comprimirlo entero) pero no medido.",
    "SIN MOTIVO."),
"SOLIDO_DEF": ("MEDIDA",
    "64 MiB de bloque solido por defecto. Medido el 31-07: la curva de frames se dio la vuelta y "
    "256 MiB ya gana en bytes (-0,493%), pero el 64 se queda por OTRA razon —+43% de RAM y cuatro "
    "veces peor granularidad para el acceso aleatorio—. La decision no es solo de bytes.",
    "ES PERILLA (--solid)."),
"BLOQUE": ("A OJO",
    "⚠️ 64 KiB de bloque de lectura. Nunca medido contra 32 ni 128.", "SIN MOTIVO."),
"CACHE_FRAMES": ("A OJO", "⚠️ 24 frames en cache. Elegido a ojo.", "SIN MOTIVO."),
"FRAME_CACHE": ("A OJO",
    "⚠️ 8 frames en la cache del sistema de ficheros FUSE. Distinto del 24 de CACHE_FRAMES y sin "
    "nota que explique por que dos caches de lo mismo llevan numeros distintos.", "SIN MOTIVO."),
"MATERIAL_CACHE_MAX": ("A OJO",
    "⚠️ 512 MiB de tope de cache de material en FUSE. Elegido a ojo.", "SIN MOTIVO."),
"TTL": ("A OJO",
    "⚠️ 3600 s de TTL de los atributos que FUSE devuelve al nucleo. Una hora es lo que se suele "
    "poner cuando el contenido es inmutable —y el nuestro lo es—, pero no se ha comprobado que "
    "el nucleo no este preguntando de mas.", "SIN MOTIVO."),
"NIVEL": ("IDENTIDAD",
    "19, el nivel de zstd con que vc-rechazados comprime lo que el carril rechaza. Es una "
    "herramienta de diagnostico: se puso el maximo para que el rechazo no se pueda achacar a "
    "haber comprimido flojo.", "SIN MOTIVO."),

# ── SEGUNDA TANDA (01-08 tarde) ─────────────────────────────────────────────────────────────
# Las que se contestan LEYENDO la fuente. Aqui no hay ninguna medida nueva: se dice de que clase
# es cada una y por que, que es exactamente lo que la norma pide. Las que siguen sin respuesta
# despues de esto son las que de verdad hacen falta medir.

# --- lo que impone DEFLATE, y no es nuestro ---
# -- LAS DEL 02-08, el dia de leer preflate ---------------------------------------------------
# Norma: si una perilla existe es porque su valor se MIDIO. El defecto es el ganador de esa
# medida, no lo que habia antes.
"TOPE_CADENA_FIJO": ("DERIVADA",
    "Sin fijar: el tope de cadena lo pone el estimador por flujo. Solo se fija a mano para "
    "ablacionar. Con la configuracion del estimador se usa SIEMPRE su cadena; con los nueve "
    "presets solo cuando dice menos de 4, porque por encima la cadena es lo que los distingue.",
    "ES PERILLA - VC_TOPE_CADENA."),
"VOLCADO": ("MEDIDA",
    "Apagado por defecto: escribe el flujo de correcciones para poder comprimirlo con OTRO "
    "codificador y saber si compensa cambiarlo, SIN tener que integrarlo antes. Es la pregunta "
    "P16 del camino. Con el se midio que xz -6 da 1,140% de bytes REALES -por debajo de nuestro "
    "modelo entropico ideal- y que vc-cm da 1,027% pero cuesta 66,3 s contra 2,05.",
    "ES PERILLA - VC_VOLCADO=<ruta>."),
"CFG_ESTIMADOR": ("MEDIDA",
    "Encendida. Coger los cinco parametros que el estimador ya calculaba -cadena, nice, "
    "good_length, max_lazy y politica- en vez de adivinar entre nueve presets: 96,26% en 1.388 s "
    "-> 96,69% en 160,6 s. Mas acierto Y 8,6 veces menos reloj.",
    "ES PERILLA - VC_CFG_ESTIMADOR=0 vuelve a los nueve presets, que es como se mide el techo."),
"DIST3_ESTIMADOR": ("MEDIDA",
    "Encendida. Usar el max_dist_3_matches del estimador en vez de DEMASIADO_LEJOS=4096 a fuego: "
    "2,101% -> 1,055%. UNA constante a ojo cargaba el hueco entero contra preflate. 453 flujos "
    "del banco emiten copias de tres bytes mas alla de 4 KiB.",
    "ES PERILLA - VC_DIST3=0 para ablacionar."),
"ORDEN_ZLIB": ("MEDIDA",
    "Encendida. deflate_slow inserta strstart ANTES de longest_match, asi que su mirada perezosa "
    "ve el candidato de pos; la nuestra insertaba al avanzar y no lo veia. 97,79% -> 97,84%, "
    "56.000 fallos menos. No ahorra reloj: es una divergencia de simulacion, no una optimizacion.",
    "ES PERILLA - VC_ORDEN_ZLIB=0."),
"CACHE_PEREZOSA": ("MEDIDA",
    "Encendida, PERO SOLO CUANDO r0 < buena. Guardando siempre perdia 303.287 aciertos: el valor "
    "de la mirada sale de una cadena reducida y la busqueda siguiente no reduce. Cuando "
    "r0 < buena ninguna de las dos reduce y son la MISMA llamada: -616 millones de pasos de "
    "cadena con la salida identica byte a byte.",
    "ES PERILLA - VC_CACHE=0. El negativo de la version sin condicion queda escrito al lado."),
"SALTOS_ON": ("MEDIDA",
    "Encendida. Cobra las correcciones con la estructura de preflate -distancia en SALTOS de "
    "cadena, rachas en vez de marca por simbolo, un canal por categoria- y da 3,035% -> 2,218% "
    "sin cambiar una prediccion. Cuesta el 2,1% de los pasos de cadena, medido.",
    "ES PERILLA - VC_SALTOS=0 vuelve al modelo plano."),
"REPREDICT": ("MEDIDA",
    "Encendida. Cuando predecimos literal y era copia, se vuelve a buscar con la cadena entera "
    "antes de corregir, como su repredict_reference. 1,169% -> 1,166%: POSITIVO PERO CASI NULO, "
    "0,003 puntos. Se queda porque suma y no cuesta; como palanca no era.",
    "ES PERILLA - VC_REPREDICT=0."),
"EST_LIMITE_N": ("MEDIDA",
    "200.000 tokens. El barrido de profundidad del estimador mantiene la tabla de hash de CADA "
    "candidato en CADA posicion del flujo entero: 17,6 s de 59,1. Cortado a 200.000 el acierto "
    "sale IDENTICO (97,84%), el coste sube 0,001 puntos y la produccion baja a 53,3 s - 551 "
    "EUR/PB-ano. Por debajo duele: 50.000 cuesta 0,027 puntos y 5.000 cuesta 1,669.",
    "ES PERILLA - VC_EST_LIMITE. Cortar SUBESTIMA max_chain, que es la direccion segura."),
"ATAJO_K": ("MEDIDA",
    "1. Cuantas configuraciones sobreviven al prefijo. K=2 acierta 0,06 puntos mas y cuesta 19% "
    "mas de reloj: en euros gana K=1, porque el reloj es donde se decide. Y este atajo dejo de "
    "importar cuando la configuracion la da el estimador: solo corre una.",
    "ES PERILLA - VC_ATAJO_K."),
"PREFIJO_FIJO": ("DERIVADA",
    "Sin fijar: el prefijo es plano/5 acotado a [64 KiB, 512 KiB]. Se midio prefijo x4 y compra "
    "menos que guardar una candidata mas (K=2), asi que no se toca.",
    "ES PERILLA - VC_PREFIJO."),
"MIN_EMIT_FIJO": ("DERIVADA",
    "Sin fijar: manda el min_len del estimador, que es el minimo sobre TODOS los tokens emitidos "
    "del flujo. Fijarlo a mano solo sirve para ablacionar.",
    "ES PERILLA - VC_MIN_EMIT."),
"TECHO_DIEZ": ("MEDIDA",
    "Apagada por defecto: solo sirve para MEDIR el techo, y anade una pasada. Con ella el suelo "
    "baja de 1,55% (nueve presets) a 1,19% (las diez), porque la del estimador tiene parametros "
    "continuos que ningun preset expresa.",
    "ES PERILLA - VC_TECHO_DIEZ=1."),
"CALIBRA": ("MEDIDA",
    "1,502/1,111 = 1,352. Lo que nuestro modelo de coste se queda CORTO frente a bytes reales, "
    "medido cobrando LA SALIDA DE preflate con NUESTRO contable. Se aplica al nuestro para poder "
    "comparar un modelo con unos bytes. HOY YA NO HACE FALTA para el numero publicado: el "
    "volcado + xz -6 da bytes reales (1,140%). Se conserva porque el modelo se sigue imprimiendo.",
    "DERIVADA de dos medidas - deja de valer si cambia el modelo de coste."),
"NCAT": ("FORMATO",
    "5 canales, los que de verdad emite predict_block por token: LiteralPredictionWrong, "
    "ReferencePredictionWrong, LenCorrection, DistOnlyCorrection y DistAfterLenCorrection. No es "
    "una eleccion nuestra: es su lista.",
    "DETERMINISMO - si anaden un canal, cambia."),
"CAT_LIT": ("IDENTIDAD", "El indice del canal LiteralPredictionWrong. Un nombre, no un numero.",
    "DETERMINISMO."),
"CAT_REF": ("IDENTIDAD", "El indice del canal ReferencePredictionWrong.", "DETERMINISMO."),
"CAT_LONG": ("IDENTIDAD", "El indice del canal LenCorrection.", "DETERMINISMO."),
"CAT_DIST": ("IDENTIDAD", "El indice del canal DistOnlyCorrection.", "DETERMINISMO."),
"CAT_DAL": ("IDENTIDAD", "El indice del canal DistAfterLenCorrection.", "DETERMINISMO."),
"CAT_NOMBRE": ("IDENTIDAD",
    "Los nombres de los cinco canales para imprimirlos. Van al lado de los indices a proposito: "
    "una segunda lista en otro sitio se desincroniza.",
    "DETERMINISMO."),
"LB": ("FORMATO",
    "Las 29 bases de longitud de DEFLATE (RFC 1951). Se usan para DERIVAR el simbolo canonico y "
    "comprobar que sim_long no hacia falta: 0 no derivables sobre 44,6 millones de copias.",
    "EN EL FLUJO - las define el formato."),
"DB": ("FORMATO",
    "Las 30 bases de distancia de DEFLATE (RFC 1951). Particionan 1..32768 sin ambiguedad, y por "
    "eso sim_dist es derivable SIEMPRE: 0 no derivables sobre 44,6 millones de copias.",
    "EN EL FLUJO - las define el formato."),

"crates/vc-core/src/bin/vc-fase3.rs:VENTANA": ("FORMATO",
    "32 KiB, la ventana de DEFLATE. Una distancia no cabe en mas de 15 bits.",
    "EN EL FLUJO."),
"crates/vc-core/src/bin/vc-prediccion.rs:VENTANA": ("FORMATO",
    "32 KiB, la ventana de DEFLATE. La misma que en vc-fase3 y por lo mismo.",
    "EN EL FLUJO."),
"MASCARA": ("DERIVADA", "VENTANA - 1. La ventana es potencia de dos, asi que el modulo es un AND.",
    "DERIVADA."),
"MASCARA_ZLIB": ("FORMATO",
    "(1<<15)-1, la mascara de la tabla de hash de zlib. Es SUYA: simulamos su cadena, asi que "
    "tiene que ser su tamano. ⚠️ Y de aqui salio el handicap medido el 01-08: el hash de "
    "libdeflate da 16 bits y esta tabla tiene 15, o sea que dos cubos suyos colisionan en uno "
    "nuestro. Se midio y vale +0,03 puntos: despreciable.",
    "EN EL FLUJO."),
"crates/vc-core/src/bin/vc-fase3.rs:MAX_COPIA": ("FORMATO",
    "258, el MAX_MATCH de DEFLATE. El codigo de longitud no sabe expresar mas.", "EN EL FLUJO."),
"crates/vc-core/src/bin/vc-prediccion.rs:MAX_COPIA": ("FORMATO",
    "258, el MAX_MATCH de DEFLATE.", "EN EL FLUJO."),
"crates/vc-core/src/bin/vc-prediccion.rs:MIN_COPIA": ("FORMATO",
    "3, el MIN_MATCH de DEFLATE.", "EN EL FLUJO."),
"CFGS": ("FORMATO",
    "La configuration_table de zlib, copiada literal de deflate.c:93 con sus cuatro columnas "
    "(good_length, max_lazy, nice_length, max_chain). No se elige nada: es la tabla que define "
    "los nueve niveles que intentamos identificar.",
    "EN EL FLUJO — es de zlib."),
"N": ("DERIVADA", "9, los niveles de zlib. Sale de CFGS.", "DERIVADA."),

# --- indices dentro del vector de contextos: aritmetica, no eleccion ---
"IDMC": ("DERIVADA", "NTAB. La ranura del DMC va justo detras de las tablas.", "DERIVADA."),
"IISSE": ("DERIVADA", "NTAB+3. Ranura de la salida de la cadena ISSE.", "DERIVADA."),
"IMM4": ("DERIVADA", "NTAB+5. Ranura del match de zancada 4.", "DERIVADA."),
"IMM8": ("DERIVADA", "NTAB+6. Ranura del match de zancada 8.", "DERIVADA."),
"IDISP": ("DERIVADA", "NTAB+7. Ranura de la dispersion.", "DERIVADA."),
"IDISP2": ("DERIVADA", "NTAB+8. Ranura de dispersion x media.", "DERIVADA."),
"IRACHA": ("DERIVADA", "NTAB+9. Donde empiezan las NTAB vistas de racha, una por contexto.",
    "DERIVADA."),
"ITRES": ("DERIVADA", "NTAB+9+NTAB. Detras de las de racha.", "DERIVADA."),
"IDET": ("DERIVADA", "NTAB+9+NTAB+NTAB. Detras de las de los bytes alternos.", "DERIVADA."),
"NCTX": ("DERIVADA",
    "NTAB+9+NTAB+NTAB+NTAB: las tablas, los nueve modelos sueltos y las TRES familias de vistas "
    "por contexto (racha, alternos, determinista).", "DERIVADA."),
"NCTX_MASCARA": ("DERIVADA",
    "NTAB+9. ⚠️ Existe SEPARADA de NCTX por una razon dura: la mascara `apagados` es un u64 y "
    "al pasar NCTX de 43 a 81 se habria desbordado. En Rust el compilador lo caza; en C habria "
    "sido comportamiento indefinido silencioso.", "DERIVADA."),
"NTAB": ("DERIVADA", "36, el numero de modelos ICM directos. Sale de la lista de modelos, no se "
    "elige aparte.", "DERIVADA."),
"NSEP": ("DERIVADA", "6, el tamano de SEPS.", "DERIVADA."),
"NISSE": ("DERIVADA", "7, el tamano de ISSE_CTX.", "DERIVADA."),
"IANID": ("IDENTIDAD", "34, la ranura del modelo de anidamiento.", "DERIVADA."),
"INUM": ("IDENTIDAD", "26, donde empiezan las ranuras numericas (para ablacionar con VC_NUM).",
    "DERIVADA."),
"ITXT": ("IDENTIDAD", "30, donde empieza la familia de texto (VC_TXT).", "DERIVADA."),
"NOMBRES": ("IDENTIDAD", "Los nombres de los contextos, para que el censo se lea.", "DERIVADA."),
"AYUDA": ("IDENTIDAD", "El texto de ayuda de la linea de ordenes.", "DERIVADA."),
"MODELOS_LEIBLES": ("IDENTIDAD", "Que versiones de MODELO sabe leer este binario.", "EN EL FLUJO."),
"MODELO_MAX": ("IDENTIDAD",
    "9, la ultima version de modelo IMPLEMENTADA. Es OTRA pregunta que MODELO: aquella dice con "
    "que se ESCRIBE por defecto y esta con hasta donde se sabe LEER, y por eso pueden diferir "
    "mientras una version se mide antes de ponerla de defecto. De aqui salen LAS DOS listas "
    "—MODELOS_LEIBLES y el reparto de descomprimir—, que es lo que impide que se desincronicen.",
    "EN EL FLUJO."),
"N_LEIBLES": ("DERIVADA", "MODELO_MAX. Cuantas versiones sabe leer este binario, o sea el tamano "
    "del array de MODELOS_LEIBLES.", "DERIVADA."),
"MODELOS_LEIBLES_ARR": ("DERIVADA", "1..=MODELO_MAX, construido en tiempo de compilacion. Es el "
    "almacenamiento de MODELOS_LEIBLES; se escribia a mano y se desincronizo dos veces.",
    "DERIVADA."),
"UMBRAL_SALTO": ("MEDIDA",
    "32. La racha de match confirmada a partir de la cual el byte entero se codifica con el match "
    "mas UNA etapa SSE (F6·Q6). Sale de la tabla de cobertura de bench/f6q6-cobertura.tsv sobre "
    "las ocho muestras: a >=16 se cubre el 54,00% de los bytes pero el match solo acierta el "
    "93,4% en enwik8; a >=32 se cubre el 45,34% —cota de Amdahl 1,83x— y el acierto sube al "
    "96,1% en el peor caso. Se compra el 8,7% de cobertura mas barato a cambio del tramo donde "
    "el match miente mas. ⚠️ 64 tambien esta medido (35,61%, 1,55x); 512 —que llego a estar "
    "puesto— NO lo esta y por eso se quito.", "EN EL FLUJO."),
"MODELO_ABLACIONADO": ("IDENTIDAD",
    "254, el numero de MODELO que se marca cuando se ha apagado algun modelo. Cualquier valor "
    "reservado vale; lo unico que importa es que no choque con un modelo de verdad.", "EN EL FLUJO."),

# --- lo que impone la maquina ---
"HUGE": ("FORMATO",
    "2 MiB, el tamano de pagina grande de x86-64. No se elige: lo fija la CPU. ⚠️ Y el nucleo "
    "solo promueve un rango que EMPIECE en un limite de 2 MiB, cosa que un Vec normal no cumple "
    "— por eso hace falta mmap y por eso el intento anterior fallaba EN SILENCIO (madvise sobre "
    "un rango desalineado no da error: acepta y no hace nada).",
    "DETERMINISMO — no afecta a los bytes, pero cambiarla no tendria sentido."),

# --- el modelo de coste: precios de fuera y medidas nuestras ---
"BANCO_B": ("MEDIDA", "374.826.209 B, el DEFLATE del banco. Se mide, no se elige.", "DERIVADA."),
"PREFLATE_B": ("MEDIDA", "6.371.168 B, lo que saca preflate del banco.", "DERIVADA."),
"PREFLATE_S": ("MEDIDA", "172,969 s: inflar + estimar + re-deflatar el banco entero.", "DERIVADA."),
"INFLATE_S": ("MEDIDA", "10,323 s: nuestro inflate sobre los mismos bytes.", "DERIVADA."),
"EUR_H": ("FORMATO",
    "46,0 / (30 x 24) euros por hora: el precio del servidor, dividido entre las horas del mes. "
    "Es una factura, no una eleccion.",
    "SIN MOTIVO — si cambia el servidor hay que tocarla a mano."),
"PB": ("IDENTIDAD", "1e15, un petabyte. Es una unidad.", "DERIVADA."),
"MESES": ("IDENTIDAD", "12, los meses del ano.", "DERIVADA."),

# --- las que siguen siendo «A OJO», y ahora al menos lo dicen ---
"ISSE_CTX": ("A OJO",
    "⚠️ [2,3,4,5,6,7,14]: que contexto refina cada etapa de la cadena, de menos a mas especifico "
    "(ordenes 2,3,4,5, palabra, 8 y 12). NUNCA SE HA BARRIDO cual conviene ni en que orden, y por "
    "eso sigue siendo A OJO. Lo que era su unico dato de alrededor —que meter tambien las salidas "
    "INTERMEDIAS de la cadena empeoraba, 0,7349 contra 0,7263 bpb en codigo— se REPITIO el 02-08 "
    "porque estaba caducado (se midio con el mezclador de UNA capa) y salio al reves: -0,257% en "
    "seis tipos, 5 de 6 a favor. Ver ISSE_INT_ON. Sigue sin medirse LA CONSTANTE EN SI.",
    "DETERMINISMO."),
"H_ZLIB": ("IDENTIDAD",
    "0. Codigo del hash de zlib dentro de vc-fase3. Es una ETIQUETA en un AtomicU8: cualquier "
    "numero distinto valdria igual mientras no choque con los otros cuatro.",
    "DERIVADA."),
"H_LIBDEFLATE": ("IDENTIDAD", "1. Etiqueta del hash de libdeflate. Ver H_ZLIB.", "DERIVADA."),
"H_MINIZ": ("IDENTIDAD", "2. Etiqueta del hash rapido de miniz. Ver H_ZLIB.", "DERIVADA."),
"H_ZLIBNG": ("IDENTIDAD", "3. Etiqueta del hash de zlib-ng. Ver H_ZLIB.", "DERIVADA."),
"H_CRC32C": ("IDENTIDAD", "4. Etiqueta del hash CRC32C (zlib-ng moderno). Ver H_ZLIB.", "DERIVADA."),
"H_SIN_DECIDIR": ("IDENTIDAD",
    "255. El centinela de «todavia nadie lo ha resuelto», para distinguir «no preguntado» de "
    "«preguntado y sale zlib». Tiene que caer FUERA del rango 0..4 y 255 es el maximo de un u8: "
    "asi ningun hash futuro se lo puede pisar sin que sea evidente.",
    "DERIVADA."),
"ISSE_INT_ON": ("MEDIDA",
    "Encendida. Las seis salidas INTERMEDIAS de la cadena ISSE entran al mezclador ademas de la "
    "final. Medido el 02-08, seis tipos de 8 MiB con --max: codigo -0,283%, csvjson -0,169%, sql "
    "-0,090%, bin -0,708%, pdfoffice -0,204% y logs +0,052% (el unico en contra, como en el DMC). "
    "TOTAL -0,257% por +0,5% de ciclos y RSS identico. ⚠️ La medida vieja decia lo contrario y no "
    "estaba mal hecha: estaba hecha con el mezclador de UNA capa, y su argumento —«no converge "
    "dentro de un frame»— dejo de cumplirse al pasar a dos capas.",
    "ES PERILLA — VC_ISSE_INT=0 la apaga. Cambia los bytes de salida."),
"IISSE_INT": ("DERIVADA",
    "Donde empiezan las seis entradas intermedias: justo detras del bloque det:, o sea "
    "NTAB+9+3*NTAB = 153. No se elige, se calcula.",
    "DERIVADA."),
"NISSE_INT": ("DERIVADA",
    "NISSE-1 = 6. La ultima etapa de la cadena ya entra por IISSE, asi que las intermedias son "
    "una menos. Y el numero NO es libre: NCTX pasa de 153 a 159 y NCTXP sigue en 160, que es lo "
    "que hace que estas seis no cuesten ni un bloque SIMD. Una septima romperia el redondeo, y "
    "por eso hay un assert!(NCTXP == 160) que lo caza al compilar.",
    "DERIVADA."),
"SEPS": ("A OJO",
    "⚠️ Los seis candidatos a separador de registro, con el salto de linea primero. Elegidos a "
    "mano mirando formatos tipicos (CSV, logs, SQL) y nunca contrastados contra el corpus.",
    "DETERMINISMO."),
"NCAMPO": ("A OJO",
    "⚠️ 64 campos por registro; mas alla se agrupan en el ultimo. El razonamiento esta escrito "
    "—«una tabla con 200 columnas es rarisima y el coste de equivocarse ahi es cero»— y es "
    "sensato, pero razonamiento no es medida.",
    "DETERMINISMO."),
"WMAX": ("A OJO",
    "⚠️ 24 letras del run alfanumerico que se guardan para sacar la raiz. El razonamiento esta "
    "escrito —«una palabra inglesa de mas de 24 letras no existe fuera de la quimica organica»— "
    "y es cierto, pero el corpus no es solo ingles ni solo prosa.",
    "DETERMINISMO."),
"SUF": ("A OJO",
    "⚠️ La lista de sufijos ingleses que quita la raiz aproximada. Elegida a mano; no es Porter "
    "ni pretende serlo. Nunca contrastada contra el corpus, que lleva codigo y SQL ademas de "
    "prosa.",
    "DETERMINISMO."),
"PESOS_ANID": ("HEREDADA",
    "⚠️ [31,11,17,23] son los pesos de `nestModel` de paq8px, afinados para SU mezcla de modelos. "
    "Primos y distintos entre si a proposito —si dos tipos de parentesis pesaran igual, `[(` y "
    "`((` darian la misma huella—, pero los VALORES no se han re-medido aqui. Es exactamente la "
    "clase de riesgo que esta tabla existe para marcar.",
    "DETERMINISMO."),
"crates/vc-core/src/bin/vc-fase3.rs:CADENA_MAX": ("A OJO",
    "⚠️ 4096, el tope de nuestro recorrido de cadena. Coincide con el max_chain del nivel 9 de "
    "zlib, que es el mayor de la tabla, asi que en teoria no recorta nada. Pero nunca se ha "
    "comprobado que ningun flujo lo toque.",
    "SIN MOTIVO."),
"crates/vc-core/src/bin/vc-prediccion.rs:CADENA_MAX": ("A OJO",
    "⚠️ 128, distinto del 4096 de vc-fase3 y sin ninguna nota que diga por que. Dos herramientas "
    "que simulan lo mismo con topes que se llevan un factor 32.",
    "SIN MOTIVO."),
"crates/vlz/src/lib.rs:CUBOS": ("A OJO",
    "⚠️ 1<<20 cubos de la tabla de hash de vlz. Nunca barrido. ⚠️ Y tiene historia: hasta 7341e4d "
    "el hash llevaba el desplazamiento a mano y CUBOS NO SERVIA DE NADA.",
    "DETERMINISMO."),
"crates/vc-core/src/bin/vc-fase3.rs:CUBOS": ("A OJO",
    "⚠️ 1<<16, un bit mas que la tabla de zlib (1<<15). Deliberado para dar sitio al hash de "
    "libdeflate, que da 16 bits. Nunca medido si sobra o falta.",
    "SIN MOTIVO."),
"crates/vc-core/src/bin/vc-prediccion.rs:CUBOS": ("A OJO",
    "⚠️ 1<<15, la de zlib. Distinta del 1<<16 de vc-fase3 y sin nota que lo explique.",
    "SIN MOTIVO."),
"crates/vlz/src/bin/vlz-saltos.rs:CUBOS": ("DERIVADA",
    "1<<20, la misma que vlz. Este binario mide la cadena de vlz, asi que copiarla es lo "
    "correcto.", "DERIVADA."),
"crates/vlz/src/bin/vlz-saltos.rs:MIN_COPIA": ("DERIVADA",
    "4, la misma que vlz por el mismo motivo.", "DERIVADA."),
"crates/vlz/src/lib.rs:MAX_COPIA": ("MEDIDA",
    "1<<16. Ya no lo limita la codificacion: el esquema de longitud lleva escape desde 9a52807, "
    "que se midio y gana en los cuatro tipos.", "EN EL FLUJO."),
# ── contestadas el 01-08 al mirarlas de verdad ──────────────────────────────────────────────
"DEMASIADO_LEJOS": ("FORMATO",
    "4096, que es el TOO_FAR de zlib (deflate.c:88). No es una eleccion nuestra: simulamos a "
    "zlib, asi que tiene que ser SU numero. ⚠️ Y contestar esta fila destapo un fallo: en zlib "
    "el descarte vive en deflate.c:1965, DENTRO de deflate_slow (1923-2050); deflate_fast va de "
    "la 1824 a la 1922 y NO lo tiene. Nosotros lo aplicabamos a las NUEVE configuraciones, o sea "
    "tambien a los tres niveles voraces. La condicion real, sin Z_FILTERED, es «longitud "
    "exactamente 3 y distancia > 4096»; lo que faltaba era el «solo si es perezoso».",
    "EN EL FLUJO — es de zlib, no puede moverse."),

"LIM": ("MEDIDA",
    "⚠️ ESTA FICHA DECIA «HEREDADA de lpaq, NUNCA re-medida aqui» Y LLEVABA CADUCADA: el fuente "
    "usa [41,40,18,12,9,7,6,5] —la forma «mucha cola»— desde que gano las SEIS muestras contra "
    "la de lpaq (codigo -0,115% · logs -0,237% · csvjson -0,187% · sql -0,109% · bin -0,011% · "
    "pdf -0,006%, etiquetas «LIM …» en historia.jsonl). Un libro mayor que describe como vigente "
    "una constante que ya se cambio es peor que no tener ficha: manda a re-medir lo hecho y "
    "esconde lo que si falta. De donde salio: (1) NO eran 241 estados sino 193 —el comentario "
    "del codigo arrastraba la cifra copiada con la tabla—; (2) con la tabla de lpaq, LIM[5], "
    "LIM[6] y LIM[7] eran INALCANZABLES (para que exista un estado con conteo menor 5 el mayor "
    "tiene que llegar a 5, y LIM[5] valia 4): tres de las ocho entradas no hacian nada, y ahi "
    "estaban los 63 sitios del byte que sobraban y ya se pagaban en memoria.",
    "DETERMINISMO — la forma de la tabla decide los bytes de salida. Sale a perilla "
    "(VC_LIM_FORMA) SOLO para barrerla; lo que gane se vuelve a fijar."),

"crates/vlz/src/lib.rs:MIN_COPIA": ("A OJO",
    "⚠️ 4, con el comentario «por debajo de 4 casi nunca compensa». NUNCA SE HA BARRIDO. Y NO ES "
    "INOCENTE: `hash4` lee 4 bytes, asi que bajar a 3 obliga a cambiar el hash — que es justo lo "
    "que distingue a zlib (3 bytes) de libdeflate (4). El censo dice que el 33,6% de nuestro "
    "banco de DEFLATE lo hizo un codificador de 4.",
    "SIN MOTIVO — nadie lo ha decidido."),

"crates/vc-core/src/bin/vc-fase3.rs:MIN_COPIA": ("FORMATO",
    "3, que es el MIN_MATCH de DEFLATE. Aqui NO se elige nada: el formato no sabe expresar una "
    "copia de 2. ⚠️ Comparte nombre con el MIN_COPIA de vlz, que vale 4 y es una eleccion "
    "nuestra sin barrer. Hasta el 01-08 la tabla los daba por la misma constante —indexaba solo "
    "por nombre— y ensenaba la posicion de esta con la respuesta de aquella.",
    "EN EL FLUJO — lo impone DEFLATE."),

# ---------- vc-cm ----------
"MEM_BITS": ("A OJO", "⚠️ 20, los cubos de la cadena ISSE. Nunca barrido por separado; lo que si "
 "se barrio es VC_MEM, que mueve TODAS las tablas grandes a la vez.", "SIN MOTIVO."),
"NMIX": ("MEDIDA", "4 mezcladores de primera capa. El primero de los cuatro ES exactamente el "
 "mezclador que habia antes, asi que la capa final no puede empeorarlo: en el peor caso aprende "
 "a copiarlo. Esa construccion es la que hace que el 4 sea seguro.", "DETERMINISMO."),
"NSEL": ("A OJO", "⚠️ [512, 256, 32, 256], los juegos de pesos de cada mezclador. Nunca barridos "
 "uno a uno.", "DETERMINISMO."),
"NSEL2": ("A OJO", "⚠️ 16. «Pequeño a proposito para que converja en las primeras decenas de KB» "
 "— razonamiento, no medida.", "DETERMINISMO."),
"NCTXP": ("DERIVADA", "NCTX redondeado al multiplo de 8, para que el mezclador vaya de ocho en "
 "ocho sin resto. Las ranuras de relleno arrancan a peso 0 y su incremento es (0*err)>>10 = 0: "
 "son neutras EXACTAS, no aproximadamente.", "DERIVADA."),
"MODELO": ("EN EL FLUJO", "8, la version con la que se ESCRIBE por defecto. Sube cuando cambia lo "
 "que el modelo predice y viaja en el archivo, para que un binario nuevo abra un archivo viejo. "
 "Nunca por encima de MODELO_MAX, que es hasta donde se sabe LEER: hoy hay una v9 implementada y "
 "leible que aun no es el defecto porque su falsador no se ha medido.", "EN EL FLUJO."),
"IMM": ("DERIVADA", "Ranura del match model corto (6 bytes) dentro de la tabla.", "DERIVADA."),
"IMM2": ("DERIVADA", "Ranura del match model largo (12 bytes).", "DERIVADA."),
"IMM3": ("DERIVADA", "Ranura del match model disperso (zancada 2).", "DERIVADA."),
# ---------- DEFLATE: todo esto es del RFC 1951 ----------
"LONG_BASE": ("FORMATO", "RFC 1951 §3.2.5. No es nuestra.", "EN EL FLUJO."),
"LONG_EXTRA": ("FORMATO", "RFC 1951 §3.2.5.", "EN EL FLUJO."),
"DIST_BASE": ("FORMATO", "RFC 1951 §3.2.5.", "EN EL FLUJO."),
"DIST_EXTRA": ("FORMATO", "RFC 1951 §3.2.5.", "EN EL FLUJO."),
"ORDEN_CL": ("FORMATO", "RFC 1951 §3.2.7, el orden en que van las longitudes del alfabeto de "
 "codigos. Ese orden concreto existe para que los ceros del final se puedan omitir.", "EN EL FLUJO."),
"TABLA_BITS": ("MEDIDA", "9 bits de tabla de primer nivel en el decodificador. Se eligio al "
 "cambiar el decodificador bit a bit por uno de tabla; forma parte de la fase 4 de vdeflate, que "
 "bajo de 279 ciclos/simbolo a 171,8.", "SIN MOTIVO."),
"ZIGZAG": ("FORMATO", "El orden zigzag de JPEG. ITU-T T.81.", "EN EL FLUJO."),
"SOBRECOSTE": ("DERIVADA", "Lo que añade el cifrado por trozo: nonce + etiqueta.", "EN EL FLUJO."),
"SAL": ("FORMATO", "Longitud de la sal de derivacion de clave. 16 B es el minimo recomendado.", "EN EL FLUJO."),
}

# los enums de la FFI de JXL y los indices de fase no se anotan uno a uno: son etiquetas
AUTO = {}
for _f, n in halladas:
    if n.startswith(("ENC_", "DEC_")):
        AUTO[n] = ("FORMATO", "Constante de la API de libjxl. No es nuestra: viene de su cabecera.", "EN EL FLUJO.")
    elif n.startswith("F_"):
        AUTO[n] = ("DERIVADA", "Indice dentro de FASES. No es una eleccion.", "DERIVADA.")
    elif n in ("TEXT", "MEDIA"):
        AUTO[n] = ("A OJO", "⚠️ La lista de extensiones que van a cada carril. Elegida a mano y "
                   "nunca contrastada contra el corpus: `firma()` detecta por CONTENIDO y ya caza "
                   "58 casos de extension mentirosa, asi que la lista es una heuristica de segundo "
                   "orden — pero decide el carril ANTES de leer el contenido.", "SIN MOTIVO.")
R.update({k: v for k, v in AUTO.items() if k not in R})

# Los nombres que aparecen en MAS DE UN fichero. Si ademas tienen valores distintos, una sola
# respuesta no puede ser cierta para los dos y las dos filas van a «sin contestar» hasta que se
# conteste cada una por su fichero (clave "crates/…/x.rs:NOMBRE" en R).
import collections as _c
_porn = _c.defaultdict(list)
for (f, n), (_f, i, t, v) in halladas.items(): _porn[n].append((f, v))
CHOCAN = {n: vs for n, vs in _porn.items() if len({v for _, v in vs}) > 1}

filas, sin = [], []
for f, n in sorted(halladas, key=lambda k: (k[1], k[0])):
    _f, i, t, v = halladas[(f, n)]
    r = R.get(f"{f}:{n}") or (None if n in CHOCAN else R.get(n))
    if r:
        de, por, var = r
        filas.append([n, f"{f}:{i}", v, de, por, var])
    else:
        sin.append((n, f"{f}:{i}", v))

with open("bench/constantes.tsv", "w", newline="") as fh:
    fh.write("# TODAS LAS CONSTANTES, Y POR QUE CADA UNA ES ESA Y NO OTRA (norma del dueño, 01-08).\n")
    fh.write("# Se extraen DE LA FUENTE: si alguien añade una y no la contesta, sale listada al final.\n")
    fh.write("# ⚠️ La clase «A OJO» es la peligrosa: elegida razonando y NUNCA medida. Las cuatro\n")
    fh.write("# constantes que hoy resultaron estar mal puestas estaban todas en esa clase.\n")
    w = csv.writer(fh, delimiter="\t")
    w.writerow(["constante", "donde", "valor", "de_donde_sale", "por_que_ESE_valor", "por_que_no_es_variable"])
    w.writerows(filas)
    if sin:
        w.writerow([])
        w.writerow(["# SIN CONTESTAR — hay que añadirlas a bench/constantes.py"])
        for n, d, v in sin: w.writerow([n, d, v, "?", "?", "?"])

c = collections.Counter(f[3] for f in filas)
# La clase es la PRIMERA PALABRA; lo que va detras es la explicacion. Antes se partia por «—» y
# las respuestas largas sin guion entraban ENTERAS como si fueran una clase, asi que el recuento
# listaba parrafos como categorias.
# Las cuatro clases tienen una y dos palabras («EN EL FLUJO», «SIN MOTIVO», «ES PERILLA»), asi
# que se reconocen por PREFIJO en vez de cortar por un separador que no siempre esta.
CLASES_VAR = ("DETERMINISMO", "EN EL FLUJO", "ES PERILLA", "SIN MOTIVO", "DERIVADA")
def clase_var(t):
    t = (t or "").strip().upper()
    for c in CLASES_VAR:
        if t.startswith(c): return c
    return "?"
vv = collections.Counter(clase_var(f[5]) for f in filas)
print(f"\n  bench/constantes.tsv · {len(halladas)} constantes en la fuente · {len(filas)} contestadas · {len(sin)} sin contestar\n")
if CHOCAN:
    print("  ⚠️ MISMO NOMBRE Y DISTINTO VALOR EN VARIOS FICHEROS — cada una necesita SU respuesta")
    for n, vs in sorted(CHOCAN.items()):
        print(f"    {n}")
        for f, v in sorted(vs): print(f"        {v:<10} {f}")
    print()
print("  DE DONDE SALE CADA UNA")
for k, v in c.most_common(): print(f"    {v:>4}  {k}")
print("\n  POR QUE NO ES VARIABLE")
for k, v in vv.most_common(): print(f"    {v:>4}  {k}")
aojo = [f for f in filas if f[3].startswith("A OJO")]
print(f"\n  ⚠️ {len(aojo)} CONSTANTES «A OJO» — elegidas razonando y NUNCA medidas:")
for f in aojo: print(f"      {f[0]:<18} {f[2]:<22} {f[1]}")
if sin:
    print(f"\n  ⚠️ {len(sin)} SIN CONTESTAR:")
    for n, d, v in sin[:20]: print(f"      {n:<22} {d}")
print()
print("  LO QUE ESTO CAMBIA: hasta hoy «esa constante es esa» se contestaba leyendo el comentario")
print("  de al lado, y los comentarios caducan. Aqui la pregunta es obligatoria y la respuesta")
print("  dice DE QUE CLASE es. Las cuatro que hoy resultaron mal puestas -la ventana de vlz, el")
print("  carril media, PESO_LONG y el barrido del CDC- eran todas de clase «A OJO» o «HEREDADA».")
