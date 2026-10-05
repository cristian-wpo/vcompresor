# -*- coding: utf-8 -*-
"""LAS 16 PREGUNTAS DEL CAMINO, HECHAS A PREDECIR + ACTUALIZAR.

Encargo del dueño (2026-07-31): «apliquemos camino a predecir + actualizar».

POR QUE ESTA PIEZA NO ESTABA. El CAMINO se le hizo a «comprimir» —la decision repartida entre
router, carriles, frame, nivel y codec— y a las diez piezas de codigo. Pero predecir+actualizar
no es ninguna de las dos cosas: es el 83,7% del tiempo de vc-cm repartido entre 43 modelos, una
cadena de siete etapas, cuatro mezcladores y dos capas. Por estar repartido no lo miraba nadie
entero, que es exactamente el mismo motivo por el que «comprimir» tampoco se miraba.

DE DONDE SALE EL 83,7%, porque el numero se citaba mal en seis sitios hasta hoy. De la piramide
de vc-cm, `perf record`, que cubre el 89,1% del tiempo:
    Preparar        2,6%   set_byte_contexts
    Predecir       43,1%   Predictor::p 40,1 + busca_cubo 3,0
    Mezclar         2,8%   punto_avx2
    Actualizar     40,6%   Predictor::update 26,6 + fija_cubos 11,3 + actualiza_avx2 2,7
43,1 + 40,6 = 83,7. NO es «83,7% buscando en tablas»: son las dos fases DONDE se toca la tabla,
y dentro va tambien la aritmetica de los 43 modelos. Explicitamente buscar y escribir el cubo son
14,3%. Y el mezclador es 5,5% (2,8 + 2,7), no 2,8%: la cifra vieja dejaba fuera la mitad.

LA REGLA DE ADMISION es la del resto del CAMINO: ninguna respuesta entra sin decir QUE se miro y
CON QUE. Un «limpio» sin prueba delante es una opinion con formato de tabla.
Lo escribe bench/camino-predecir.py.
"""
import csv, collections

