# Nucleación → vcompresor · lo que transfiere, MEDIDO

> 2026-08-04. Fuente: `vnervio/src/vnervio.cpp` (997 líneas, leídas enteras) y `docs/01`-`06`.
> Destino: `vcompresor/crates/vc-cm/src/lib.rs`.
> ⚠️ **Ideas sí, código nunca** — la regla es del dueño (`vnervio/docs/02-cortafuegos.md`).
> Todas las cifras de abajo son de **bytes**, deterministas, `n=1`, medidas en el servidor sobre
> `texto5` (5 muestras de 10 MB). **Ninguna es de reloj.**

## 0 · La tesis, y por qué vcompresor es el sitio donde más duele

Nucleación (doc 03): *una representación del pasado que se aprende para CONSERVAR predicción en
vez de para descartar*. Y la corrección clave: **los nodos no se emiten, se convierten en el
CONTEXTO**. En vez de «¿qué sigue tras estos 8 bytes exactos?» se pregunta «¿qué sigue tras el
NODO de estos 8 bytes?», que se ha visto miles de veces.

La casilla de vcompresor es un **estado de 8 bits** (`struct Historia`, :92): una máquina que mete
una historia ilimitada en 256 casillas **está diseñada para descartar**. Es literalmente el
estrangulamiento que nucleación invierte. Y vcompresor **ya sabe que le duele** — la línea 2027
detecta el contexto famélico y le parte el voto por la mitad, con un comentario que admite que esa
probabilidad «el StateMap la ha heredado, no aprendido». Trata el síntoma con un parche de paq8px.

## 1 · LA PUERTA: el censo. Falsador escrito antes de mirar, y disparó

`docs/03` §«Lo que CAMINO exige ANTES de construir»: *la premisa no está medida; el censo la
contesta sin escribir una línea de nucleación. Si sale 0,1%, el proyecto muere barato y bien.*

Traspuesto aquí sobre el `nn0+nn1` que vcompresor ya calcula y tira. **Control: los bytes con el
censo salieron idénticos a los del binario limpio en las 5 muestras.**

| muestra | n=0 | **n≤2** | 3-8 | 9-32 | >32 | **desalojos** | …de ellos vacías |
|---|--:|--:|--:|--:|--:|--:|--:|
| logs | 0,29 | **0,53** | 20,97 | 31,60 | 46,89 | **0,38** | 99,99 |
| sql | 1,12 | **1,99** | 12,98 | 50,50 | 34,53 | **1,26** | 98,87 |
| csvjson | 3,84 | **6,99** | 22,19 | 45,31 | 25,51 | **3,96** | 86,77 |
| codigo | 6,07 | **9,21** | 18,03 | 44,54 | 28,22 | **6,24** | 73,91 |
| **enwik8** | 7,93 | **11,06** | 28,73 | 50,77 | **9,45** | **10,04** | **52,57** |

**Falsador:** <10% famélicas ⇒ nucleación muere barata · >30% ⇒ la mayor palanca sin tocar.
**Sale 0,53–11,06%.** La premisa **no se sostiene** en el corpus de vcompresor.

🔑 **Y enwik8 es el peor en las dos columnas a la vez**: el más famélico, el que menos casillas
maduras tiene (9,45% contra 25-47%), el que más desaloja, y el único donde casi la mitad de los
desalojos echan a una casilla **viva** (52,57% eran vacías, contra 99% en logs).
⇒ **Nucleación paga donde vcompresor es débil (texto, donde pierde 13% contra zpaq) y no paga
donde es fuerte.** Es la misma línea que traza `docs/02-cortafuegos.md`, ahora con números.

⚠️ Y la dirección del sesgo juega a favor del veredicto: a 10 MB la escasez está en su MÁXIMO
(pocas observaciones por casilla). Con más datos bajaría. El falsador aguanta *a fortiori*.

## 2 · Inventario VERIFICADO línea a línea (no supuesto)

