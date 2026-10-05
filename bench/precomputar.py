#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""LA NORMA DE PRECOMPUTAR, y el censo de todo lo precomputable del proyecto.

Encargo del dueño: «aplica esas 3 reglas como norma general en el panel para precomputar todo
lo precomputable».

Aplicarlas es dos cosas: escribirlas, y pasar por ellas cada candidato que haya. Aqui estan las
dos, y ademas una cuarta pregunta que las tres no contestan y sin la cual la norma engaña.

── LAS TRES REGLAS ─────────────────────────────────────────────────────────────────────────
Cambiar un calculo por una consulta gana SOLO si se cumplen las tres:

  1. EL DOMINIO ES PEQUEÑO       la tabla tiene que caber, y «caber» significa en cache
  2. LAS ENTRADAS SE REPITEN     si cada entrada es nueva, no hay nada que amortizar
  3. EL CALCULO CUESTA MAS       mas que un acceso a cache, que es con lo que se compara

La tercera es la que decide y la que se olvida. Un acceso que falla en cache cuesta ~200 ciclos;
una multiplicacion, 3. Por eso los procesadores llevan multiplicador en hardware y no una tabla
de multiplicar: la tabla seria SESENTA VECES mas lenta.

── LA CUARTA PREGUNTA, que las tres no contestan ───────────────────────────────────────────
  4. ¿CUANTO MUEVE?

Las tres reglas dicen si precomputar FUNCIONA. No dicen si MERECE LA PENA. El censo de aqui
abajo tiene un caso que lo enseña: lane_for hace hasta 53 comparaciones de cadena y reserva un
String por cada uno de los 43.298 ficheros. Pasa las tres holgadamente. Y la fase que lo
contiene son 0,34 s de 377, o sea el 0,1%: arreglarlo no se notaria.

Sin la cuarta pregunta, esta norma se convierte en una excusa para tocar codigo que no molesta.

