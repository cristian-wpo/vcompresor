# -*- coding: utf-8 -*-
"""LAS JUNTURAS — la ley R11, con comprobacion.

LA LEY (dueño, 2026-07-31): «Todas las herramientas deben estar conectadas y pasarse informacion
de forma perfecta, ida y vuelta.»

POR QUE HACE FALTA UN INSTRUMENTO PROPIO Y NO BASTA CON MEDIR CADA PIEZA. El fallo mas caro
encontrado hoy no vivia DENTRO de ninguna herramienta: el archivador elegia frames de 64 MiB y le
pasaba a zstd una linea sin un solo parametro, asi que zstd usaba su ventana por defecto de 4 MiB.
Dieciseis a uno. Las dos piezas median bien por separado —el archivador da sus bytes y su reloj,
zstd hace exactamente lo que le piden— y el numero que las descuadra NO SALE EN NINGUNA DE LAS DOS
BASCULAS. Una bascula por herramienta es ciega a la juntura por construccion.

DOS PARTES, y la primera es automatica:

 1. EL DETECTOR. Casi toda libreria de fuera tiene dos caras: una funcion de un disparo que se lo
    traga todo por defecto, y una version configurable. Llamar a la de un disparo es, literalmente,
    NO pasarle lo que sabemos. Esto recorre el codigo y las señala. Habria cazado el fallo de hoy.

 2. EL CENSO. Una fila por traspaso: que deberia cruzar, si cruza, si la ida y vuelta esta
    demostrada, y CUANTO cuesta el hueco. Un coste sin medida al lado no entra: se marca SIN MEDIR.
    Lo contrario es una lista de sospechas con formato de tabla.

Lo escribe bench/junturas.py.
"""
import csv, os, re, subprocess, sys

FUENTE = ["crates/vc-core/src", "crates/vc-cli/src", "crates/vc-cm/src", "crates/vlz/src"]

# ── 1. EL DETECTOR: llamadas de un disparo a librerias de fuera ────────────────────────────────
#
# clave de busqueda · que se traga · cual es la version que SI acepta lo que sabemos
UN_DISPARO = [
    ("zstd::bulk::compress(",   "nivel y nada mas: ventana, LDM, estrategia y hash van por defecto",
     "zstd::bulk::Compressor + set_parameter"),
    ("zstd::bulk::decompress(", "el tope de ventana del descompresor",
     "zstd::bulk::Decompressor + set_parameter"),
    ("zstd::encode_all(",       "todo salvo el nivel", "Compressor con parametros"),
    ("bincode::serialize(",     "la configuracion de codificacion (varint, endianness, limite)",
     "bincode::options() o DefaultOptions"),
    ("blake3::hash(",           "nada configurable, pero SI recalcula si ya se hasheo antes",
     "reutilizar el hash que ya tiene el troceador"),
    ("std::fs::read(",          "lee el fichero ENTERO otra vez, aunque ya se leyera para firmarlo",
     "pasar el buffer que ya se leyo"),
    ("flate2::",                "el nivel y la estrategia de zlib", "Compress::new_with_window_bits"),
]

def busca(clave):
    """Donde se llama, con fichero y linea. Sin ripgrep: en este servidor no esta."""
    fuera = []
    for raiz in FUENTE:
        for d, _, fs in os.walk(raiz):
            for f in sorted(fs):
                if not f.endswith(".rs"): continue
                p = os.path.join(d, f)
                try: ls = open(p, encoding="utf-8").read().splitlines()
                except OSError: continue
                for i, l in enumerate(ls, 1):
                    # Un comentario que NOMBRA la llamada no es una llamada.
                    s = l.strip()
                    if s.startswith("//") or s.startswith("///") or s.startswith("*"): continue
                    if clave in l: fuera.append((p, i, s[:110]))
    return fuera

print("\n  1. LLAMADAS DE UN DISPARO A LIBRERIAS DE FUERA")
print("     Cada una es informacion nuestra que NO cruza la juntura.\n")
detectadas, n_sitios = [], 0
for clave, traga, mejor in UN_DISPARO:
    sitios = busca(clave)
    if not sitios: continue
    n_sitios += len(sitios)
    print(f"  {clave}")
    print(f"     se traga: {traga}")
    print(f"     acepta lo que sabemos: {mejor}")
    for p, i, s in sitios[:6]:
        print(f"       {p}:{i}   {s}")
    if len(sitios) > 6: print(f"       ... y {len(sitios)-6} mas")
    print()
    detectadas.append((clave, traga, mejor, len(sitios),
                       " ".join(f"{p}:{i}" for p, i, _ in sitios[:8])))
if not detectadas:
    print("     ninguna\n")