| pieza de vnervio | vcompresor | dónde |
|---|---|---|
| repulsión: etiqueta + sondeo de 4 en una línea + desalojo | ✅ está | `busca_cubo` :1443 |
| desalojo por evidencia de 16 bits (`visitas`) | ⚠️ **satura**: usa `h.n0[e]+h.n1[e]` del nodo raíz | :1450 |
| casilla vacía preferida (`libre`) | ≈ equivalente (la virgen da info 0 y gana el mínimo) | :1452 |
| **vista gruesa del grupo** (`grueso()`) | ❌ **no existe** | — |
| **evidencia en el selector** (`cubo`) | ❌ no | `sel[]` :2261-2264 |
| **`res` del imán en el selector** | ❌ no | idem |
| segunda capa, cada mezclador con SU error | ✅ está | `actualiza_mezcla` :2352 |
| selector de la 2ª capa con `cuanto_se()` (m29) | ⚠️ el suyo es `bpos*2+hay_match`, 16 juegos | :2265 |
| **juegos dinámicos por identidad** (`busca_juego`) | ❌ no: `sel[k]*NCTXP`, índice puro | :2302 |
| **aspirantes · prestar · segunda oportunidad** (m32-39) | ❌ no | — |
| **heredar pesos al desalojar** | ❌ no | — |
| **`cuanto_se()`** (error reciente con olvido) | ❌ no | — |
| **bucle legal** (repasar el prefijo ya visto) | ❌ no | — |
| tablas por modelo (m27/28) | ✅ está, y más fino | `TAB_BITS`/`TAB_TOPE` :761 |
| cadena SSE calibrada | ✅ **lo tiene vcompresor y no vnervio** | :2320-2332 |

**Nueve piezas de vnervio no están aquí. Tres de vcompresor no están allí.**

Y `DUPLICADOS` **no borra los órdenes: los promueve.** `DUPLICADOS = {2,3,4,5,7,14}` e
`ISSE_CTX = {2,3,4,5,6,7,14}` son el mismo conjunto (más `palabra`): la tabla directa se apaga
porque la cadena ISSE ya refina ese contexto. Es «promover en vez de fusionar» hecho aquí sin
llamarlo así.

## 3 · Lo medido hoy

### A · La evidencia en el selector — **FALSADA**, y el experimento encontró otra cosa

El hueco existe y está verificado: los cuatro selectores llevan `c0`, `b1`, `bpos`, `b2` y `lb`
—que es la confianza del *match model*—. **Ninguno lleva un bit de evidencia de las casillas que
están prediciendo.** Y el número ya está en un registro (`nn0+nn1`, :2026).

Control interno: con la perilla apagada, bytes **idénticos** al binario limpio en las 5.

| brazo (todos con la misma tabla y el mismo coste) | texto5 |
|---|--:|
| 2 bits de EVIDENCIA en sel[0] (512→2048) | −0,118% |
| **CONTROL M9: 2 bits ARBITRARIOS** (`hist[5]>>6`) | **−0,221%** |
| 3 bits de evidencia (512→4096) | −0,211% |

🚨 **El control tonto gana al criterio listo por casi el doble.** Lo que pagaba era **la resolución
del selector**, no la evidencia. Es la misma forma que la atracción falsada de vnervio: elegir con
un criterio «inteligente» pierde contra no elegir.

Y medido en serie con ciclos, el negativo es peor todavía — **la evidencia cuesta el triple y
compra la mitad**, porque hay que recorrer los ~20 modelos en cada bit para contarla:

| | bytes | ciclos |
|---|--:|--:|
| más juegos (2 bits arbitrarios) | **−0,221%** | **+0,137%** |
| por evidencia | −0,118% | **+0,506%** |

⚠️ **Limitación propia, para que viaje con la cifra**: los cortes (5/10/15 sobre «cuántos de ~20
modelos tienen ≥4 observaciones») caen donde el censo de arriba dice que **no hay masa** — ese
recuento vive casi siempre por encima de 15, así que los dos bits eran casi constantes. **Lo
falsado es esta codificación, no la evidencia en general.** El listón para cualquier otra es
−0,221%, y con 3 bits tampoco se alcanzó.

**Lo que sí transfiere es el foco 2 tal cual: más juegos de pesos pagan.** Re-medido en SERIE por
`bench/mide.sh`, con ciclos y `bin_sha`:

| | bytes | ciclos |
|---|--:|--:|
| codigo | **−0,359%** | +0,138% |
| csvjson | −0,233% | −0,490% |
| enwik8 | −0,162% | +0,486% |
| logs | −0,278% | +0,626% |
| sql | −0,235% | −0,094% |
| **TOTAL** | **−0,221%** | **+0,137%** |

Los ciclos alternan de signo: ruido, como debía ser — **el producto escalar no cambia de longitud**
(siguen siendo `NCTXP=160` carriles), sólo cambia qué juego se indexa. Cuesta 1,3 MB de pesos
contra 328 KB. 🔑 **Al cambio de la frontera F5 —donde 1% de bytes CUESTA 25-31% de ciclos— esto
va en la dirección contraria a todo lo que quedaba.** Y desmiente el aviso del propio fuente
(«512 juegos no convergen dentro de un frame»): 2048 converge y gana; 4096 ya empata con 2048.

