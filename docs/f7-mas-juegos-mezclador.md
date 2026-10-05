# F7 · Mas juegos de pesos en el mezclador 0 — la v10

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
| codigo.10m | 787.471 | 784.646 | **-0,359%** |
| csvjson-r.10m | 411.690 | 411.059 | **-0,153%** |
| enwik8.10m | 2.088.907 | 2.085.516 | **-0,162%** |
| logs.10m | 48.822 | 48.687 | **-0,277%** |
| sql.10m | 226.103 | 225.573 | **-0,234%** |

**TOTAL: -0,211%** sobre 5 muestras, 5 a favor.

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
