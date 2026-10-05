# -*- coding: utf-8 -*-
"""TECNICAS PRESTADAS: ¿de que problema venian, y es el nuestro?

Encargo del dueño (2026-07-31), y es la pregunta que le sigue a haber frenado el cebador:
«¿que otras tecnicas puede que estemos aplicando que en realidad no cubren completamente nuestro
problema?».

POR QUE HACE FALTA UN CENSO Y NO BASTA CON EL DE TERCEROS. `terceros.tsv` lista QUE codigo de
fuera usamos y con que lo hemos sustituido. Esto es otra cosa: una IDEA prestada no trae codigo,
trae una PREMISA — el problema para el que se invento. Y una premisa no encaja o deja de encajar
sin que nadie lo note, porque no hay dependencia que actualizar ni linea que cambie.

EL CASO QUE ABRIO ESTO, y esta medido hoy. El cebador (diccionario) viene de comprimir MUCHOS
FICHEROS PEQUEÑOS, donde el arranque en frio ES el coste. Nuestros tramos son de 32 MiB. Medido:
    codigo    -31,50% con 64 KiB de dato  ->  -0,31% con 4 MiB     se desploma
    csvjson   -19,59% con 64 KiB          ->  -8,10% con 4 MiB     aguanta
    sql        -2,73% con 64 KiB          ->  +5,29% con 256 KiB   HACE DAÑO
La tecnica prestada sirve para UN tipo de dato y estorba en otro. Aplicarla uniformemente —que
es como viene— habria empeorado sql.

LA REGLA DE ADMISION: cada fila dice de que problema viene la tecnica, si esa premisa es la
nuestra, y CON QUE se comprueba. «Sospecho que no encaja» sin prueba al lado es una queja.
Lo escribe bench/prestadas.py.
"""
import csv, collections

# tecnica · de donde viene · su premisa original · ¿es la nuestra? · veredicto · la prueba
P = [
("el cebador / diccionario", "comprimir muchos ficheros pequeños e independientes",
 "el arranque en frio del modelo ES el coste, porque cada item dura unos KB",
 "SOLO A VECES — nuestros tramos son de 32 MiB y el modelo «se calienta en unos KB»",
 "MEDIDO 31-07 · DEPENDE DEL DATO",
 "codigo se desploma de -31,50% (64 KiB) a -0,31% (4 MiB); csvjson aguanta -8,10%; sql pasa a "
 "+5,29% y HACE DAÑO. bench/cebador.sh, sin tocar una linea de producto: "
 "en_caliente = comprimir(cebador+dato) - comprimir(cebador)."),

("el carril MEDIA a nivel 3", "archivadores generales que asumen que .mp4/.jpg/.zip ya vienen comprimidos",
 "el medio YA esta comprimido, asi que gastar CPU en el es tirarla",
 "NO PARA TODO EL MEDIO — falla en una clase entera de contenido real",
 "MEDIDO 31-07 · LA PREMISA FALLA",
 "Sobre un MP4 de 106 MB que es una GRABACION DE PANTALLA: zstd -3 -lo que hace el carril hoy- "
 "deja 79.714.990 B (71,60%) y zstd -19 deja 69.229.912 B (62,19%). El nivel 19 compra 10.485.078 "
 "B mas: un 13,2% menos de archivo. Y por trozos se ve el porque: el principio comprime al 53,26% "
 "y el final al 96,13%. El contenido sintetico -zonas planas, texto, bordes limpios- deja mucho "
 "que el codificador de video no cogio. Las grabaciones de pantalla no son raras en un respaldo: "
 "tutoriales, demos, capturas. "
 "⚠️ Y un hallazgo de propina que no buscaba: vc-cm sale PEOR que zstd -19 en los tres trozos "
 "(54,05% contra 53,26%, 64,73% contra 63,61%, 97,46% contra 96,13%). Nuestro codec mas fuerte "
 "pierde en este dato. "
 "LO QUE FALTA: que `--price` decida el nivel del carril media por contenido, como ya decide los "
 "carriles. La sonda ya cronometra; no hace falta inventar el instrumento."),

("el FRAME (trocear para poder sacar uno)", "codecs de streaming y archivadores con acceso aleatorio",
 "el usuario quiere sacar UN fichero sin descomprimir el archivo entero",
 "NADIE LO HA COMPROBADO — y es la premisa mas cara de todas",
 "SIN MIRAR — el hueco mas grande de este censo",
 "MEDIDO lo que CUESTA la premisa: frames de 64 MiB en vez de 8/2/4 dan -3,833% de bytes con "
 "MENOS memoria (E2E sobre 2,62 GB). Lo que NO se ha medido nunca es si alguien saca ficheros "
 "sueltos de un respaldo de 2,6 GB. Si no lo hace, el frame pequeño es coste puro. "
 "La prueba: preguntarselo al dueño, que es quien lo vende. No hay instrumento que lo conteste."),

("mezcla de contextos (linaje paq/lpaq/zpaq)", "las competiciones de compresion maxima",
 "el RATIO manda y el tiempo no se factura",
 "NO — nuestro criterio es el DINERO: bytes x precio de disco + CPU x precio de maquina",
 "MEDIDO · LA PREMISA NO ES LA NUESTRA",
 "`--max` comprime 2,56% contra 5,05% y PIERDE DINERO: 414,79 s contra 27,13 s. La arquitectura "
 "entera esta optimizando algo que no vendemos. Ver la memoria de economia del CM y "
 "bench/vs-restic-borg.tsv."),

("la auto-verificacion de preflate", "una herramienta SUELTA que recomprime zips ajenos",
 "no hay nadie mas comprobando, asi que tiene que jurar el resultado ella misma",
 "NO DEL TODO — nosotros verificamos el archivo ENTERO ademas",
 "MEDIDO · VERIFICACION DUPLICADA",
 "preflate re-deflata el plano ENTERO solo para comprobarse: 41,58 s de sus 172,97, el 24%. Y "
 "despues `vcompresor verify` comprueba el archivo completo otra vez. Su premisa era «soy la "
 "ultima linea»; aqui no lo es. bin/vc-preflate-coste. ⚠️ Quitarla es tocar la seguridad: la "
 "prueba que lo abre es medir cuantos flujos rechaza esa verificacion — si son cero en 1.973, "
 "hablamos; si rechaza alguno, se queda."),

("troceado por contenido (CDC, hash rodante)", "sistemas de respaldo con deduplicacion",
 "el mismo contenido reaparece DESPLAZADO -alguien inserto bytes al principio- y por eso los "
 "bloques de tamaño fijo se desalinean",
 "SIN COMPROBAR — puede que nuestros duplicados sean ficheros enteros o solo anexos (logs)",
 "SIN MEDIR",
 "Si los duplicados son fichero-entero o prefijo-mas-cola, un hash por fichero haria lo mismo "
 "mucho mas barato. La prueba: de los trozos que deduplican, ¿cuantos vienen de contenido "
 "DESPLAZADO y cuantos de ficheros identicos o anexados? Se cuenta con el indice, sin comprimir "
 "nada."),

("E8E9 (transformada de ejecutables)", "empaquetadores de binarios",
 "el dato es codigo x86 con direcciones absolutas en los saltos y las llamadas",
 "SIN COMPROBAR sobre nuestro corpus",
 "SIN MEDIR",
 "El corpus tiene 553 .cpp y 234 .h, que son FUENTE y no binarios. La deteccion se corre sobre "
 "una ventana de cada fichero que no cae en texto ni en media. La prueba: cuantos ficheros la "
 "pasan de verdad y cuanto compran, contra lo que cuesta mirarlos a todos."),

("paginas grandes para las tablas", "bases de datos con conjuntos de trabajo grandes",
 "el conjunto de trabajo es grande PERO con localidad: se recorre, no se salta",
 "A MEDIAS — nuestros accesos son hashes: localidad casi nula",
 "PARCIAL, con razon escrita",
 "Las paginas grandes arreglan la TLB -600 entradas en vez de 123.904- y eso si aplica. Lo que "
 "NO arreglan es la cache, que es donde esta el problema. La premisa encaja para la mitad del "
 "beneficio y no para la otra, y eso esta bien siempre que no se cite como si fueran las dos."),

("hash criptografico para deduplicar (blake3)", "sistemas donde un adversario puede fabricar colisiones",
 "alguien podria construir dos contenidos con el mismo hash para que guardes el equivocado",
 "PROBABLEMENTE SI, pero no es gratis preguntarselo",
 "SIN MEDIR · baja prioridad",
 "Se trunca a 128 bits, asi que la fuerza criptografica completa ya no esta entera. La pregunta "
 "honesta no es si hace falta cripto: es cuanto costaria un hash no criptografico de 128 bits y "
 "si se nota. Si son 3 s de 400, no hay conversacion."),

("los niveles 1..19 de zstd", "una curva ajustada sobre corpus generales y maquinas generales",
 "el nivel N cuesta y compra lo que dice la tabla del autor",
 "NO — y ya nos costo una decision",
 "MEDIDO · ARREGLADO",
 "«El nivel 19 cuesta 34,8 veces mas que el 1» es cierto PARA EL CODEC SOLO y sobre otro corpus. "
 "Para el archivador entero son 1,79 veces. Tres numeros correctos y ninguno valido para lo del "
 "otro. Hoy `--price` CRONOMETRA en vez de leer la tabla."),
]

