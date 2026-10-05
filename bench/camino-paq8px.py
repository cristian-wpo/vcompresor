# -*- coding: utf-8 -*-
"""EL CAMINO A LAS IDEAS DE paq8px.

Lo pide el dueño: «aplicale camino a sus ideas». No al codigo —que es GPL y no se copia— sino al
CUERPO DE IDEAS que hay dentro y a lo que nosotros hemos hecho al leerlo.

⚠️ LICENCIA, y va primero porque condiciona todo lo demas: paq8px es GPL v2+. Se clono FUERA del
repo (/tmp) el 01-08 para leerlo. Se pueden coger IDEAS —una idea no es copiable— y hay que
escribir codigo propio con constantes re-medidas. No entra ni una linea suya en nuestro arbol.

Lo escribe bench/camino-paq8px.py.
"""
import csv, collections

R = [
("P1", "¿Cuantas veces le preguntamos al nucleo lo mismo?", "NO APLICA",
 "La pregunta es de llamadas al sistema y aqui se esta leyendo codigo, no ejecutandolo. La "
 "version util seria «¿cuantas veces le preguntamos LO MISMO a su codigo?», y ahi si hay algo: "
 "el CHANGELOG (2.100 lineas) es un registro de veinte anos de QUE COMPRO CADA CAMBIO, y es "
 "informacion que no esta en el codigo.",
 "Leido por encima. Sin explotar: es una lista de mejoras ya medidas por otro sobre el mismo "
 "problema, y no la hemos peinado."),

("P2", "¿Cuantas veces leemos el mismo byte?", "HALLAZGO — Y ES SOBRE NOSOTROS",
 "Su `nestModel` mezcla DOS cosas en un modelo: la huella de la pila de corchetes Y las clases "
 "de caracter con las repeticiones aplastadas. La segunda YA LA TENEMOS —es `cgrp`, la «forma "
 "del texto»—. Copiar el modelo entero habria sido pagar dos veces por la misma informacion.",
 "Se cogio SOLO la parte que faltaba (el acumulador de corchetes). Ese error —modelo duplicado— "
 "ya esta documentado en nuestro propio fichero para el caso de la cadena ISSE."),

("P3", "¿Reservamos memoria por el caso peor o por el real?", "🚨 HALLAZGO — Y TUMBA UNA COARTADA NUESTRA",
 "En vc-cm/src/lib.rs estaba escrito desde el 28-07: «paq8px -8 usa 2.408 MB y nosotros 600: le "
 "mediamos con cuatro veces nuestra memoria y luego concluiamos que nos ganaba por arquitectura». "
 "Sobre esa frase se SUBIO nuestra memoria. La comparacion a memoria igualada NUNCA SE HIZO.",
 "Medida el 01-08: su MINIMO (-1) usa 1.070 MB y nosotros 810. YA USAMOS MENOS QUE SU MINIMO y "
 "seguimos 11,6% detras de su peor ajuste. La coartada no se sostiene."),

("P4", "¿Tiramos algo que ya habiamos calculado?", "AVISO SOBRE EL METODO DE LECTURA",
 "El clon esta en /tmp, o sea que se pierde al reiniciar la maquina. Lo que hay que CONSERVAR no "
 "es su codigo (GPL, y ademas se vuelve a clonar en un minuto) sino NUESTRAS CONCLUSIONES: que "
 "modelos tienen, cual falta, que compro cada uno.",
 "Por eso esta ficha y bench/censo-paq8px.tsv viven en NUESTRO repo y el clon no."),

("P5", "¿El ORDEN de las fases obliga a repetir trabajo?", "HALLAZGO ESTRUCTURAL",
 "Su cadena SSE/APM esta ESPECIALIZADA POR TIPO DE BLOQUE: Texto tiene 4 APM + 3 APM1 + 2 "
 "APMPost, x86_64 otra distinta, Imagen/Audio/Jpeg las suyas. Nosotros tenemos UNA cadena de 5 "
 "APM para todo. Ellos deciden el tipo de bloque ANTES y luego usan el juego que toca; nosotros "
 "usamos el mismo siempre.",
 "SIN MEDIR para nosotros. Y ojo: nuestro archivador SI enruta por carriles, asi que la pieza "
 "para especializar la cadena ya existe — es el orden lo que habria que cambiar, no inventar."),

("P6", "¿Estamos optimizando la perilla que importa?", "🚨 HALLAZGO — LA PERILLA NO ERA LA MEMORIA",
 "Las dos curvas de memoria son PLANAS. paq8px de -1 a -8 gana 10,9% TRIPLICANDO la RAM (1.070 "
 "-> 2.778 MB). Nosotros de VC_MEM=0 a -5 perdemos 2,4% con SEIS VECES menos (810 -> 135 MB).",
 "La memoria vale entre el 2% y el 11% en un rango de 20x; el hueco de arquitectura es del 11,6% "
 "al 25,2%. Eso ordena las prioridades — y dice que la subida de memoria del 28-07 compro poco."),

("P7", "¿La justificacion se llego a medir?", "🚨 HALLAZGO — EL HUECO SI, LA ATRIBUCION NO",
 "El hueco M12 esta medido (11,6% a memoria igualada, 25,2% contra su maximo). PERO su atribucion "
 "a cualquiera de sus modelos NO LO ESTA. La lista de «lo que nos falta» —nestModel, xmlModel, "
 "charGroupModel, chartModel, sparseBitModel, linearPredictionModel— es una lista de HIPOTESIS "
 "ordenadas por plausibilidad, no de causas medidas.",
 "El primero YA se midio, y por eso el resto sigue siendo hipotesis y no fe: el anidamiento da "
 "-0,578% en 2 MB de codigo. Real, pero es una veinteava parte del hueco."),

("P8", "¿Hacemos el mismo trabajo dos veces con parametros distintos?", "AVISO — LAS UNIDADES NO SON COMPARABLES",
 "«Ellos tienen 32 modelos y nosotros 43» es una frase sin sentido: sus 32 son CLASES de modelo "
 "(wordModel, recordModel...) y nuestros 43 son CONTEXTOS individuales. Un solo modelo suyo "
 "aporta varias entradas al mezclador.",
 "Comparar por numero es lo que llevaba a «tenemos mas y perdemos». Lo que se compara es QUE "
 "IDEAS hay, no cuantas cajas."),

("P9", "¿Pagamos el caso general para servir al caso raro?", "HALLAZGO — 9 de sus 32 no nos sirven",
 "Nueve de sus modelos son de imagen, audio, JPEG y DEC Alpha. Nosotros esos datos los mandamos "
 "por SUS CARRILES —JXL para JPEG, el carril media para video— o los almacenamos. Adoptar «su "
 "numero de modelos» seria pagar el caso general por el raro.",
 "De sus 32, los que aplican a nuestro corpus son 16. Y de esos, la mitad ya los tenemos."),

("P10", "¿El numero viaja con su contexto?", "AVISO — tres contextos que hay que decir siempre",
 "(1) El hueco esta medido sobre 2 MB de CODIGO; los cinco corpus del 29-07 dan de 15,2% a "
 "65,9%, o sea que el tipo de dato manda muchisimo. (2) paq8px NO ES UN PRODUCTO: 0,008 MB/s, y "
 "somos 17,7 VECES mas rapidos. (3) Su LSTM esta APAGADO por defecto y va PRE-ENTRENADO con "
 "pesos que envian aparte: comparar contra el es comparar contra un modelo con deberes hechos.",
 "Los tres van en bench/censo-paq8px.tsv."),

("P11", "¿Lo que medimos sigue siendo lo que creemos?", "🚨 HALLAZGO — el numero de nuestra propia nota estaba MAL",
 "Nuestro codigo decia «paq8px -8 usa 2.408 MB». Medido el 01-08: 2.982 MB de pico segun el "
 "propio programa, 2.778-2.845 MB de RSS. El 2.408 sale de su README —es la memoria que RESERVA "
 "para modelos, no la que USA— y se cito como si fuera lo segundo.",
 "Es P11 de manual: el numero seguia escrito, seguia sonando bien, y ya no era lo que decia."),

("P12", "¿El instrumento conserva todo lo que la medida produjo?", "HALLAZGO — SIN ARREGLAR, y van CUATRO",
 "La tabla de memoria igualada esta en bench/registros/salidas/ como texto, pero NO paso por "
 "bench/mide.sh. Es el cuarto experimento seguido con el mismo agujero (entrelazar, la curva de "
 "juntos, el barajado, y este).",
 "⚠️ Cuatro veces ya no es un descuido: es que el carril del banco se escribe al final y deberia "
 "escribirse al principio."),

("P13", "¿Quien comprueba la prueba?", "HALLAZGO — EL CONTROL EXISTIO Y CAMBIO LA CONCLUSION",
 "La conclusion vieja era «nos gana por arquitectura, pero es que le medimos con cuatro veces "
 "nuestra memoria». El control —barrer SUS seis niveles de memoria y NUESTROS tres— la partio en "
 "dos: la parte de «por arquitectura» se confirma y la coartada de la memoria se cae.",
 "Y el control era barato: seis compresiones suyas de 2 MB, veinte minutos."),

("P14", "¿Corremos TODAS nuestras suites?", "PENDIENTE — y es obligatorio aqui",
 "El modelo de anidamiento cambia MODELO de 4 a 5, o sea que cambia lo que se predice. La ida y "
 "vuelta esta comprobada en la primera muestra y hay cuatro corriendo, pero la SUITE del producto "
 "no se ha pasado todavia.",
 "Un modelo nuevo que no descomprime exacto no es un modelo: es corrupcion con buen ratio."),

("P15", "¿Es esta la MEJOR prueba posible?", "NO — la mejor seria una ablacion POR MODELO y su CLI no la da",
 "Lo que de verdad decidiria que implementar es apagar SUS modelos uno a uno sobre NUESTRO corpus "
 "y ver cual cuesta cuanto. Su linea de comandos no lo permite: no hay interruptor por modelo.",
 "Se podria parchear su fuente para anadirlos —es GPL, se puede modificar para uso propio— pero "
 "eso es un proyecto, no una tarde. Lo que SI da su CLI esta en P16."),

("P16", "¿Se puede medir SIN construir?", "SI, Y NO LO HABIAMOS USADO",
 "Su CLI trae tres ablaciones que contestan mucho sin tocar una linea: `-0L` comprime SOLO CON EL "
 "LSTM, `-forcetext` fuerza el juego de modelos de texto y `-forcebinary` el generico. Con esas "
 "tres se sabe cuanto vale el LSTM POR SI SOLO y cuanto vale la especializacion por tipo de "
 "bloque — que son dos de las hipotesis grandes de la lista.",
 "Lanzadas el 01-08. Si `-0L` sale cerca del total, los modelos importan poco y el LSTM lo "
 "explica casi todo; si sale muy lejos, es al reves. Cualquiera de las dos respuestas reordena la "
 "lista de lo que hay que construir."),
]

