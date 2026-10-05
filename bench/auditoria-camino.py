#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""LAS 12 PREGUNTAS, CORRIDAS SOBRE CADA PIEZA NUESTRA.

Encargo del dueño: «hay que correr estas 12 preguntas sobre todo lo que ya hemos creado
nosotros para el compresor y apuntar sus respuestas — por cada cosa que hemos hecho tipo
libreria o propio como herramienta, arrancar las preguntas».

QUE ENTRA. Las diez piezas que son NUESTRAS: las nueve librerias del binario mas el banco de
pruebas, que tambien es cosa nuestra y tambien se audita. 8.559 lineas de Rust y unas 3.000 de
guiones. Lo de terceros (blake3, el cifrado, zstd, libjxl, libc) NO entra aqui: para eso esta
la tabla de terceros, que pregunta otra cosa —si podriamos hacerlo nosotros— y no como esta
hecho por dentro.

COMO SE LEE CADA CELDA:
    HALLAZGO    hay desperdicio, con evidencia y con fichero:linea
    LIMPIO      se miro y no lo hay, y se dice POR QUE se puede afirmar
    NO APLICA   la pregunta no tiene sentido para esta pieza (y se dice por que)
    SIN MIRAR   no se ha mirado todavia. NO es lo mismo que limpio.

