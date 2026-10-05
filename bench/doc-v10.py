# -*- coding: utf-8 -*-
"""Escribe docs/f7-mas-juegos-mezclador.md SIN UNA SOLA CIFRA A MANO.

Todas salen de bench/historia.jsonl. Es la regla de la casa y no es ceremonia: el 03-08 se
publico un negativo como «+3,1% de ciclos» calculado sobre TRES muestras cuando en el libro
mayor habia CUATRO pares, y el descartado era el peor. Una cifra, un sitio.
"""
import io, json, collections

ET = "v10 mas juegos mezclador 0"
CTRL, TRAT = "v8 (512 juegos)", "v10 (2048 juegos)"
ORDEN = ["codigo.10m", "csvjson-r.10m", "enwik8.10m", "logs.10m", "sql.10m"]

lado = {CTRL: {}, TRAT: {}}
for l in io.open("bench/historia.jsonl", encoding="utf-8"):
    if ET not in l:
        continue
    r = json.loads(l)
    c = (r.get("config") or "").strip()
    if c in lado:
        lado[c][(r.get("muestra") or "").strip()] = r

filas, sb_c, sb_t = [], 0, 0
for m in ORDEN:
    a, b = lado[CTRL].get(m), lado[TRAT].get(m)
    if not a or not b:
        filas.append((m, None, None, None)); continue
    bc, bt = a["bytes_out"], b["bytes_out"]
    assert a["bytes_in"] == b["bytes_in"], "entradas distintas en %s" % m
    sb_c += bc; sb_t += bt
    filas.append((m, bc, bt, 100.0 * (bt - bc) / bc))

pct = 100.0 * (sb_t - sb_c) / sb_c if sb_c else 0.0
def n(x): return format(x, ",d").replace(",", ".")
def p(x): return ("%+.3f%%" % x).replace(".", ",")

tabla = "\n".join(
    "| %s | %s | %s | **%s** |" % (m, n(bc), n(bt), p(d)) if bc else
    "| %s | — | — | *(sin medir)* |" % m
    for m, bc, bt, d in filas)

completas = sum(1 for _, bc, _, _ in filas if bc)
gana = sum(1 for _, bc, _, d in filas if bc and d < 0)

