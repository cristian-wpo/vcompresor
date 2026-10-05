#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""TODOS LOS BUCLES DEL CAMINO DE COMPRESION, EN ORDEN, Y CUAL ES EL PRIMERO QUE CUESTA DE MAS.

Encargo del dueño, y su correccion: «saber cual es el primer fichero que empieza a ponerse lento
es una lectura superficial. Mi punto es en que algoritmo o en que procesamiento empieza la
lentitud, cual es el primer for o while que empieza a ponerlo lento».

Asi que esto no son fases ni ficheros: son LOS BUCLES, en el orden en que se ejecutan, con lo
que recorre cada uno, lo que hace en cada vuelta y su complejidad. La pregunta se contesta
leyendo la columna «veredicto» de arriba abajo y parando en el primer «CUESTA DE MAS».

COMO SE LEE «n»:
    F = numero de ficheros        (43.298 en el arbol de verdad)
    B = bytes totales             (2,62 GB)
    T = trozos tras el troceado
    P = patrones de --exclude

LA REGLA QUE SE APLICA: un bucle no es malo por ser O(F). Es malo cuando hace en cada vuelta
algo que no hace falta, o cuando su coste pasa de O(F) a O(B) sin necesidad. Los tres primeros
bucles del camino pagan una llamada al sistema por fichero y eso es el suelo. El cuarto ya no.
"""
import csv

# orden · bucle · fichero:linea · recorre · que hace en cada vuelta · complejidad · veredicto · nota
B = [
(1, "spawn del pool", "lib.rs ~1540", "hilos", "crear un hilo", "O(hilos)", "bien",
 "Diez hilos una vez. No entra en la cuenta."),

(2, "reconstruir el mapa de dedup", "lib.rs ~1580", "trozos del indice base",
 "insertar la huella de cada trozo ya existente en un HashMap", "O(T_base)", "bien",
 "Solo en `add`. Necesario: sin el mapa no hay deduplicacion contra lo que ya estaba."),

(3, "recorrer() — andar el arbol", "lib.rs 228", "F",
 "un readdir por directorio y un symlink_metadata por entrada", "O(F) · 1 stat", "SUELO",
 "Es el minimo inevitable: para saber que hay, hay que preguntarlo. STAT NUMERO 1."),

(4, "for (p, tipo) in entradas", "lib.rs ~1740", "F",
 "casa_patron contra cada --exclude, y inodo_de -> OTRO symlink_metadata", "O(F·P) · 1 stat", "CUESTA DE MAS",
 "STAT NUMERO 2 sobre el mismo fichero. recorrer() ya hizo symlink_metadata tres lineas antes y tiro el resultado: si devolviera el (dev, ino, nlink) que ya tenia, este stat desaparece. Es la primera perdida del camino, y es pequeña pero es gratis quitarla."),

(5, "suma de bytes para la barra de progreso", "lib.rs ~1876", "F",
 "std::fs::metadata de cada fichero, otra vez, solo para saber el total", "O(F) · 1 stat", "CUESTA DE MAS — Y ES MIA, DE HOY",
 "STAT NUMERO 3. Lo meti esta manana para que el porcentaje fuera por bytes en vez de por ficheros, y lo justifique diciendo «los stat vienen calientes del recorrido, asi que cuesta milisegundos». NUNCA LO MEDI. El tamaño ya lo sabian los dos bucles anteriores; esta pasada entera sobra."),

(6, "firmas de similitud (paralelo)", "lib.rs 1061", "F",
 "File::open + vec![0u8; 1 MiB] + leer hasta 1 MiB + calcular 4 minimos + TIRAR LOS BYTES",
 "O(B) · 1 open + 1 mmap", "AQUI EMPIEZA LA LENTITUD DE VERDAD",
 "EL PRIMER BUCLE CUYO COSTE ES O(B) Y NO O(F). Lee EL ARBOL ENTERO —los ficheros del corpus real miden 60 KB de media, o sea que el tope de 1 MiB no recorta nada— calcula cuatro numeros y descarta lo leido. Veinte lineas mas abajo el mismo fichero se vuelve a leer entero. Y ademas: vec![0u8; 1 MiB] pasa del umbral de mmap de glibc (128 KB), asi que son 43.298 pares de mmap/munmap para guardar 60 KB. Todo esto ANTES de comprimir un solo byte."),

(7, "union-find de las firmas", "lib.rs 1078", "F·4",
 "buscar cuatro huellas en un HashMap y unir grupos", "O(F·α)", "bien",
 "Casi lineal. La parte algoritmica de agrupar esta bien resuelta; lo caro es la lectura de arriba."),

(8, "orden.sort_by_key", "lib.rs 1096", "F", "comparar dos claves", "O(F log F)", "bien",
 "43.298 log 43.298 ≈ 690.000 comparaciones. Nada al lado de leer 2,6 GB."),

(9, "for lote in rutas.chunks(64)", "lib.rs ~1885", "F/64",
 "pretranscodificar los JPEG del lote, en paralelo", "O(B_jpeg)", "a MEDIR",
 "El corpus tiene 12.569 JPEG de 43.298 ficheros. La ablacion --no-jpeg dice cuanto cuesta; esta encolada."),

(10, "for (i, ruta) in lote — el bucle principal", "lib.rs ~1887", "F",
 "metadata (stat 4), metadata (stat 5), firma_de (open + 8 B), y a veces lee_prefijo 256 KB o lee_ventana 512 KB (open)",
 "O(F) · 2 stat + 2 open", "CUESTA DE MAS",
 "STATS 4 Y 5 sobre el mismo fichero, y dos aperturas mas para decidir el carril. En total, contando desde el principio: CINCO stat y hasta CINCO open por fichero, y el contenido leido entero DOS veces como minimo."),

(11, "transformadas: preflate / log-ts / E8E9", "lib.rs ~1830", "F candidatos",
 "std::fs::read del fichero ENTERO otra vez, y el trabajo de la transformada", "O(B)", "a MEDIR",
 "Cada una vuelve a leer el fichero completo aunque los bucles 6 y 10 ya lo hayan tocado. VC_SIN_PREFLATE / VC_SIN_LOGTS / VC_SIN_E8E9 lo miden; encolado."),

(12, "while flujo.siguiente() — el troceado", "lib.rs ~1900", "B",
 "avanzar la ventana rodante, cortar el trozo, blake3, buscar en el mapa, copiar al carril",
 "O(B)", "SUELO",
 "Este SI tiene que recorrer todos los bytes: es el trabajo. Aqui es donde deberia irse el tiempo."),

(13, "for df in rx_done — reordenar tramos", "lib.rs ~1560", "tramos",
 "sacar del mapa el siguiente tramo por orden y escribirlo", "O(tramos)", "bien",
 "El escritor va en su hilo. No bloquea al principal salvo por el canal."),
]

with open("bench/bucles.tsv", "w", newline="") as f:
    f.write("# LOS BUCLES DEL CAMINO DE COMPRESION, EN ORDEN, y cual es el primero que cuesta de mas.\n")
    f.write("# La pregunta era «en que algoritmo empieza la lentitud, cual es el primer for o while».\n")
    f.write("# Se lee la columna «veredicto» de arriba abajo y se para en el primer «CUESTA DE MAS».\n")
    f.write("# n:  F = ficheros (43.298)  ·  B = bytes (2,62 GB)  ·  T = trozos  ·  P = patrones de --exclude\n")
    f.write("# Lo escribe bench/bucles.py.\n")
    w = csv.writer(f, delimiter="\t")
    w.writerow(["n", "bucle", "donde", "recorre", "que_hace_cada_vuelta", "complejidad",
                "veredicto", "nota"])
    for r in B:
        w.writerow(r)

print(f"bench/bucles.tsv: {len(B)} bucles del camino\n")
for n, nom, donde, rec, _, cx, ver, _ in B:
    marca = {"bien": "   ", "SUELO": " = ", }.get(ver, " ! ")
    print(f"{marca}{n:>2}. {nom:<40} {rec:>10}  {cx:<16} {ver}")
print()
print("EL PRIMERO QUE CUESTA DE MAS: el bucle 4 (un stat repetido).")
print("DONDE EMPIEZA LA LENTITUD DE VERDAD: el bucle 6 — el primero O(B) en vez de O(F).")