### C · ¿Merece la pena un `orden-16`? — **NO al cambio de F5**

vnervio midió hoy −0,802% al añadir los órdenes 12 y 16 a {1,2,3,4,6,8}. La escalera de vcompresor
es 0,1,2,3,4,5,8,12: tiene el 12, no el 16. Medido truncando la cadena ISSE (cuyo orden es
2→3→4→5→palabra→8→12, así que quitar la última etapa quita exactamente el orden-12):

| | sin orden-12 | sin orden-8 tampoco |
|---|--:|--:|
| TOTAL | **+0,285%** | +0,875% |
| sql | **+1,420%** | +2,599% |

**El orden-8 aporta 2,07× lo que aporta el orden-12**: la escalera se apaga hacia arriba con razón
geométrica. Extrapolando, el orden-16 daría ~0,14% y costaría `NCTXP` 160→168 — un bloque SIMD más
**en cada bit**. Al cambio medido en F5 eso es pagar reloj por ratio: dirección equivocada.
⚠️ Es a 10 MB y los órdenes altos necesitan datos; y **sql se sale de la fila** (el orden-12 vale
ahí cinco veces más que en el resto). Si algún día se mide a 100 MB, esta fila se revisa.

### B · Las salidas intermedias del ISSE — **YA ESTABA HECHO**

El fuente lo marca como «el único negativo caducado» y deja la perilla `VC_ISSE_INT`. **Su defecto
ya es 1 (encendido)** y hay **12 filas en `historia.jsonl` del 02-08**. Alguien ya repitió el
negativo caducado y ganó. ⚠️ **Pero no tiene fila en `decisiones.tsv`**: está medido y sin
registrar como decisión.

## 4 · Lo que NO cabe, y por qué se para aquí

**El nodo con cuentas.** El cubo son 16 bytes (1 firma + 15 nodos del árbol del nibble) y `n0`/`n1`
sólo existen como consulta aproximada y **saturada** del estado. **Dos estados no se pueden sumar**,
y la restricción dura nº1 de nucleación es «sólo fusionar; las cuentas se suman». Tenerlo son 4
bytes por nodo: de 483 MiB a ~2 GB de tablas, subida de `MODELO` y cambio del presupuesto de
memoria que hoy limita `nworkers` a 4. **Se para y se avisa.**

## 5 · Lo que hay que arreglar del banco antes de medir nada más de esto

🚨 **P15 choca de frente.** `docs/03`: *«medirla a 1 o 10 MB diría "no funciona" cuando la verdad
sería "no ha empezado". Se mide a ≥100 MB»*. **`texto5` son muestras de 10 MB.** Para la ESCASEZ
el sesgo juega a favor del veredicto (a menos datos, más escasez), pero para cualquier medida de
RATIO de nucleación haría falta el punto grande. Y la regla de las dos escalas (`docs/06`) dice
que un cambio no está medido hasta que se ha medido en dos tamaños.

## 6 · Los sitios donde vcompresor «decide que dos cosas son lo mismo con una regla a mano»

El diagnóstico de `docs/04` aplicado aquí — es lo más valioso que transfiere, porque es método:

1. **El StateMap** `sm[(i<<8)|e]`: todos los contextos en el mismo estado comparten probabilidad.
   La clase de equivalencia más grande del compresor, escrita a mano hace veinte años en PAQ.
2. **`lb = match mm_len { 0=>0, 1..=7=>1, 8..=31=>2, _=>3 }`** (:2260) — cubos de evidencia con
   fronteras inventadas. Es el foco 4 de vnervio, calcado, y aquí ni se ha probado otro valor.
3. **`v >>= 1` si `nn0+nn1 <= 2`** — proporción FIJA. Su m14 dice que envejecen mal.
   ⚠️ Pero el censo dice que sólo dispara en el 0,53-11,06% de las predicciones: es de baja
   prioridad, y esa cifra es exactamente el tipo de cosa que evita gastar una tarde.
4. **`PODA_DESDE = 8 MiB`** — umbral a mano que decide qué 16 de 36 modelos mueren.
5. **`DUPLICADOS`** — este SÍ está medido (F5 verificó que apagar el orden-3 da bytes idénticos).