# ── 2. EL CENSO DE JUNTURAS ────────────────────────────────────────────────────────────────────
#
# de · a · que deberia cruzar · cruza · ida y vuelta · con que se demuestra · cuesta · medida
#
# «cruza»:     SI · NO · A MEDIAS
# «ida y vuelta»: SI · NO · NO PROCEDE — y en columna APARTE con QUE se demuestra.
#   El estado y su prueba van separados A PROPOSITO. En la primera version iban en el mismo
#   campo y la tabla imprimio «NO» en la fila que MEJOR demostrada esta, porque el texto
#   empezaba por «verify». Una columna que mezcla el veredicto con su justificacion no se puede
#   leer ni a ojo ni a maquina.
# «cuesta»:    una cifra con su medida al lado, o SIN MEDIR. Nunca una estimacion a ojo.
J = [
 ("el archivador", "zstd", "el tamaño del frame, para que la ventana lo cubra",
  "SI — desde 31-07", "SI",
  "verify del archivo + 32 idas y vueltas en la sonda con el descompresor SIN configurar",
  "-1,365% de bytes y 0,932x de CPU",
  "bench/ventana.py · 8 trozos de 64 MiB repartidos · commit db76ff6"),

 ("el archivador", "la sonda de --price", "el tamaño del frame, para predecir lo que hara",
  "SI — desde 31-07", "NO PROCEDE", "la sonda no produce archivo, produce una prediccion",
  "SIN MEDIR — la sonda comprimia la muestra ENTERA de una vez, o sea con mas alcance del que el "
  "producto va a tener nunca: prediccion optimista sobre la que se decide el precio",
  "lo destapo bench/junturas.py al correrlo por primera vez"),

 ("vdeflate", "zstd", "las coincidencias que el DEFLATE de entrada YA TRAE",
  "NO", "NO PROCEDE", "hoy no cruza nada",
  "comprimir son 133,28 s de los 197,46 s de CPU del programa (67,5%), y el plano se expande 17x "
  "-de 357 MB a 6,03 GB- solo para que zstd busque otra vez lo que ya estaba encontrado",
  "piramide VC_FASES=1 · bin/vc-zstd-perfil"),

 ("preflate", "preflate_seguro", "su propia verificacion: re-deflata el plano ENTERO para comprobarse",
  "SI — desde 31-07", "SI", "los dos .vc con el MISMO sha256 y el mismo conjunto aceptado",
  "ERA UN DUPLICADO EXACTO, no una red de seguridad. preflate_seguro ya recreaba el flujo y lo "
  "comparaba byte a byte, y ADEMAS le pasaba PreflateConfig::default() -verify_compression: true- "
  "que le hace recrearlo entero otra vez por dentro. Y el validador interno solo hace UNA cosa al "
  "fallar: Err(RoundtripMismatch) -stream_processor.rs:145 y :211-. No corrige, no reintenta: "
  "rechaza. Su Err cae por nuestro .ok()? y da None; nuestro `rec != cuerpo` da None tambien. "
  "Mismo conjunto aceptado por dos caminos. Se quita la SUYA y se queda la NUESTRA -compara contra "
  "los bytes que hay que reproducir, vive en nuestra capa y va dentro del catch_unwind-. "
  "AHORRO: -25,9% del reloj del archivador entero en PNG puro (80% de DEFLATE) y -10,2% en corpus "
  "mixto (23% de DEFLATE). Escala con cuanto DEFLATE lleve el dato, como tenia que ser.",
  "A/B/A/B en serie · PNG: 958 ficheros repartidos, 249 MB, 947 flujos, 59,2 s -> 43,8 s, los dos "
  ".vc con sha256 identico y 182.834.586 B, ida y vuelta bit a bit 958/958 · MIXTO: corpus-mac, "
  "570 MB, 11 tipos, 10.824 ficheros, 638 flujos, 29,5 s -> 26,5 s, los dos 304.401.584 B. "
  "commit aff005e"),

 ("vc-cm: el hash", "vc-cm: el match", "que la coincidencia sea REAL antes de creersela",
  "SI — desde 31-07 (v4)", "SI", "ida y vuelta OK en los cinco tipos, y la v3 se sigue abriendo",
  "mm_hashtab[mh] se leia y se usaba SIN COMPARAR NI UN BYTE. 2^21 ranuras para tramos de 32 MiB "
  "= unas 16 posiciones por ranura: colisionar no era el caso raro, era lo normal. FALSOS: codigo "
  "22,79% · sql 6,00% · logs 2,11% · csvjson 14,90% · bin 36,98%. Comparar los 6 bytes que "
  "preceden al candidato contra los 6 de la posicion actual -exactamente lo que el hash afirma- "
  "da -0,0496% · -0,0403% · -0,0233% · -0,0798% · -0,2837%, ponderado -0,1399%. Mejora los CINCO: "
  "eso es lo que lo hace un arreglo y no una perilla. Cuesta un memcmp de 6 y solo en los "
  "arranques.",
  "VC_MM_STATS=1 y VC_MM_VERIF=1 · 4 MiB de cada tipo · bench/registros-m3 · commit 08418ba"),

 ("recorrer", "trocear", "los atributos del fichero, que ya se leyeron al recorrer",
  "A MEDIAS", "NO PROCEDE", "son metadatos, no contenido",
  "5,7 stat y 3,7 open POR FICHERO", "bench/syscalls.tsv"),

 ("enrutar", "trocear", "el buffer que firma_de() ya leyo, en vez de volver a leer el fichero",
  "NO", "NO PROCEDE", "son los mismos bytes leidos dos veces",
  "SIN MEDIR — es la deuda D5, y hasta que se mida no se sabe si vale la pena",
  "—"),

 ("el archivador", "vc-cm", "el tamaño del frame (32 MiB con --max)",
  "SI", "SI", "determinismo verificado, bench/determinismo-vccm.txt",
  "medido al reves: 14 frames de 8 MiB dan 11.598.908 B y 4 de 32 MiB dan 11.057.710, un 4,7% menos",
  "el comentario de frame_target() en vc-core/src/lib.rs, con su medida"),

 ("un tramo de vc-cm", "el siguiente tramo", "EL MODELO YA APRENDIDO",
  "NO", "NO PROCEDE", "no cruza nada: se tira entero y se construye otro",
  "`vc_cm::comprimir` hace `Predictor::new` en CADA llamada. Con --max los tramos son de 32 MiB, "
  "asi que sobre un arbol de 2,6 GB el modelo aprende y OLVIDA unas ochenta veces. Medido en "
  "parte: 7 tramos cuestan +1,054% frente a 2 sobre 207 MiB de 4.184 ficheros distintos. Un "
  "compresor de contextos mejora cuanto mas ha visto — esto es ratio tirado, no solo tiempo.",
  "bench/vccm-sin-medir.tsv · M1, medida del 30-07. NO se cierra con el instrumento de hoy: "
  "cambiar el tamaño del tramo cambia tambien QUE FICHEROS CAEN JUNTOS, y los dos efectos son "
  "del mismo orden. Separarlos exige poder arrastrar el modelo entre tramos — o sea, el propio "
  "arreglo. La medida y la solucion son el mismo trabajo."),

 ("todos los buscadores", "todos los buscadores",
  "«¿donde vi estos bytes antes?» — la MISMA pregunta, sobre los MISMOS bytes",
  "NO", "NO PROCEDE", "cada uno mantiene su indice y ninguno se lo dice a los demas",
  "En UNA pasada del archivador conviven cuatro indices independientes sobre los mismos bytes: "
  "vc-cm con 5 tablas de posiciones (~72 MB), zstd/vlz con las suyas, vdeflate con cadenas de "
  "hash sobre el mismo plano, y vc-cdc con un hash rodante byte a byte. MEDIDO por que importa: "
  "el racimo match vale 1,8974% y UNO SOLO de los cinco entrega el 66% — o sea que las cinco "
  "tablas no compran cinco cosas, compran una cinco veces. Y comprimir es el 67,5% de la CPU.",
  "bench/registros-m3/racimos.tsv (Mac, 4 MiB de codigo, commit 7be26b4) + piramide VC_FASES=1"),

 ("no-lineal:dispersion", "el mezclador", "una prediccion que sirva PARA ESTE TIPO DE DATO",
  "NO — es la misma para todos", "NO PROCEDE", "no hay ida y vuelta: es un modelo, no un traspaso",
  "SE MIDIO POR UNA HIPOTESIS QUE SALIO FALSA, y el hallazgo esta en el negativo. La sospecha era "
  "que la dispersion -83,8% del peso del mezclador, y apagarla MEJORA el texto- estaba aprendiendo "
  "a compensar el ruido de los arranques falsos del match. NO: apagarla gana -0,9201% en logs con "
  "el match viejo y -0,8970% con el verificado. Practicamente lo mismo, o sea independientes. "
  "Lo que si deja claro es que NO ES UN SI-O-NO SINO UN POR TIPO: apagada gana en logs (-0,9201%) "
  "y sql (-0,0869%); encendida gana en bin (+0,3756%) y csvjson (+0,0082%). Hoy va encendida "
  "SIEMPRE, o sea que en logs estamos pagando casi un 1% por no preguntarle al dato. El mecanismo "
  "para elegir por tipo ya existe -elige_familia- y esta medido como cacheable en 4 de 5 tipos.",
  "2x2 con VC_APAGAMASK=2199023255552 (indice 41) x VC_MM_VERIF · 4 MiB de cada tipo · commit 08418ba"),

 ("vc-cdc", "dedup", "el hash del trozo, que el troceador ya calcula",
  "SI", "SI", "el dedup compara por hash y el archivo verifica entero",
  "dedup = 99,94% del espacio y 0% del tiempo", "bench/incremental-mide.sh"),

 ("la instantanea previa", "el archivador", "que ficheros no han cambiado, para no rehacerlos",
  "SI — desde 31-07 (D12)", "SI", "bytes identicos, bench/incremental-verifica.sh 10/10",
  "9,8x de reloj, 24,3x de CPU y 5,07x de memoria", "bench/incremental-mide.sh"),

 ("vc-jpeg", "el archivador", "el JPEG reconstruido bit a bit desde JXL",
  "SI", "SI", "bit a bit",
  "SIN MEDIR el coste de la juntura", "—"),

 ("vlz", "el archivador", "sustituir a zstd como codec del producto",
  "NO — todavia no esta enchufado", "SI", "8/8 ida y vuelta dentro de vlz",
  "SIN MEDIR — su ratio contra zstd no ha pasado por la bascula, a proposito: primero la ida y "
  "vuelta demostrada, despues el ratio",
  "crates/vlz · 8/8 tests"),
]