CAB = ["tecnica", "de_donde_viene", "su_premisa_original", "es_la_nuestra", "veredicto", "la_prueba"]

with open("bench/prestadas.tsv", "w", newline="") as f:
    f.write("# TECNICAS PRESTADAS: ¿de que problema venian, y es el nuestro?\n")
    f.write("# terceros.tsv lista QUE codigo de fuera usamos. Esto es otra cosa: una IDEA\n")
    f.write("# prestada no trae codigo, trae una PREMISA — el problema para el que se invento.\n")
    f.write("# Y una premisa deja de encajar sin que nadie lo note, porque no hay dependencia\n")
    f.write("# que actualizar ni linea que cambie.\n")
    f.write("# Cada fila dice CON QUE se comprueba. «Sospecho que no encaja» sin prueba al lado\n")
    f.write("# es una queja. Lo escribe bench/prestadas.py.\n")
    w = csv.writer(f, delimiter="\t"); w.writerow(CAB); w.writerows(P)

c = collections.Counter(r[4].split()[0] for r in P)
print(f"\n  TECNICAS PRESTADAS — {len(P)}\n")
print(f"  {'tecnica':<38}{'¿es nuestra la premisa?':<16}{'veredicto'}")
print("  " + "-" * 92)
for r in P:
    corta = r[3].split("—")[0].split("-")[0].strip()
    print(f"  {r[0][:36]:<38}{corta[:14]:<16}{r[4][:44]}")
print()
print("  " + " · ".join(f"{k}: {v}" for k, v in c.most_common()))
print()
print("  EL HUECO MAS GRANDE es el FRAME, y no es de codigo: nadie ha comprobado nunca si")
print("  alguien saca ficheros sueltos de un respaldo de 2,6 GB. Si no lo hace, el frame")
print("  pequeño es coste puro — y ya esta medido lo que cuesta: -3,833% de bytes con MENOS")
print("  memoria al pasar a 64 MiB. Es la unica fila que no se contesta con un instrumento.")
print()