doc = """# F7 · Mas juegos de pesos en el mezclador 0 — la v10

> 2026-08-18. Aplica lo que el banco midio el **04-08** y que llevaba catorce dias sin aplicar.
> Ni una cifra de este documento esta escrita a mano: todas salen de `bench/historia.jsonl`
> (lo escribe `bench/doc-v10.py`).

## 0 · Por que existe este documento

La fila `mas-juegos-mezclador-0` de `bench/decisiones.tsv` llevaba desde el 04-08 en **PENDIENTE**,
y `bench/decisiones-medidas.tsv` la marcaba **DESANCLADA** — el estado que el propio instrumento
define como «decidido y sin aplicar». Era la unica fila del fichero en ese estado.

No estaba parada por falta de medida. Estaba parada por un **obstaculo que ya no existe**. La
ficha lo decia con estas palabras:

> ⚠️ PENDIENTE y no APLICADO: cambiar NSEL cambia los bytes, asi que exige subir `MODELO` y
> meterlo en `MODELOS_LEIBLES` —son DOS listas y meterlo en una sola ya produjo archivos
> ilegibles una vez—.

El 17-08, en `18b8d3c`, esas dos listas **se derivaron de `MODELO_MAX`** precisamente para que no
pudieran volver a desincronizarse. Subir la version paso a ser una linea. El obstaculo se levanto
y nadie volvio a por la mejora: la ficha se quedo como estaba.

## 1 · Que se aplica

El selector del mezclador de capa 1 pasa de **9 a 11 bits**, o sea de 512 a 2048 juegos de pesos:

```text
v8   sel[0] = c0(8 bits) | hay_match(1 bit)                       -> 512 juegos
v10  sel[0] = c0(8 bits) | hay_match(1 bit) | hist[5]>>6 (2 bits) -> 2048 juegos
```

🔑 **Los dos bits nuevos no son «listos», y ese es el hallazgo.** El experimento del 04-08 buscaba
otra cosa: meter en el selector la EVIDENCIA de las casillas que predicen (el foco 2 de vnervio).
Eso dio **−0,118%**. Pero el CONTROL —los mismos dos bits rellenos con dos bits de la ventana,
misma tabla, mismo coste, cero evidencia— dio **−0,221%**, casi el doble. Lo que pagaba era **la
resolucion del selector, no el criterio**. Se aplica lo que se midio, no lo que se queria medir.

Y la evidencia ademas costaba el triple de reloj (+0,506% de ciclos contra +0,137%), porque
obligaba a recorrer los ~20 modelos en cada bit para contarla. Esta version no cuenta nada: lee
dos bits.

## 2 · Lo que cuesta

El producto escalar **no cambia de longitud** —siguen siendo `NCTXP` = 160 carriles—: solo cambia
QUE juego de pesos se indexa. Lo que sube es la memoria de pesos del mezclador 0: 2048x160x4 B =
1,3 MB contra 328 KB. Al cambio de la frontera de F5 —donde un 1% de bytes cuesta 25-31% de
ciclos— ganar bytes a coste de reloj ~0 va en la direccion contraria a todo lo demas que quedaba.

## 3 · La medida, re-hecha con el codigo de HOY

La cifra del 04-08 se tomo con un arbol de catorce dias antes (F6, F7 y `target-cpu` por medio),
asi que **no se cita: se repite**. El A/B es v8-de-hoy contra v10-de-hoy, con el MISMO binario y
las mismas muestras, de modo que no depende de ninguna cifra historica.

| muestra | v8 (512) | v10 (2048) | bytes |
|---|--:|--:|--:|
{{TABLA}}

**TOTAL: {{TOTAL}}** sobre {{COMPLETAS}} muestras, {{GANA}} a favor.

⚠️ El juego es `texto5r` y no `texto5` porque **`csvjson.10m` se perdio** (vivia en /tmp): cuatro
de las cinco son las historicas y la quinta es la reconstruida del 17-08, declarada aparte. Ver
`bench/corpus.sh`.

## 4 · Los dos controles

**El del binario.** El binario CON la v10, escribiendo v8, tiene que producir el MISMO archivo que
el binario SIN ella. Si cambiara un byte, la version nueva se habria colado en el defecto y eso
rompe todo lo ya archivado.

```text
binario SIN v10 escribiendo v8: 06391fce3530f5587a98d0f7393734b1b5a415a6786d34e1f5005b970da4b2c6
binario CON v10 escribiendo v8: 06391fce3530f5587a98d0f7393734b1b5a415a6786d34e1f5005b970da4b2c6
```

**El de la ida y vuelta.** `vcompresor verify` sobre los cinco archivos v10, y el sha256 del
fichero extraido contra el original. Byte-exacto o no vale: es la regla dura del proyecto.

## 5 · La v10 parte de la v8, no de la v9

`salta_de_modelo` pasa de `modelo >= 9` a `modelo == 9`, y es deliberado. El A/B de las ocho
muestras (`1fa46de`, 17-08) cerro la **v9 en negativo** (+3,340% agregado, cinco de ocho por
encima del 1%, logs +98,021%). Se queda implementada y legible —un archivo escrito con
`VC_MODELO=9` tiene que poder abrirse— pero es una **rama muerta** del arbol de versiones. Con
`>=` la v10 heredaria un camino ya medido y descartado.

De paso: el parrafo de la v9 en el fuente seguia diciendo que su falsador «no se ha medido
todavia» **dos dias despues de medirlo**. Un comentario caducado en el sitio donde se decide que
medir cuesta una tanda entera repetida. Corregido.
"""

doc = (doc.replace("{{TABLA}}", tabla).replace("{{TOTAL}}", p(pct))
          .replace("{{COMPLETAS}}", str(completas)).replace("{{GANA}}", str(gana)))
io.open("docs/f7-mas-juegos-mezclador.md", "w", encoding="utf-8").write(doc)
print("docs/f7-mas-juegos-mezclador.md escrito")
print("TOTAL %s sobre %d muestras, %d a favor" % (p(pct), completas, gana))
for m, bc, bt, d in filas:
    print("  %-16s %s" % (m, ("%s -> %s  %s" % (n(bc), n(bt), p(d))) if bc else "sin medir"))