── EL LIMITE, medido en casa ───────────────────────────────────────────────────────────────
vc-cm esta construido ENTERO sobre tablas precomputadas y su cuello es justo eso: el 83,7% de su
tiempo son busquedas en tablas de 484 MiB que NO CABEN EN CACHE. Es la regla 1 rota a lo grande,
y el resultado no es que se pierda la ganancia: es que la tabla se convierte en el problema.
Por eso la tabla nueva del decodificador es de 9 bits y no de 15.
"""
import csv

# nombre · donde · r1 dominio · r2 repite · r3 coste · r4 cuanto mueve · estado · nota
C = [
("tabla GEAR del troceado", "vc-core/src/cdc.rs:15",
 "SI · 256 valores de 64 bits = 2 KiB", "SI · una vez por byte troceado",
 "SI · un xorshift de tres pasos contra un acceso a L1",
 "no se midio por separado; es const fn, asi que su coste en ejecucion es CERO",
 "HECHO",
 "La forma mas pura: `const fn`, se calcula AL COMPILAR. No hay ni inicializacion perezosa."),

("squash / stretch", "vc-cm/src/lib.rs:97",
 "SI · 4.096 entradas cada una", "SI · varias veces por bit codificado",
 "SI · evita coma flotante en el camino caliente",
 "sin medir por separado",
 "HECHO",
 "El propio comentario lo dice: «sin float en runtime del codec». Es la razon correcta."),

("arboles de Huffman fijos", "vc-core/src/inflate.rs:229",
 "SI · dos arboles de 288 y 30 simbolos", "SI · una vez por flujo, 1.973 flujos",
 "SI · tres recorridos de 288 simbolos contra un puntero",
 "parte del +39% del bucle de copia + este; no se aislo",
 "HECHO 2026-07-30",
 "Se construian UNA VEZ POR FLUJO a partir de arrays constantes: 3.946 construcciones "
 "identicas sobre el banco. Lo señalo el dueño."),

("tabla de 9 bits del decodificador", "vc-core/src/inflate.rs:lee",
 "SI · 512 entradas de 4 B = 2 KiB por arbol, cabe en L1",
 "SI · 1.983.267.030 pasos en Huff::lee, 7,70 por simbolo",
 "SI · ~8 pasos con lectura de bit y comprobacion contra 1 acceso a L1",
 "MEDIDO: 279,0 -> 171,8 ciclos por simbolo (-38%), 354 -> 416,7 MB/s",
 "HECHO 2026-07-30",
 "9 bits y no 15 A PROPOSITO: con 15 serian 32.768 entradas por arbol y cada bloque dinamico "
 "la ensuciaria entera — que es el fallo de vc-cm."),

("tablas de modelos de vc-cm", "vc-cm/src/lib.rs",
 "NO · 484 MiB. No caben en ninguna cache", "SI",
 "NO · el acceso ES el coste",
 "MEDIDO al reves (perf record): 83,7% en predecir+actualizar, y 14,3% es explicitamente buscar y escribir el cubo. «No cabe en cache» esta inferido del tamaño, no medido",
 "EL LIMITE",
 "La regla 1 rota a lo grande. No es que se pierda la ganancia: la tabla se convierte en el "
 "problema. Es el contraejemplo que hay que tener delante antes de precomputar nada."),

("lane_for: extension -> carril", "vc-core/src/lib.rs:lane_for",
 "SI · 53 extensiones entre las dos listas", "SI · una vez por fichero, 43.298",
 "SI · hasta 53 comparaciones de cadena + un String en minusculas, contra un hash",
 "MEDIDO: la fase «enrutar» son 0,34 s de 377, el 0,1%. NO MERECE LA PENA.",
 "PASA LAS TRES Y NO SE HACE",
 "El caso que justifica la cuarta pregunta. Cumple las tres reglas holgadamente y arreglarlo "
 "no se notaria. Queda escrito para que nadie lo 'descubra' otra vez."),

("codigos canonicos del emisor", "vc-core/src/deflate_emit.rs:89",
 "SI · un vector por arbol", "SIN MEDIR · se recalculan 3 veces por bloque comprimido, y hay "
 "11.840 bloques; falta saber cuantos comparten arbol",
 "SI · un recorrido de 288 simbolos contra un puntero",
 "SIN MEDIR",
 "SIN MEDIR",
 "Candidato real y sin cerrar. La pregunta que lo decide: ¿cuantos bloques repiten arbol? Si "
 "casi ninguno, no hay nada que cachear."),
]

with open("bench/precomputar.tsv", "w", newline="") as f:
    f.write("# LA NORMA DE PRECOMPUTAR y el censo de lo precomputable.\n")
    f.write("# Cambiar un calculo por una consulta gana SOLO si se cumplen las TRES:\n")
    f.write("#   1. el dominio es pequeño (cabe en cache) · 2. las entradas se repiten\n")
    f.write("#   3. el calculo cuesta MAS que un acceso a cache  <- la que decide\n")
    f.write("# Y una CUARTA que las tres no contestan: ¿cuanto mueve? Sin ella la norma se\n")
    f.write("# convierte en una excusa para tocar codigo que no molesta.\n")
    f.write("# Lo escribe bench/precomputar.py.\n")
    w = csv.writer(f, delimiter="\t")
    w.writerow(["candidato", "donde", "r1_dominio_pequeño", "r2_se_repite",
                "r3_cuesta_mas_que_un_acceso", "r4_cuanto_mueve", "estado", "nota"])
    w.writerows(C)

import collections
c = collections.Counter(r[6].split()[0] for r in C)
print(f"bench/precomputar.tsv: {len(C)} candidatos\n")
print("  LAS TRES REGLAS")
print("    1. el dominio es pequeño        la tabla tiene que caber EN CACHE")
print("    2. las entradas se repiten      si cada una es nueva, no hay que amortizar")
print("    3. el calculo cuesta MAS        que un acceso a cache  <- la que decide")
print("    4. ¿cuanto mueve?               las tres dicen si FUNCIONA, no si MERECE LA PENA")
print()
for r in C:
    print(f"  [{r[6]:<24}] {r[0]}")
print()
print("  " + " · ".join(f"{k}: {v}" for k, v in c.most_common()))