with open("bench/camino-paq8px.tsv", "w", newline="") as f:
    f.write("# EL CAMINO A LAS IDEAS DE paq8px (dueño, 01-08). No a su codigo —GPL, no se copia—\n")
    f.write("# sino al cuerpo de ideas y a lo que hemos hecho al leerlo.\n")
    f.write("# 🚨 P3/P6/P11: la coartada de la memoria estaba escrita en NUESTRO codigo desde el 28-07\n")
    f.write("# y era falsa. Ya usamos MENOS que su minimo (810 contra 1.070 MB) y seguimos 11,6% detras.\n")
    f.write("# 🚨 P7: el hueco esta medido; su atribucion a un modelo concreto NO. Todo lo demas son\n")
    f.write("# hipotesis ordenadas por plausibilidad — salvo el anidamiento, ya medido en -0,578%.\n")
    w = csv.writer(f, delimiter="\t")
    w.writerow(["id", "la_pregunta", "veredicto", "que_se_encontro", "con_que_se_midio"])
    w.writerows(R)

c = collections.Counter(r[2].split("—")[0].strip() for r in R)
print(f"\nbench/camino-paq8px.tsv: {len(R)} preguntas\n")
for r in R:
    print(f"  [{r[2][:40]:<40}] {r[0]:<4} {r[1]}")
print()
print("  " + " · ".join(f"{k}: {v}" for k, v in c.most_common()))
print()
print("  LO QUE DECIDE, y son tres:")
print("  P3/P6/P11 — LA COARTADA DE LA MEMORIA ERA FALSA y estaba escrita en nuestro propio codigo.")
print("  Ya usamos MENOS que su minimo. Y las dos curvas de memoria son planas: la memoria vale")
print("  entre 2% y 11% en un rango de 20x, y el hueco de arquitectura es del 11,6% al 25,2%.")
print()
print("  P7 — EL HUECO ESTA MEDIDO, LA ATRIBUCION NO. La lista de lo que nos falta es de hipotesis,")
print("  no de causas. La primera ya se midio: el anidamiento da -0,578% en codigo. Real, pero una")
print("  veinteava parte del hueco — o sea que quedan diecinueve veintavos por explicar.")
print()
print("  P16 — SU CLI YA TRAE ABLACIONES QUE NO HABIAMOS USADO: `-0L` (solo LSTM), `-forcetext` y")
print("  `-forcebinary`. Contestan cuanto vale el LSTM solo y cuanto la especializacion por tipo,")
print("  que son dos de las hipotesis grandes, y no hay que escribir ni una linea.")