CAB = ["de", "a", "que_deberia_cruzar", "cruza", "ida_y_vuelta", "como_se_demuestra",
       "cuanto_cuesta", "con_que_se_midio"]

# LAS REGLAS DE ADMISION, y saltan aqui mismo. Sin esto la tabla se llena de sospechas con
# formato de dato, que es peor que no tenerla: parece medido.
malas = [f for f in J if re.search(r"\d", f[6]) and not f[6].startswith("SIN MEDIR") and f[7] == "—"]
for f in malas:
    print(f"  ABORTA: «{f[0]} -> {f[1]}» declara un coste con cifras y no dice con que se midio")
sueltos = [f for f in J if f[4] not in ("SI", "NO", "NO PROCEDE") and not f[4].startswith("SI ")]
for f in sueltos:
    print(f"  ABORTA: «{f[0]} -> {f[1]}» tiene un estado de ida y vuelta que no es SI/NO/NO PROCEDE: {f[4]!r}")
if malas or sueltos: sys.exit(2)

os.makedirs("bench", exist_ok=True)
with open("bench/junturas.tsv", "w", newline="") as fh:
    fh.write("# LAS JUNTURAS — la ley R11: todas las herramientas conectadas, con ida y vuelta perfecta.\n")
    fh.write("# El fallo mas caro de hoy no vivia DENTRO de ninguna pieza: vivia entre dos. Una\n")
    fh.write("# bascula por herramienta es ciega a la juntura por construccion.\n")
    fh.write("# «cuanto_cuesta» sin «con_que_se_midio» no entra: el generador aborta.\n")
    w = csv.writer(fh, delimiter="\t"); w.writerow(CAB); w.writerows(J)