# pregunta · veredicto · que se encontro · con que se midio
R = [
("P1", "¿Cuantas veces le preguntamos al nucleo lo mismo?", "SIN MIRAR",
 "Predecir y actualizar no hacen llamadas al sistema: las tablas estan reservadas de una vez y "
 "marcadas para paginas grandes. Lo que SI toca al nucleo es el primer acceso a cada pagina de "
 "los 484 MiB, y eso no lo ha contado nadie. Con paginas de 2 MiB serian ~242 fallos por hilo, "
 "que suena a nada; con paginas normales serian 123.904, que ya no.",
 "Falta: contar fallos de pagina con perf, o leer /proc/self/smaps_rollup al acabar. Es la "
 "linea M9 de bench/vccm-sin-medir.tsv y sigue abierta."),

("P2", "¿Cuantas veces leemos el mismo byte?", "HALLAZGO — lo vio el dueño",
 "Los contextos de orden se hashean CADA UNO DESDE CERO sobre rangos anidados: orden-1 sobre "
 "1 byte, orden-2 sobre 2, orden-3 sobre 3, orden-5 sobre 5, orden-8 sobre 8 y orden-12 sobre 12. "
 "Son 35 lecturas y 35 pasos de hash por byte de entrada para una informacion que esta metida "
 "una dentro de otra. El motivo es la DIRECCION: FNV dobla del byte mas viejo al mas nuevo, asi "
 "que hash(h[6..8]) no sirve para hash(h[5..8]) porque arrancan en sitios distintos. Doblando al "
 "reves, el orden N seria el orden N-1 mas un paso: 7 en vez de 35.",
 "Leido en set_byte_contexts, vc-cm/src/lib.rs. ⚠️ CUANTO VALE, SIN MEDIR: el hasheo cae dentro "
 "de «Preparar», que es el 2,6% del tiempo. Ahorrar el 80% de un 2,6% son 2 puntos como techo "
 "absoluto, y solo si todo ese 2,6% fuera hashear. No es el hueco grande."),

("P3", "¿Reservamos memoria por el caso peor o por el real?", "HALLAZGO — parcialmente arreglado",
 "El censo (VC_CENSO=1) destapo que varios modelos reservan 32 MiB para hashear dos bytes: una "
 "tabla que NO PUEDE llenarse nunca porque el dominio del contexto es mas pequeño que la tabla. "
 "sparse-paso8 era el extremo. De ahi salieron DUPLICADOS y PODA_V2. Pero cuanto de los 484 MiB "
 "se toca DE VERDAD sigue sin medirse, y es lo que decide si el dimensionado esta bien.",
 "bench/censo (VC_CENSO=1) para la ocupacion por modelo. Falta M9 para el total tocado."),

("P4", "¿Tiramos algo que ya habiamos calculado?", "HALLAZGO",
 "El modelo de coincidencias lee `mm_hashtab[mh]` y USA el puntero SIN COMPARAR UN SOLO BYTE. No "
 "verifica que la coincidencia sea real. Una colision mete un puntero a datos que no se parecen "
 "en nada y el modelo vota basura al mezclador hasta que `mm_len` deja de crecer. Y la "
 "verificacion no seria trabajo nuevo: los bytes a comparar YA estan en `mm_buf`, que es el mismo "
 "buffer del que sale la prediccion.",
 "Leido en vc-cm/src/lib.rs, la actualizacion del match por byte. ⚠️ SIN MEDIR cuanto cuesta la "
 "basura: la prueba es contar colisiones verificando a posteriori y ver el ratio con y sin."),

("P5", "¿El ORDEN de las fases obliga a repetir trabajo?", "LIMPIO — y con prueba",
 "Era el fallo y esta arreglado: `fija_cubos` calculaba una direccion y la tocaba acto seguido, "
 "~41 fallos de cache EN SERIE cada uno esperando al anterior. Partido en dos pasadas —calcular "
 "las 41 direcciones y pedirlas TODAS a la vez con prefetch, y despues resolverlas— las "
 "peticiones se solapan y se paga UNA espera en vez de cuarenta y una.",
 "El propio codigo y su comentario. Es el patron correcto y ya estaba puesto antes de hoy."),

("P6", "¿Estamos optimizando la perilla que importa?", "HALLAZGO — 31-07",
 "NO. Dos de las tres perillas del mezclador estan puestas en valores que EL PROPIO BANCO midio "
 "como peores: el paso >>10 contra >>12 (-0,907%) y el tope del StateMap 1023 contra 191 "
 "(-0,231%). Estan medidas, escritas y sin aplicar. Mientras tanto el esfuerzo fue al barrido de "
 "TAMAÑO de tablas. Y hay un encuadre que estaba mal: el mezclador es el 5,5% del reloj, asi que "
 "tocarlo NO es una jugada de velocidad — es de RATIO, que es lo que se paga cada mes en disco.",
 "bench/tasa-mezclador.tsv, bench/tasa-statemap.tsv, bench/tasa-isse.tsv (esta si esta en su "
 "optimo). ⚠️ Se remiden JUNTAS y por tipo de dato antes de aplicarlas: atacan lo mismo -la "
 "velocidad de adaptacion- y pueden estorbarse."),

("P7", "¿La justificacion se llego a medir alguna vez?", "HALLAZGO — el mas caro de hoy",
 "`no-lineal:dispersion` tiene el 83,8% del peso del mezclador, el segundo mas alto de los 43. "
 "Su aportacion medida es NEGATIVA: apagarlo hace el archivo MAS PEQUEÑO, -0,0593% en codigo y "
 "-0,0816% el par entero en sql. Peso alto no es aportacion: es que el mezclador le hace mucho "
 "caso. Y si lo que dice es ruido correlacionado con el resto, hacerle caso es justo lo que "
 "sobra. No se habia visto NUNCA porque hasta hoy los 9 modelos derivados no se podian apagar.",
 "Ablacion con el interruptor nuevo, 4 MiB de codigo y de sql, commit 7be26b4. Los pesos vienen "
 "de bench/m4-pesos.tsv (VC_DUMP_W)."),

("P8", "¿Hacemos el mismo trabajo dos veces con parametros distintos?", "HALLAZGO — el grande",
 "Si, y por racimos. Los cinco modelos `match` valen 1,8974% en codigo y 4,5512% en sql cuando se "
 "apagan TODOS — pero dejando UNO SOLO encendido se recupera el 66% y el 73%. Cinco tablas hash "
 "(~72 MB) y cinco busquedas por byte comprando una cosa cinco veces con variaciones. Y de uno "
 "en uno cada modelo media entre 0,01% y 0,15%: la ablacion de uno en uno NO PODIA VERLO.",
 "bench/registros-m3/racimos.tsv + la correlacion de M3 (r=0,93..0,99 entre los cinco, EN LOS "
 "CINCO TIPOS de dato)."),

("P9", "¿Pagamos el caso general para servir al caso raro?", "SOSPECHA — sin cerrar",
 "Las 34 tablas se dimensionan por el caso peor y varias no pueden llenarse nunca (ver P3). Y hay "
 "un caso al reves, medido hoy: `num:delta` DEJADO SOLO sale peor (+0,3740%) que apagar su racimo "
 "entero (+0,1258%). Un modelo que en compañia ayuda y solo hace daño es el caso raro pagandose "
 "con el general.",
 "bench/registros-m3/racimos.tsv, 4 MiB de codigo. Falta repetirlo en los cinco tipos."),

("P10", "¿El numero viaja con su contexto, o se copio suelto?", "HALLAZGO — ARREGLADO 31-07",
 "El 83,7% se citaba en SEIS sitios como «busquedas en tablas que no caben en cache», y ninguno "
 "de los seis era la medida: todos citaban. La medida dice «Predecir 43,1% + Actualizar 40,6%», "
 "que son las fases DONDE se toca la tabla pero llevan dentro toda la aritmetica de los 43 "
 "modelos. Buscar y escribir el cubo explicitamente son 14,3%. Y de paso salio un segundo error: "
 "«2,8% en el mezclador» dejaba fuera la actualizacion de pesos; son 5,5%.",
 "bench/piramide-vccm.tsv, que es la unica fuente. Las seis citas corregidas hoy."),

("P11", "¿Lo que medimos sigue siendo lo que creemos que medimos?", "HALLAZGO — freno dos veces hoy",
 "Primero: la correlacion sobre 1 MiB daba r=1,0000 EXACTO entre `columna x sep` y `columna2 x "
 "sep2`, y parecia un modelo duplicado. Con 4 MiB desaparece: con tan poco dato el detector de "
 "separadores aun no habia visto bastante para elegir dos distintos, asi que los dos modelos "
 "miraban lo mismo. Segundo: la mascara de modelos apagados por defecto DEPENDE DE LA FAMILIA que "
 "elija `elige_familia()` para ese dato, asi que «este modelo no aporta» y «este modelo ya estaba "
 "apagado» se pueden confundir entre tipos.",
 "El propio barrido a 1 y a 4 MiB. Para la familia: la ablacion individual desambigua -si apagar "
 "solo k da los mismos bytes que la base, es que ya estaba apagado-."),

("P12", "¿El instrumento conserva todo lo que la medida produjo?", "HALLAZGO — ARREGLADO 31-07",
 "NO, y en el propio instrumento: `mide.sh` mandaba el stdout del comando a /dev/null. Se comio "
 "TRES tablas en un dia y la ultima costo 53 minutos de maquina —la sonda de la fase 3 imprimio "
 "sus dos cotas y desaparecieron—. Quedaba la fila del libro mayor con sus ciclos y su RAM, y se "
 "perdia LO QUE LA MEDIDA HABIA AVERIGUADO. Estaba asi por un motivo bueno que caduco: mide.sh "
 "nacio para cronometrar el archivador, que no imprime nada util.",
 "Ahora se guarda en bench/registros/salidas/<etiqueta>-<commit>.txt, tambien cuando la medida "
 "falla, y la bascula dice donde. Probado."),

("P13", "¿Quien comprueba la prueba?", "HALLAZGO — ARREGLADO 31-07",
 "El modelo de dispersion se calcula LEYENDO las predicciones de los demas, y su bucle saltaba "
 "los modelos de tabla apagados pero NO los derivados —la condicion se escribio cuando los "
 "derivados no se podian apagar—. O sea que apagar `match-6` le quitaba la voz ante el mezclador "
 "pero su valor seguia entrando en la dispersion, que si votaba. Toda la ablacion de derivados "
 "salia subestimada.",
 "Arreglado y verificado en las DOS direcciones: la medida se mueve una milesima (+0,1528% -> "
 "+0,1541%) y la base sale byte a byte identica. Ademas se comprobo que el interruptor nuevo "
 "LLEGA: los nueve derivados dan bytes distintos al apagarlos, que es justo lo que antes no "
 "pasaba y se leia como «no aportan nada»."),

("P14", "¿Corremos TODAS nuestras suites, o solo la que conocemos?", "SIN MIRAR — y hay un riesgo",
 "Las medidas de ablacion cuentan BYTES y no hacen ida y vuelta. Un archivo comprimido con "
 "VC_APAGA sale de un juego de modelos distinto del que usaria el descompresor por defecto, asi "
 "que probablemente NO SE ABRE. Como bandera de diagnostico es aceptable, pero nadie lo ha "
 "comprobado ni esta escrito en ningun sitio, y una bandera que produce archivos ilegibles sin "
 "avisar es una trampa esperando.",
 "Falta: `vc-cm test` -que ya hace ida y vuelta- con VC_APAGA puesto. Es una linea."),

("P15", "¿Es esta la MEJOR prueba posible, o solo una valida?", "HALLAZGO — ARREGLADO 31-07",
 "La ablacion de uno en uno NO DISTINGUE «inutil» de «redundante», y era el diseño de la prueba, "
 "no su implementacion. Si cinco modelos predicen casi lo mismo, apagar cualquiera cuesta ~nada "
 "-el +0,01% a +0,15% que salio- y apagar los cinco cuesta 1,90%. La suma de ablaciones "
 "individuales no es la ablacion del grupo, y la conclusion «los cinco match son prescindibles» "
 "habria sido falsa.",
 "Prueba nueva con TRES preguntas: suelto, racimo entero, y DEJANDO UNO ENCENDIDO. La tercera es "
 "la que decide y es la que faltaba. Y en TXT salio la interaccion al reves: quitar "
 "`txt:skip-grama` cuesta 0,2624% pero dejarlo SOLO a el vale exactamente lo mismo que apagarlos "
 "todos, byte por byte — no aporta por si mismo, solo encima de los otros cuatro."),

("P16", "¿Se puede medir esto SIN construir antes lo que la medida justificaria?", "LIMPIO",
 "Si, y es como se ha hecho todo lo de hoy: la correlacion entre los 43 sale de acumular cinco "
 "sumas en linea -15 KB- en vez de volcar las predicciones, que sobre 200 MiB serian 275 GB. Y "
 "los racimos se midieron apagando modelos, sin quitar ni una linea del compresor. Cero lineas de "
 "producto tocadas para saber que cuatro de cinco `match` se reparten un tercio.",
 "bench/ventana.py como precedente, y hoy VC_CORR + VC_APAGA sobre los 43."),
]

