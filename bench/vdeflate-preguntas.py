#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""LAS 16 PREGUNTAS DEL CAMINO, APLICADAS A VDEFLATE.

Encargo del dueño: «a vdeflate aplícale todas las preguntas, la aguja propia y la pirámide
propia».

Se le hacen a las cinco piezas del proyecto: el inflate, el emisor, los arneses, el banco y las
tablas. Y como en la auditoria del archivador, las celdas SIN MIRAR se enseñan: una auditoria
que solo enseña lo que encontro parece completa y no lo esta.

LA PRIMERA QUE SE CONTESTA SOLA ES INCOMODA. La P10 —«¿el numero viaja con su contexto o se
copio suelto?»— la falla la tabla que acabo de escribir para esta misma pestaña: bench/
vdeflate.py tiene el 78%, el 24,7x, el 1,70% y los 279 ciclos ESCRITOS A MANO, copiados de las
medidas. Es exactamente el fallo de veredictos.py que se arreglo esta mañana, cometido por la
tarde en la tabla que lo cuenta.
"""
import csv

PREG = {}
with open("bench/preguntas-camino.tsv") as f:
    fil = [l.rstrip("\n").split("\t") for l in f if not l.startswith("#")]
cab = fil[0]
for r in fil[1:]:
    if len(r) == len(cab):
        d = dict(zip(cab, r))
        PREG[d["id"]] = d["la_pregunta"]

NA = "NO APLICA — trabaja sobre un buffer que le dan; no toca el nucleo ni mide nada."
NA_INS = "NO APLICA — no es un instrumento de medida."

PIEZAS = [
("inflate.rs", "crates/vc-core/src/inflate.rs", 470, "Fase 1 y 1b: descomprime y captura el flujo.", {
  "P1": ("LIMPIO", "Ni una llamada al sistema: recibe un slice y devuelve un Vec."),
  "P2": ("HALLAZGO", "Bits::bit() toca el MISMO byte hasta ocho veces, con comprobacion de "
         "limites en cada una. Medido: 621 instrucciones y 279 ciclos por simbolo, cuando un "
         "decodificador con tabla hace 10-20 ciclos. Es el 96% del margen que nos separa del "
         "suelo fisico de 0,95 s."),
  "P3": ("HALLAZGO", "Vec::with_capacity(d.len() * 4) es una conjetura de 4x. MEDIDO sobre el "
         "banco: 357,5 MiB de DEFLATE dan 5.791,3 MiB de plano, o sea 16,2x. La reserva se queda "
         "CUATRO VECES corta y el Vec tiene que crecer y copiar varias veces un buffer de GB. "
         "Y el tamaño exacto se conoce: Hueco.len_plano lo guarda y no se le pasa."),
  "P4": ("ARREGLADO 2026-07-30", "Los tokens se calculaban y se tiraban. Ya se devuelven, con "
         "prueba de que reconstruyen el plano y medido que no cuesta nada (-0,7%, bajo el ruido)."),
  "P5": ("LIMPIO", "Una sola pasada por el flujo."),
  "P6": ("HALLAZGO", "La unica perilla es ese factor 4 de la reserva, y nunca se ha barrido. Con "
         "el dato de 16,2x medido, ponerlo bien es una linea."),
  "P7": ("LIMPIO", "La cabecera dice que el metodo bit a bit es «el mas lento y el mas facil de "
         "demostrar correcto». Era una decision honesta Y AHORA ESTA MEDIDA: 24,7 veces por "
         "encima del suelo fisico de esta maquina. Pocas justificaciones del proyecto tienen su "
         "numero al lado."),
  "P8": ("LIMPIO", "Huff::nuevo calcula los conteos canonicos una vez por arbol."),
  "P9": ("LIMPIO", "Sin condicionales de plataforma ni dependencias."),
  "P13": ("HALLAZGO", "Sus vectores de prueba van tecleados a mano en hexadecimal. UNO ESTABA "
          "MAL —4b 4c 84 00 00, que zlib lee como ocho aes cuando el test esperaba seis— y dejo "
          "el test en rojo culpando a nuestro codigo, que acertaba. Ahora el fichero dice como "
          "regenerarlos, pero siguen siendo tecleados."),
  "P10": ("NO APLICA", NA_INS), "P11": ("NO APLICA", NA_INS), "P12": ("NO APLICA", NA_INS),
  "P14": ("NO APLICA", NA_INS), "P15": ("NO APLICA", NA_INS), "P16": ("NO APLICA", NA_INS)}),

("deflate_emit.rs", "crates/vc-core/src/deflate_emit.rs", 330, "Fase 2: vuelve a emitir byte a byte.", {
  "P1": ("LIMPIO", "Solo memoria."),
  "P2": ("HALLAZGO", "Escritor::codigo() escribe el codigo Huffman BIT A BIT, llamando a bits(1) "
         "una vez por bit. Es el espejo del mismo problema del lector, y tiene el mismo arreglo: "
         "un acumulador que escriba varios bits de golpe."),
  "P3": ("HALLAZGO", "Escritor::nuevo() arranca con Vec::new() y crece doblando, cuando el "
         "tamaño de salida se conoce con precision: es el del flujo original que se esta "
         "reproduciendo."),
  "P4": ("HALLAZGO", "codigos_canonicos() recalcula, a partir de las longitudes, exactamente los "
         "conteos que Huff::nuevo ya calculo al leer el mismo arbol. La informacion existe y se "
         "vuelve a derivar."),
  "P5": ("HALLAZGO", "Los codigos canonicos se recalculan POR BLOQUE. Un flujo del banco llega a "
         "143 bloques, y muchos comparten arbol: se recalcula lo mismo decenas de veces."),
  "P6": ("NO APLICA", "No tiene perillas: el formato lo fija el RFC 1951."),
  "P7": ("LIMPIO", "Lo que afirma —byte a byte sobre 1.973 flujos— esta medido por su arnes."),
  "P8": ("LIMPIO", "Duplica a proposito la logica de codigos canonicos del lector, y la razon "
         "esta escrita: «compartir el estado entre lector y escritor ahorraria treinta lineas y "
         "haria imposible probar uno contra otro». Es duplicacion con motivo."),
  "P9": ("LIMPIO", ""),
  "P13": ("ARREGLADO", "Sus tres vectores tambien van a mano, pero el cuarto —la longitud 258 "
          "via 284+31— se CONTRASTO CONTRA ZLIB desde fuera en vez de contra nuestro propio "
          "inflate, que es lo unico que demuestra algo."),
  "P10": ("NO APLICA", NA_INS), "P11": ("NO APLICA", NA_INS), "P12": ("NO APLICA", NA_INS),
  "P14": ("NO APLICA", NA_INS), "P15": ("NO APLICA", NA_INS), "P16": ("NO APLICA", NA_INS)}),

("los arneses", "bin/vc-inflate, vc-redeflate, vc-cobertura, vc-preflate-coste", 350,
 "Los cuatro programas que dicen si vdeflate funciona.", {
  "P1": ("LIMPIO", "Leen cada fichero del banco una vez."),
  "P2": ("HALLAZGO", "vc-redeflate y vc-cobertura INFLAN LOS MISMOS 1.973 FLUJOS por separado. "
         "Son dos pasadas completas sobre 357 MiB para dos preguntas que se contestan con una."),
  "P3": ("SIN MIRAR", ""),
  "P4": ("LIMPIO", "vc-cobertura usa los tokens que el inflate ya devuelve."),
  "P5": ("NO APLICA", ""), "P6": ("NO APLICA", ""),
  "P7": ("LIMPIO", "No afirman nada que no midan."),
  "P8": ("HALLAZGO", "Los cuatro repiten el mismo codigo de leer el banco y ordenar los casos."),
  "P9": ("LIMPIO", ""),
  "P10": ("LIMPIO", "Imprimen lo que acaban de medir, en la misma ejecucion."),
  "P11": ("HALLAZGO ARREGLADO", "La cabecera de vc-inflate decia «2.676 flujos» en tres sitios "
          "cuando son 1.973. Un numero en prosa que nadie recalcula."),
  "P12": ("HALLAZGO", "vc-cobertura IMPRIME su tabla y no la guarda: no hay TSV, asi que el "
          "panel no puede leerla y hay que volver a correrla para consultarla. Es el fallo que "
          "mide.sh tenia con el stderr, otra vez."),
  "P13": ("LIMPIO", "Los arneses no llevan vectores: usan flujos reales."),
  "P14": ("HALLAZGO", "No hay nada que corra los cuatro de una vez. Hoy son cuatro ordenes que "
          "hay que acordarse de teclear, y ya paso una vez que una suite verde tapara otra roja."),
  "P15": ("LIMPIO", "La cobertura CUENTA (medida directa) en vez de inferir; el suelo se mide "
          "en la maquina en vez de estimarse."),
  "P16": ("NO APLICA", "")}),

("el banco", "bench/saca-deflate.py + /tmp/deflate-banco", 60,
 "Los 1.973 flujos DEFLATE reales contra los que se valida todo.", {
  "P1": ("LIMPIO", ""), "P2": ("LIMPIO", ""), "P3": ("LIMPIO", ""), "P4": ("LIMPIO", ""),
  "P5": ("NO APLICA", ""),
  "P6": ("HALLAZGO", "El banco sale de los PNG de UN corpus. La cobertura dice que aun asi toca "
         "las 14 dimensiones del formato, lo cual es una suerte y no un diseño: nadie eligio "
         "esos flujos para cubrir el RFC. Un banco elegido a proposito seria mas pequeño y "
         "cubriria igual o mas."),
  "P7": ("LIMPIO", ""), "P8": ("LIMPIO", ""), "P9": ("LIMPIO", ""),
  "P10": ("NO APLICA", ""),
  "P11": ("HALLAZGO", "El banco vive en /tmp y se borra. Es regenerable —el guion esta "
          "commiteado— pero eso significa que dos medidas de dos dias distintos pueden estar "
          "hechas sobre ficheros distintos sin que nada avise. No hay huella del banco."),
  "P12": ("NO APLICA", ""), "P13": ("NO APLICA", ""), "P14": ("NO APLICA", ""),
  "P15": ("LIMPIO", "Son flujos REALES, que es la unica clase de entrada que sirve para decir "
          "«funciona»."),
  "P16": ("NO APLICA", "")}),

("las tablas", "bench/vdeflate.py + vdeflate.tsv", 180,
 "Lo que el panel enseña de vdeflate.", {
  "P1": ("NO APLICA", ""), "P2": ("NO APLICA", ""), "P3": ("NO APLICA", ""),
  "P4": ("NO APLICA", ""), "P5": ("NO APLICA", ""), "P6": ("NO APLICA", ""),
  "P7": ("LIMPIO", "Cada cifra que enseña tiene detras una medida de hoy."),
  "P8": ("NO APLICA", ""), "P9": ("NO APLICA", ""),
  "P10": ("HALLAZGO", "LA TABLA TIENE LOS NUMEROS COPIADOS A MANO. El 78%, el 24,7x, el 1,70%, "
          "los 279 ciclos: todos escritos dentro del guion, sin la carga de la maquina ni el "
          "commit ni la hora. Es EXACTAMENTE el fallo de veredictos.py que se arreglo esta "
          "mañana, cometido por la tarde en la tabla que lo cuenta. Se arregla igual: leyendo "
          "de bench/historia.jsonl y de la salida de los arneses."),
  "P11": ("HALLAZGO", "Si alguien mejora el decodificador, la tabla seguira diciendo 279 ciclos "
          "y 24,7x hasta que alguien la edite a mano. Una tabla de estado que no se recalcula "
          "deja de ser un estado y pasa a ser una foto vieja."),
  "P12": ("LIMPIO", "Escribe el TSV entero."),
  "P13": ("NO APLICA", ""), "P14": ("NO APLICA", ""),
  "P15": ("NO APLICA", ""), "P16": ("NO APLICA", "")}),
]

filas = []
for pieza, donde, lineas, que, resp in PIEZAS:
    for pid in sorted(PREG, key=lambda x: int(x[1:])):
        v, texto = resp.get(pid, ("SIN MIRAR", ""))
        filas.append([pieza, donde, lineas, que, pid, PREG[pid], v, texto])

with open("bench/vdeflate-preguntas.tsv", "w", newline="") as f:
    f.write("# LAS 16 PREGUNTAS DEL CAMINO, aplicadas a las cinco piezas de vdeflate.\n")
    f.write("# HALLAZGO = hay desperdicio, con evidencia · LIMPIO = se miro y no lo hay, y se\n")
    f.write("# dice por que · NO APLICA = la pregunta no tiene sentido aqui · SIN MIRAR = no se\n")
    f.write("# ha mirado, que NO es lo mismo que limpio.\n")
    f.write("# Lo escribe bench/vdeflate-preguntas.py.\n")
    w = csv.writer(f, delimiter="\t")
    w.writerow(["pieza", "donde", "lineas", "que_es", "pregunta_id", "la_pregunta",
                "veredicto", "la_respuesta"])
    w.writerows(filas)

import collections
c = collections.Counter(r[6].split()[0] for r in filas)
print(f"bench/vdeflate-preguntas.tsv: {len(PIEZAS)} piezas x {len(PREG)} preguntas = {len(filas)} celdas\n")
print("  " + " · ".join(f"{k} {v}" for k, v in c.most_common()))
print()
print("  %-18s %6s %9s %8s %9s %10s" % ("pieza", "lineas", "HALLAZGO", "LIMPIO", "SIN MIRAR", "NO APLICA"))
for pieza, _, lin, _, resp in PIEZAS:
    cc = collections.Counter(resp.get(p, ("SIN MIRAR", ""))[0].split()[0] for p in PREG)
    print("  %-18s %6d %9d %8d %9d %10d" %
          (pieza, lin, cc["HALLAZGO"], cc["LIMPIO"], cc["SIN"], cc["NO"]))
print()
print("  LOS HALLAZGOS:")
for pieza, _, _, _, resp in PIEZAS:
    h = [p for p in sorted(PREG, key=lambda x: int(x[1:]))
         if resp.get(p, ("", ""))[0] == "HALLAZGO"]
    if h:
        print(f"    {pieza:<18} {', '.join(h)}")