with open("bench/junturas-undisparo.tsv", "w", newline="") as fh:
    fh.write("# LLAMADAS DE UN DISPARO A LIBRERIAS DE FUERA — informacion nuestra que no cruza.\n")
    fh.write("# Detectadas recorriendo el codigo, no escritas a mano. Esto habria cazado el fallo\n")
    fh.write("# de la ventana: zstd::bulk::compress(datos, nivel) y ni un parametro mas.\n")
    w = csv.writer(fh, delimiter="\t")
    w.writerow(["llamada", "que_se_traga", "la_version_que_si_acepta", "sitios", "donde"])
    w.writerows(detectadas)

print("  2. EL CENSO DE JUNTURAS\n")
print(f"  {'de':<22}{'a':<21}{'cruza':<12}{'ida y vuelta':<14}{'coste'}")
print("  " + "-" * 96)
for f in J:
    cr = "SÍ" if f[3].startswith("SI") else ("a medias" if f[3] == "A MEDIAS" else "NO")
    iv = {"SI": "sí", "NO": "NO", "NO PROCEDE": "—"}[f[4] if f[4] in ("SI","NO","NO PROCEDE") else "SI"]
    co = "sin medir" if f[6].startswith("SIN MEDIR") else "medido"
    print(f"  {f[0]:<22}{f[1]:<21}{cr:<12}{iv:<14}{co}")
print()
n_no = sum(1 for f in J if f[3].startswith("NO"))
n_sin = sum(1 for f in J if f[6].startswith("SIN MEDIR"))
n_iv = sum(1 for f in J if f[4] == "NO")
print(f"  {len(J)} junturas · {n_no} por las que HOY no cruza nada · {n_sin} sin medir lo que cuestan"
      f" · {n_iv} sin ida y vuelta demostrada")
print(f"  {len(detectadas)} formas de llamada de un disparo, en {n_sitios} sitios del codigo")
print("\n  bench/junturas.tsv · bench/junturas-undisparo.tsv\n")