with open("bench/camino-predecir.tsv", "w", newline="") as f:
    f.write("# LAS 16 PREGUNTAS DEL CAMINO, HECHAS A PREDECIR + ACTUALIZAR.\n")
    f.write("# Es el 83,7% del tiempo de vc-cm, repartido entre 43 modelos, una cadena de siete\n")
    f.write("# etapas, cuatro mezcladores y dos capas. Por estar repartido no lo miraba nadie\n")
    f.write("# entero — el mismo motivo por el que «comprimir» tampoco se miraba.\n")
    f.write("# Ninguna respuesta entra sin decir QUE se miro y CON QUE.\n")
    f.write("# Lo escribe bench/camino-predecir.py.\n")
    w = csv.writer(f, delimiter="\t")
    w.writerow(["id", "la_pregunta", "veredicto", "que_se_encontro", "con_que_se_midio"])
    w.writerows(R)

c = collections.Counter(r[2].split()[0] for r in R)
print(f"\nbench/camino-predecir.tsv: {len(R)} preguntas a predecir+actualizar\n")
for r in R:
    print(f"  [{r[2]:<28}] {r[0]:<4} {r[1]}")
print()
print("  " + " · ".join(f"{k}: {v}" for k, v in c.most_common()))
print()
print("  Nueve de dieciseis dieron HALLAZGO y cinco se arreglaron hoy. El mas caro no es de")
print("  velocidad: es que el modelo con el 83,8% del peso del mezclador RESTA, y no se habia")
print("  podido ver nunca porque los nueve derivados no tenian interruptor de ablacion.")
