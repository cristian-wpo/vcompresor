#!/usr/bin/env python3
"""F6.Q3 §6 — el precio de cebar, que es lo que decide aunque los bytes salgan bien.

Las condiciones (b) y (c) miden lo que el cebado COMPRA en bytes. No miden lo que CUESTA
montarlo en el producto, y esa es la mitad que decide: A4 no existe para comprimir mejor, existe
para usar los doce nucleos. Un metodo que devuelve bytes y se lleva por delante el paralelismo no
sirve para lo que lo queremos.

Tres numeros, que es justo lo que hace falta:

  1. EL MEMCPY. La forma barata de cebar no es re-modelar el frame anterior -eso es lo que mide
     (b)/(c), y cuesta mas que comprimirlo- sino guardar una instantanea del estado al acabar el
     frame N-1 y copiarla al predictor del frame N. Se cronometra esa copia.
  2. LA GUARDIA DE RAM. Con doce hilos hay doce predictores vivos, y cebar añade una instantanea
     por hilo. Se compara contra la RAM de la maquina.
  3. EL CAMINO CRITICO EN SERIE. Es el que manda: el frame N no puede empezar hasta que el N-1
     haya terminado, porque necesita su estado FINAL. Se pone el numero al lado del 4-5x que se
     buscaba.

Uso: bench/f6q3-precio-cebar.py [MiB_de_estado]
"""
import json, os, sys, time

AQUI = os.path.dirname(os.path.abspath(__file__))
NUCLEOS = 12          # 12 vCPU del servidor (6 fisicos + SMT), segun clients/vcompresor/target.json
RAM_GB = 62

# EL TAMAÑO DEL ESTADO, MEDIDO Y NO SUPUESTO. F5 dice «483 MiB de tablas» y esa cifra es de la
# configuracion por defecto: `tam` fija tmask/imask, asi que a bloques de 32 MiB las tablas son
# otras. Se saca del RSS que el propio libro mayor tiene anotado para las corridas de este frame,
# menos la entrada, que tambien esta residente.
def estado_medido():
    reg = os.path.join(AQUI, "historia.jsonl")
    mejor = 0
    with open(reg, encoding="utf-8") as fh:
        for linea in fh:
            f = json.loads(linea)
            if f.get("etiqueta", "").startswith("F6.Q3 s32") and f.get("rc") == 0 and f.get("rss_kb"):
                mejor = max(mejor, f["rss_kb"] / 1024 - f.get("bytes_in", 0) / 1048576)
    return mejor

MIB = float(sys.argv[1]) if len(sys.argv) > 1 else (estado_medido() or 483.0)
N = int(MIB * 1048576)

print(f"F6.Q3 §6 — EL PRECIO DE CEBAR")
print(f"  estado del predictor a solid=32: {MIB:.0f} MiB")
print(f"  (F5 anota 483 MiB de tablas en la configuracion por defecto; a bloques de 32 MiB")
print(f"   son mas, y lo que vale es lo que se mide en ESTAS corridas.)\n")

# 1. EL MEMCPY. Mejor de cinco: interesa el coste del cobre, no el de la vez que coincidio con
# otra cosa en la maquina. La copia es memcpy de C; el envoltorio de Python no entra en el reloj.
src = bytearray(N)
mejor = float("inf")
for _ in range(5):
    t0 = time.perf_counter()
    dst = bytes(src)
    t = time.perf_counter() - t0
    mejor = min(mejor, t)
    del dst
print(f"1. MEMCPY de {MIB:.0f} MiB (mejor de 5): {mejor*1000:.0f} ms  =  {MIB/1024/mejor:.1f} GiB/s")

# Y en la unidad en la que se compara todo aqui: ciclos. La frecuencia efectiva se saca de las
# propias filas (ciclos/segundo), no de /proc/cpuinfo, que da la nominal.
GHZ = 0
reg = os.path.join(AQUI, "historia.jsonl")
with open(reg, encoding="utf-8") as fh:
    for linea in fh:
        f = json.loads(linea)
        if f.get("etiqueta", "").startswith("F6.Q3 s32") and f.get("ciclos") and f.get("seg"):
            GHZ = max(GHZ, f["ciclos"] / f["seg"] / 1e9)
print(f"   a {GHZ:.2f} GHz efectivos = {mejor*GHZ:.1f} Gciclos por frame y por hilo")

# 2. LA GUARDIA DE RAM. Doce predictores vivos + una instantanea por hilo para cebar el siguiente.
vivo = NUCLEOS * MIB / 1024
con_ceba = vivo * 2
print(f"\n2. RAM con {NUCLEOS} hilos")
print(f"   hoy (un predictor por hilo):        {vivo:.1f} GiB")
print(f"   cebando (predictor + instantanea):  {con_ceba:.1f} GiB   de {RAM_GB} GB de la maquina")
veredicto = "CABE" if con_ceba < RAM_GB * 0.8 else "NO CABE con holgura"
print(f"   veredicto: {veredicto} ({100*con_ceba/RAM_GB:.0f}% de la RAM)")

# 3. EL CAMINO CRITICO. Lo que de verdad decide, y no hace falta medirlo: es una dependencia.
print(f"\n3. CAMINO CRITICO EN SERIE")
print(f"   Cebar el frame N desde el estado FINAL del frame N-1 encadena los frames: el N no")
print(f"   puede empezar hasta que el N-1 acabe. Con F frames y coste T cada uno:")
print(f"     hoy, en frio y en paralelo : max(F/{NUCLEOS},1) * T")
print(f"     cebado en serie            : F * T")
for F in (4, 12):
    frio = max(F / NUCLEOS, 1)
    print(f"     F={F:>2} frames -> frio {frio:.2f}*T   cebado {F}*T   = {F/frio:.1f}x MAS LENTO")
print(f"\n   El eje del paralelismo valia 1,76x y se buscaba 4-5x. Un cebado en cadena no lo sube:")
print(f"   lo baja a 1x por construccion, porque deja de haber paralelismo que medir.")