POR QUE «SIN MIRAR» APARECE Y NO SE ESCONDE. Una auditoria que solo enseña lo que encontro
parece completa y no lo esta. Las celdas sin mirar son el mapa de lo que queda, y son la parte
util para mañana. En esta primera pasada hay 120 celdas y no todas estan cerradas.
"""
import csv
import collections

# LAS PREGUNTAS SE LEEN, NO SE COPIAN. Estaban copiadas aqui, y al añadir P13 y P14 esta
# tabla se habria quedado en doce sin decir nada —imprimiendo «10 piezas x 12 preguntas», que
# es una frase perfectamente creible—. Es el mismo fallo que tenia veredictos.py, cometido en
# la tabla que lo denuncia.
PREG = {}
with open("bench/preguntas-camino.tsv") as _f:
    _fil = [l.rstrip("\n").split("\t") for l in _f if not l.startswith("#")]
_cab = _fil[0]
for _r in _fil[1:]:
    if len(_r) == len(_cab):
        _d = dict(zip(_cab, _r))
        PREG[_d["id"]] = _d["la_pregunta"]
assert PREG, "no se han leido las preguntas"

NA_LIB = "NO APLICA — no es un instrumento de medida; esta pregunta es para el banco."
NA_MEM = "NO APLICA — trabaja sobre memoria, no toca el nucleo."

PIEZAS = [
("vc-cdc", "crates/vc-core/src/cdc.rs", 166, "Troceado por contenido. Sustituyo a fastcdc.", {
  "P1": ("NO APLICA", NA_MEM + " TrozosFlujo lee por bloques de max*3, no byte a byte."),
  "P2": ("HALLAZGO", "sketch() rueda gear y hashea blake3 sobre el primer MiB de cada fichero, y "
         "la fase de leer+trocear vuelve a rodar y a hashear esos mismos bytes. Para la mayoria "
         "del arbol (mediana 1.972 B) ese «primer MiB» es el fichero entero."),
  "P3": ("LIMPIO", "Vec::with_capacity(max*3) = 48 KB, dimensionado al PARAMETRO y no a un "
         "numero magico. Es la reserva correcta: hace falta ver max*2 para poder cortar."),
  "P4": ("HALLAZGO", "corte() calcula la huella gear de cada byte y la TIRA al devolver la "
         "longitud. Podria servir de identificador barato del trozo. Cuidado: la condicion de "
         "corte fija sus bits bajos a cero, asi que es un identificador mas debil de lo que "
         "parece. LA PRUEBA QUE LO CIERRA: contar colisiones de la huella de corte contra "
         "blake3 sobre el corpus. Sin esa prueba no se puede usar."),
  "P5": ("NO APLICA", "es una sola fase; el orden lo decide lib.rs."),
  "P6": ("HALLAZGO", "CDC_MIN=64K, CDC_AVG=256K, CDC_MAX=1M no se han barrido NUNCA: no hay ni "
         "una fila en bench/historia.jsonl. Y el 88,3% de los ficheros del arbol real esta por "
         "DEBAJO de CDC_MIN, asi que para ellos el troceador no rueda ni un byte. La perilla que "
         "decide todo el dedup es la unica que nadie ha tocado."),
  "P7": ("HALLAZGO", "el doc del modulo dice «eso se mide (ver bench/f2.8-deps.tsv)» y ESE "
         "FICHERO NO EXISTE — el real es f2.7-deps.tsv. Una cita a un fichero que no esta es "
         "peor que no citar: parece medido y no se puede comprobar."),
  "P8": ("HALLAZGO", "sketch trocea con 1024/4096/16384 y el camino principal con 64K/256K/1M, "
         "sobre los mismos bytes. Dos pasadas de rodadura por no haberlas pensado juntas. El "
         "arreglo es un solo recorrido con DOS mascaras."),
  "P9": ("LIMPIO", "no arrastra dependencias ni casos raros: son 120 lineas sin condicionales "
         "de plataforma."),
  "P10": ("NO APLICA", NA_LIB), "P11": ("NO APLICA", NA_LIB), "P12": ("NO APLICA", NA_LIB)}),

("vc-inflate", "crates/vc-core/src/inflate.rs", 324, "DEFLATE propio, fase 1: descomprimir.", {
  "P1": ("LIMPIO", "no hay ni una llamada al sistema: trabaja sobre un slice que le dan."),
  "P2": ("HALLAZGO", "Bits::bit() toca el mismo byte hasta OCHO veces, con comprobacion de "
         "limites en cada una, y bits(n) lo llama n veces en un bucle. Decodificar un codigo "
         "Huffman de 9 bits son 9 comprobaciones de limites. Es la decision deliberada estilo "
         "puff —claridad y correccion primero— y el precio son los 285 MB/s medidos."),
  "P3": ("HALLAZGO", "Vec::with_capacity(d.len()*4): una conjetura de 4x. El tamaño EXACTO del "
         "plano ya esta guardado en Hueco.len_plano y no se le pasa. Si el flujo comprime mas "
         "de 4x, realoja y copia; si comprime menos, reserva de mas."),
  "P4": ("HALLAZGO", "Huff::lee recorre las longitudes 1..16 simbolo a simbolo. Los conteos "
         "canonicos YA estan calculados en arboles_dinamicos: con ellos se monta una tabla de "
         "9 bits que resuelve la mayoria de simbolos en un solo paso."),
  "P5": ("NO APLICA", "una sola fase."),
  "P6": ("NO APLICA", "no tiene perillas: el formato lo fija RFC 1951."),
  "P7": ("LIMPIO", "las dos afirmaciones que hace —285 MB/s y 1.973 de 1.973 identicos— salen "
         "de bin/vc-inflate contra flujos DEFLATE reales, no de una opinion."),
  "P8": ("LIMPIO", "deflate.rs solo LOCALIZA flujos dentro de otros formatos; no duplica el "
         "codec. La separacion esta limpia y dicha en el doc."),
  "P9": ("LIMPIO", "sin condicionales de plataforma ni dependencias."),
  "P10": ("NO APLICA", NA_LIB), "P11": ("NO APLICA", NA_LIB), "P12": ("NO APLICA", NA_LIB)}),

("vc-formato", "crates/vc-core/src/formato.rs", 462, "Serializacion del indice. Sustituyo a serde+bincode.", {
  "P1": ("NO APLICA", NA_MEM),
  "P2": ("SIN MIRAR", "no se ha revisado el camino de lectura buscando dobles pasadas."),
  "P3": ("HALLAZGO", "Esc arranca con Vec::new() y crece doblando. Para un indice de 97 MB son "
         "~24 realojos y ~194 MB de memcpy acumulado, todo evitable: al empezar ya se sabe el "
         "numero de ficheros y de trozos, que es lo que hace falta para estimar."),
  "P4": ("SIN MIRAR", "no se ha buscado si el camino de lectura recalcula algo que ya tenia."),
  "P5": ("NO APLICA", "una sola fase."),
  "P6": ("LIMPIO", "la decision esta MEDIDA contra bincode: 955 bytes peor de 97 MB (0,001%) a "
         "cambio de quitar dos dependencias. bench/f2.7-deps.tsv."),
  "P7": ("LIMPIO", "las dos razones del doc —durabilidad del formato y varints/deltas— tienen "
         "la medida al lado."),
  "P8": ("HALLAZGO", "cada transformada se serializa entera en un sub-buffer y luego se copia "
         "al principal con extend_from_slice. Se escribe dos veces lo mismo. HAY RAZON —el "
         "sub-buffer se hashea para deduplicar referencias— pero el segundo trayecto se puede "
         "evitar escribiendo en sitio y parcheando la longitud despues."),
  "P9": ("LIMPIO", "sin casos raros que paguen todos."),
  "P10": ("NO APLICA", NA_LIB), "P11": ("NO APLICA", NA_LIB), "P12": ("NO APLICA", NA_LIB)}),

("vc-deflate", "crates/vc-core/src/deflate.rs", 185, "Localiza flujos DEFLATE dentro de otros formatos.", {
  "P1": ("LIMPIO", "trabaja sobre un buffer que le dan."),
  "P2": ("SIN MIRAR", "no se ha comprobado si el olfateo recorre el fichero mas de una vez."),
  "P3": ("HALLAZGO", "Vec::new() para los tramos y para la salida, sin reserva previa. Es menor "
         "—son listas cortas— pero el numero de tramos se puede acotar por el tamaño."),
  "P4": ("SIN MIRAR", ""), "P5": ("NO APLICA", "una sola fase."),
  "P6": ("SIN MIRAR", "los umbrales de deteccion (PREFLATE_MAX, minimo de 16 B) no se han barrido."),
  "P7": ("LIMPIO", "el doc no afirma coste; afirma cobertura (.zip .png .docx .gz...) y eso es "
         "verificable leyendo el codigo."),
  "P8": ("LIMPIO", "no duplica nada de inflate.rs."), "P9": ("LIMPIO", ""),
  "P10": ("NO APLICA", NA_LIB), "P11": ("NO APLICA", NA_LIB), "P12": ("NO APLICA", NA_LIB)}),

("vc-jpeg", "crates/vc-core/src/jpeg.rs", 820, "JPEG→JXL propio. En obra, sustituye a libjxl.", {
  "P1": ("LIMPIO", "trabaja sobre el buffer del fichero."),
  "P2": ("SIN MIRAR", ""),
  "P3": ("HALLAZGO", "Pieza::Lit(d[lit_desde..].to_vec()) COPIA tramos del fichero en vez de "
         "guardar rangos del buffer original. Para un JPEG de 5 MB con varias piezas literales "
         "son megas copiados. Antes de arreglarlo hay que comprobar la vida util del buffer: "
         "si no sobrevive, la copia es necesaria y entonces es LIMPIO."),
  "P4": ("SIN MIRAR", ""), "P5": ("NO APLICA", "una sola fase."),
  "P6": ("SIN MIRAR", "no se ha barrido ningun parametro propio del carril."),
  "P7": ("LIMPIO", "lo que afirma esta medido y es su mejor cifra: 99,45% byte-identicos sobre "
         "10.642 JPEG reales, 0 divergencias, 28,0 MB/s contra los 6,4 de libjxl."),
  "P8": ("SIN MIRAR", "convive con el carril libjxl; no se ha mirado si repiten trabajo."),
  "P9": ("LIMPIO", ""), "P10": ("NO APLICA", NA_LIB), "P11": ("NO APLICA", NA_LIB),
  "P12": ("NO APLICA", NA_LIB)}),

("vc-cripto", "crates/vc-core/src/cripto.rs", 260, "Cifrado y derivacion de clave (envuelve a terceros).", {
  "P1": ("LIMPIO", "salvo leer el fichero de contraseña, una vez."),
  "P2": ("LIMPIO", ""),
  "P3": ("LIMPIO", "Vec::with_capacity(claro.len() + SOBRECOSTE): EXACTO, no una conjetura. Es "
         "la reserva mejor hecha de todo el arbol."),
  "P4": ("SIN MIRAR", ""), "P5": ("NO APLICA", ""),
  "P6": ("SIN MIRAR", "los parametros de Argon2 (memoria, pasadas) no se han barrido — y ahi "
         "la perilla no es velocidad sino resistencia, asi que el criterio es otro."),
  "P7": ("LIMPIO", "la afirmacion importante es por que NO lo hacemos nosotros, y esta dicha "
         "con su razon: en criptografia el fallo NO SE VE en las pruebas."),
  "P8": ("LIMPIO", ""),
  "P9": ("HALLAZGO", "33 crates, la dependencia mas gorda del arbol, en el binario de TODOS "
         "aunque casi nadie cifre. Ya esta en la tabla de terceros como SE QUEDA con motivo, "
         "pero la pregunta sigue viva: podria ir tras una caracteristica apagada por defecto."),
  "P10": ("NO APLICA", NA_LIB), "P11": ("NO APLICA", NA_LIB), "P12": ("NO APLICA", NA_LIB)}),

("vc-cm", "crates/vc-cm/src/lib.rs", 1945, "Codec de mezcla de contextos. Nuestra alternativa a zstd-19.", {
  "P1": ("LIMPIO", "solo el mmap de las tablas, una vez por tramo."),
  "P2": ("LIMPIO", "una pasada por byte; el coste esta en las tablas, no en releer."),
  "P3": ("LIMPIO", "las tablas se reservan con mmap SIN TOCAR, asi que solo se materializa lo "
         "que se usa. Y VC_MEM se BARRIO de verdad (F2.52, cuatro tamaños): es la unica perilla "
         "del proyecto con barrido hecho."),
  "P4": ("HALLAZGO", "DOS, y el primero es el desperdicio mas caro del proyecto. (a) El modelo "
         "se REINICIA en cada tramo: con --max los tramos son de 32 MiB, asi que sobre un arbol "
         "de 2,6 GB aprende y OLVIDA ochenta veces. Un compresor de contextos mejora cuanto mas "
         "ha visto: eso es ratio tirado, no solo tiempo. SIGUE SIN MEDIR (M1). (b) El mezclador "
         "aprende un peso por modelo, VC_DUMP_W los vuelca, y NINGUNA tabla del banco los usa: "
         "se calculan y se tiran (M4)."),
  "P5": ("SIN MIRAR", "el orden interno de los 34 modelos no se ha mirado como tuberia."),
  "P6": ("LIMPIO", "VC_MEM barrido. Y la piramide dice donde esta el gasto: 83,7% en predecir+actualizar -de los que 14,3% es explicitamente buscar y escribir el cubo- en "
         "tablas de 484 MiB, y 5,5% en el mezclador (2,8 de mezclar + 2,7 de actualizar sus pesos: la "
         "cifra vieja de 2,8% dejaba fuera la mitad) — asi que la perilla que importa "
         "es el TAMAÑO de las tablas, que es justo la que se barrio."),
  "P7": ("SOSPECHA", "dos comentarios afirman «no cuesta ni un fallo de memoria mas» (l.886) y "
         "«no cuesta ni una instruccion mas» (l.1439). Son afirmaciones fuertes sobre el camino "
         "caliente. Hay que comprobar si tienen medida al lado o son razonamiento."),
  "P8": ("SIN MIRAR", "M2 y M3: la ablacion es por GRUPOS de 8, no por los 34 modelos, y nadie "
         "ha medido la correlacion entre pares. Dos modelos que predicen lo mismo cuestan el "
         "doble y aportan poco mas."),
  "P9": ("LIMPIO", "no entra en el binario si no se pide --max... (verificar en el enlazado)."),
  "P10": ("NO APLICA", NA_LIB), "P11": ("NO APLICA", NA_LIB), "P12": ("NO APLICA", NA_LIB)}),

("vc-cli", "crates/vc-cli/src/main.rs", 503, "Parseo de argumentos propio. Sustituyo a clap.", {
  "P1": ("LIMPIO", "solo lee argv."),
  "P2": ("LIMPIO", ""),
  "P3": ("LIMPIO", "un Vec<String> de argv y nada mas."),
  "P4": ("LIMPIO", ""), "P5": ("NO APLICA", ""),
  "P6": ("HALLAZGO", "--price sondea los datos del usuario para elegir el NIVEL de zstd, que "
         "entre 17 y 19 mueve un 0,8% del tamaño, y NO toca los carriles, que van de 0,15x a "
         "5,92x de retorno segun el precio del almacenamiento. Optimiza la perilla que no "
         "importa. Es el arreglo mas rentable de todo el censo."),
  "P7": ("SIN MIRAR", "el modelo de coste de --price (0,0639 EUR/h por defecto) no se ha "
         "contrastado con la factura real del servidor."),
  "P8": ("LIMPIO", ""),
  "P9": ("HALLAZGO", "fusefs.rs mete 13 crates en el binario de TODO EL MUNDO por un `mount` "
         "que solo existe en Linux y que casi nadie usa."),
  "P10": ("NO APLICA", NA_LIB), "P11": ("NO APLICA", NA_LIB), "P12": ("NO APLICA", NA_LIB)}),

("vc-core (tuberia)", "crates/vc-core/src/lib.rs", 2832, "Recorrido, enrutado y tuberia. Sustituyo a walkdir y filetime.", {
  "P1": ("HALLAZGO", "SEIS stat por fichero devolviendo la misma estructura: recorrer(), "
         "PROG_TOTAL_BYTES, PROG_BYTES, `let size`, meta_de() y duenos_de(). Las dos ultimas "
         "estan escritas una debajo de otra y llamadas una debajo de otra, para leer dos "
         "mitades del mismo struct. Sobre 43.298 ficheros son ~260.000 viajes al nucleo donde "
         "bastarian 43.298."),
  "P2": ("HALLAZGO", "hasta TRES aperturas del mismo fichero: agrupar lee 1 MiB y tira los "
         "bytes, firma_de abre para leer 8 bytes, y fs::read lo abre otra vez entero."),
  "P3": ("HALLAZGO", "lee_prefijo y lee_ventana hacen `vec![0u8; max]` — 1 MiB por fichero para "
         "un arbol de mediana 1.972 B. Ademas 1 MiB pasa del umbral de mmap de glibc, asi que "
         "cada fichero paga un mmap y un munmap que no harian falta."),
  "P4": ("HALLAZGO", "recorrer() pide el struct entero de cada entrada y se queda solo con un "
         "enum de UN BYTE. Todo lo demas —tamaño, modo, fecha, dueño— se vuelve a pedir despues."),
  "P5": ("HALLAZGO", "agrupar corre ANTES de la fase stat, asi que no puede saber el tamaño de "
         "los ficheros y tiene que reservar por el caso peor. No es un fallo de la funcion: es "
         "del ORDEN. Cambiar el orden arregla P3 sin tocar el algoritmo."),
  "P6": ("SIN MIRAR", "FRAME_TARGET (8/2/4 MiB por carril) y JXL_LOTE no se han barrido."),
  "P7": ("HALLAZGO", "«Los stat vienen calientes del recorrido, asi que cuesta milisegundos», "
         "escrito ENCIMA del bucle que hace 43.298 llamadas al nucleo, y nunca medido. Un "
         "comentario asi congela la pregunta: el siguiente que pasa lo lee y ya no comprueba."),
  "P8": ("HALLAZGO", "lee_prefijo (l.1030) y lee_ventana (l.993) son la misma funcion con un "
         "seek de diferencia — mismo cuerpo, mismo bucle y el MISMO defecto de reserva."),
  "P9": ("HALLAZGO", "el `mount` de Linux (fuser, 13 crates) lo paga el binario de todos."),
  "P10": ("NO APLICA", NA_LIB), "P11": ("NO APLICA", NA_LIB), "P12": ("NO APLICA", NA_LIB)}),

("el banco", "bench/mide.sh, veredictos.py, panel.py, aguja.sh", 3000, "Nuestras herramientas de medida. Tambien son nuestras y tambien se auditan.", {
  "P1": ("LIMPIO", "mide.sh llama al programa una vez y lee /proc una vez."),
  "P2": ("LIMPIO", ""),
  "P3": ("LIMPIO", "los guiones trabajan sobre TSV pequeños."),
  "P4": ("HALLAZGO ARREGLADO", "mide.sh mandaba el stderr del programa a un temporal y lo "
         "BORRABA: el desglose por fases costaba siete minutos de maquina y se tiraba. Arreglado "
         "hoy con -s."),
  "P5": ("NO APLICA", ""),
  "P6": ("LIMPIO", "el banco mide lo que decide: ciclos, bytes, carga y commit."),
  "P7": ("HALLAZGO ARREGLADO", "bench/registros/LEEME.md afirmaba «lo reproducen los guiones "
         "que los generaron, que estan en la historia de git de este commit». Era FALSO: el "
         "commit guardo los registros y los guiones vivian en /tmp. Corregido, y el fallo se "
         "deja escrito en vez de borrarlo."),
  "P8": ("LIMPIO", ""),
  "P9": ("LIMPIO", ""),
  "P10": ("HALLAZGO ARREGLADO", "veredictos.py tenia las medidas COPIADAS A MANO dentro del "
          "guion, sin la carga de la maquina. Publicaba una medida de carga 56,3 contra una de "
          "carga 6,5. Ahora lee de historia.jsonl y rechaza las parejas incomparables."),
  "P11": ("HALLAZGO ARREGLADO", "la ablacion de agrupar iba a comparar la configuracion por "
          "defecto contra --no-group DESPUES de que el defecto pasara a ser sin agrupar: habria "
          "publicado «agrupar cuesta 0,00%». Se pillo porque los bytes salieron identicos. El "
          "guion pide --group explicito y avisa si dos configuraciones sacan lo mismo."),
  "P12": ("HALLAZGO ARREGLADO", "ademas de mide.sh: el panel NO DECLARABA su codificacion, y "
          "arranca.sh lo sirve con python3 -m http.server, que manda Content-Type sin charset. "
          "La forma PREVISTA de abrir el panel rompia todos los acentos; abriendolo como fichero "
          "local se veia bien, que es lo que hacia que no se notara.")}),
]

filas = []
for pieza, donde, lineas, que, resp in PIEZAS:
    for pid in sorted(PREG, key=lambda x: int(x[1:])):
        v, texto = resp.get(pid, ("SIN MIRAR", ""))
        filas.append([pieza, donde, lineas, que, pid, PREG[pid], v, texto])

with open("bench/auditoria-camino.tsv", "w", newline="") as f:
    f.write("# LAS 12 PREGUNTAS DEL CAMINO, CORRIDAS SOBRE CADA PIEZA NUESTRA.\n")
    f.write("# HALLAZGO = hay desperdicio, con evidencia · LIMPIO = se miro y no lo hay, y se\n")
    f.write("# dice por que · NO APLICA = la pregunta no tiene sentido aqui · SIN MIRAR = no se\n")
    f.write("# ha mirado todavia, que NO es lo mismo que limpio.\n")
    f.write("# Las celdas SIN MIRAR se enseñan a proposito: una auditoria que solo enseña lo que\n")
    f.write("# encontro parece completa y no lo esta. Son el mapa de lo que queda.\n")
    f.write("# Lo escribe bench/auditoria-camino.py.\n")
    w = csv.writer(f, delimiter="\t")
    w.writerow(["pieza", "donde", "lineas", "que_es", "pregunta_id", "la_pregunta",
                "veredicto", "la_respuesta"])
    w.writerows(filas)

c = collections.Counter(r[6].split()[0] for r in filas)
print(f"bench/auditoria-camino.tsv: {len(PIEZAS)} piezas x {len(PREG)} preguntas = {len(filas)} celdas\n")
print("  " + " · ".join(f"{k} {v}" for k, v in c.most_common()))
print()
print("  %-20s %6s %9s %8s %9s %10s" % ("pieza", "lineas", "HALLAZGO", "LIMPIO", "SIN MIRAR", "NO APLICA"))
for pieza, _, lin, _, resp in PIEZAS:
    cc = collections.Counter(resp.get(p, ("SIN MIRAR", ""))[0].split()[0] for p in PREG)
    print("  %-20s %6d %9d %8d %9d %10d" %
          (pieza, lin, cc["HALLAZGO"], cc["LIMPIO"], cc["SIN"], cc["NO"]))
print()
print("  LOS HALLAZGOS, por pieza:")
for pieza, _, _, _, resp in PIEZAS:
    h = [p for p in PREG if resp.get(p, ("", ""))[0].startswith("HALLAZGO")]
    if h:
        print(f"    {pieza:<20} {', '.join(h)}")
