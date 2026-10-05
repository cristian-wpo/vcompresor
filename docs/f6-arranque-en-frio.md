# F6 · Q3 — de qué está hecho el +1,43% de partir en frames

Encargo del dueño (10-08-2026): partir la entrada en frames para usar los núcleos cuesta bytes
—`--solid=32` daba **+1,427%** sobre el corpus de código en F5— y ese número son **dos costes
distintos sumados**, que se arreglan de forma distinta:

- **(i) tablas frías** — cada frame arranca con las tablas vacías y vuelve a aprender lo que el
  frame anterior ya sabía. Lo arregla **cebar**: arrancar de una instantánea de estado.
- **(ii) coincidencias largas perdidas** — los modelos de match no pueden mirar al otro lado de
  la frontera del frame. Lo arregla dar **historia de sólo lectura**.

Si el grueso es (i), o (i)+(ii), A4 (usar los núcleos) deja de costar bytes, deja de ser vía (b)
y el eje del paralelismo pasa de valer 1,76× a valer 4-5×. Si el grueso es irrecuperable, el
techo de F5 se confirma y no hay que escribir el resto.

**Este encargo SÓLO MIDE.** No implementa el cebador en el producto: eso es Q5, y leerá esto.

---

## 0. Lo que hay que decir antes de nada: el corpus de F5 ya no está en el servidor

El corpus `codigo` de F5 —**136.415.253 B en 27.162 ficheros**, en
`/tmp/compresor-f1/corpus/codigo`— **ya no existe**. `/tmp` se limpió: de aquel árbol sólo queda
`/tmp/compresor-f1/samples/` (dos muestras de 10 MB). Se buscó en todo el disco
(`find / -type d -name codigo`) y no hay copia ni tarball. Ninguna cifra de F5 medida sobre él
—los 9.043.664 B, el +1,427%— se puede volver a derivar. **No se finge que sí.**

Se congela un sustituto del mismo tamaño y del mismo tipo, construido con código real del
servidor (fuentes de `~/.cargo/registry/src`, `/usr/include`, `/usr/lib/python3.12`):

| | corpus F5 (perdido) | corpus F6·Q3 (este) |
|---|--:|--:|
| bytes | 136.415.253 | **136.475.130** |
| ficheros | 27.162 | **9.491** |
| ruta | `/tmp/compresor-f1/corpus/codigo` | `/tmp/corpus-f6q3/codigo` |
| sello del manifiesto (sha256 de `sha256sum` por fichero, ordenado) | — | `e1e4ad978028517970dc889c680b0757a5f678c25ef6bebcdff7726923b6cd3b` |

Está puesto en sólo lectura (`chmod -R a-w`) para que ninguna tanda lo mueva a mitad.

**Consecuencia para leer este documento.** El *reparto* —qué fracción del hueco recupera cada
condición— es la pregunta del encargo y se contesta enteramente dentro de esta tanda, sobre una
entrada idéntica byte a byte en las cuatro condiciones. Lo que **no** se puede hacer es repartir
el número `+1,427%` de F5: ese porcentaje es de un corpus que ya no existe. Se mide y se reparte
**el porcentaje que este corpus da hoy**, y se dice cuál es.

---

## 1. La perilla de banco

Copia el patrón de la ablación (`MODELO_ABLACIONADO`, 31-07) y del banco DMC: **con la perilla
apagada no cambia ni un byte; con la perilla puesta, el archivo se marca con el modelo 254** para
que falle limpio y sea ilegible a propósito. No se toca el formato del contenedor ni se añade una
versión de modelo.

| variable | qué hace | ¿mueve bytes con ella apagada? | ¿marca 254? |
|---|---|---|---|
| `VC_CEBA=<ruta>` | ceba el predictor con esos bytes antes de codificar la entrada: se **modelan** (el predictor aprende) y **no se emite ni un bit** por ellos | no | **sí** |
| `VC_CEBA_VENTANA=1` | además deja que los modelos que miran atrás **lean** los bytes del cebador | no | (va con `VC_CEBA`) |
| `VC_BANCO_FRAMES=<dir>` | vuelca el contenido CRUDO de cada frame tal y como lo arma el archivador | no (sólo escribe una copia) | no |
| `VC_BANCO_SOLIDO=1` | baja el suelo del clamp de `--solid` de 32 a 1 MiB | no | no |

**Por qué el 254 en `VC_CEBA` y no en las otras dos.** El cebador NO viaja en el archivo: el
descompresor arrancaría con las tablas vacías y devolvería basura del tamaño exacto, con exit 0 y
sin una queja — exactamente el fallo que la ablación ya obligó a tapar. `VC_BANCO_FRAMES` sólo
escribe una copia y `VC_BANCO_SOLIDO` sólo cambia el troceado: un `.vc` con frames de 11 MiB es
perfectamente válido y legible, así que marcarlo sería mentir en la otra dirección.

**Cómo se corta la historia sin tocar las tablas** (`Predictor::corta_ventana`, llamada una vez en
la frontera). El predictor sigue sabiendo todo lo que aprendió —cubos, StateMaps, pesos del
mezclador, DMC, cadena ISSE, indirectos, contadores de separador— y deja de poder leer un solo
byte de lo anterior:

- se borran las tablas de los **cinco** match models (`mm`, `mm2`, `mm3`, `mm4`, `mm8`) y sus
  punteros vivos, así que ningún candidato puede apuntar al cebador;
- `vent_min` corta las **otras dos** lecturas de `mm_buf` que hay en el modelo: el contexto de
  orden 12 (`cxt[14]`) y el «byte de encima» de los dos modelos de fila (`cxt[12]`, `cxt[13]`,
  `cxt[18]`, `cxt[19]`).

`mm_buf` **no** se trunca a propósito: `sep_last`/`sep_prev` guardan posiciones absolutas dentro
de él y truncarlo las dejaría apuntando fuera. Se corta por lectura, no por tamaño.

`vent_min` vale **0** fuera del banco, o sea que las guardas nuevas son `x >= 0` y no pueden
cambiar nada. Eso es un argumento; la comprobación está en §2.

---

## 2. Los sha de control, con la perilla apagada

Binario de control = el mismo árbol **sin** el cambio (`git stash`, `cargo build --release`).
Binario nuevo = con el cambio. Misma entrada, perillas sin poner:

| sello del binario | A (control) | B (nuevo) |
|---|---|---|
| `vc-cm` | `a5f01756d7c3` | `48a87e59a285` |
| `vcompresor` | `d83950b8b7eb` | `e7a705aaaef0` |

| entrada | herramienta | sha256 de la salida con A | con B | |
|---|---|---|---|---|
| `ctrl.bin` (1.069.333 B) | `vc-cm` | `94e5e115…980111a` | `94e5e115…980111a` | **=** |
| `ctrl2.bin` (5.426.855 B) | `vc-cm` | `27af5cf8…5c9bf00e13` | `27af5cf8…5c9bf00e13` | **=** |
| `ctrl2.bin` (5.426.855 B) | archivador | `b29f7fa3…81c0442b6b8` | `b29f7fa3…81c0442b6b8` | **=** |

Los tres pares son **idénticos byte a byte**, con las dos herramientas y en los dos tamaños. Eso
convierte en medida lo que al final de §1 sólo era un argumento: `vent_min` vale 0 fuera del
banco, así que las guardas nuevas no pueden cambiar nada. Los sha completos, los cuatro sellos y
la procedencia están en [`bench/registros/f6q3-control.txt`](../bench/registros/f6q3-control.txt),
versionado —y no en `/tmp`, que es exactamente como se perdió el corpus de F5 (§0).

---

## 3. El experimento — cuatro condiciones sobre el MISMO frame

Sobre el corpus congelado, con `--solid=S`, se vuelcan los frames crudos del carril de texto
(`VC_BANCO_FRAMES`) y se confirma con `vcompresor info` cuántos frames salen **de verdad** — no se
supone. Se coge el frame **1** (el segundo, para que tenga uno delante) y se mide con `vc-cm`
suelto, que es el mismo códec que corre dentro del archivador:

| | qué es | cómo se mide |
|---|---|---|
| **F0** | el frame 0 solo, en frío | `vc-cm c f0` |
| **(a) FRÍO** | el frame 1 con el estado inicial vacío. **Es lo que pasa hoy.** | `vc-cm c f1` |
| **(b) TABLAS PRIMADAS** | el frame 1 arranca del estado como quedó al terminar el frame 0; los modelos de match **sin** historia | `VC_CEBA=f0 vc-cm c f1` |
| **(c) PRIMADAS + HISTORIA** | como (b) y además ventana de sólo lectura sobre los bytes del frame 0 | `VC_CEBA=f0 VC_CEBA_VENTANA=1 vc-cm c f1` |
| **(d) TECHO** | frame 0 y frame 1 como UN solo bloque sólido | `vc-cm c (f0‖f1)` |

Todo se compara **en totales de los dos frames**, que es lo único que no mezcla geometrías de
tabla (el `tam` con el que se construye el predictor fija `tmask`/`imask`, y en (d) el bloque es
del doble de tamaño):

```
(a) = F0 + F1_frío        (b) = F0 + F1_cebado       (c) = F0 + F1_cebado_ventana       (d) = F01
hueco = (a) - (d)
recupera (b) = F1_frío - F1_cebado         recupera (c) = F1_frío - F1_cebado_ventana
```

Los totales (a)(b)(c) llevan **dos** cabeceras de flujo (10 B cada una) y (d) lleva **una**: 10 B
de diferencia estructural sobre ~2 MB, que se dice en vez de esconderlo y no mueve ninguna
conclusión.

**Y repetido al tamaño que importa.** El objetivo real no son 4 frames sino ~12, para llenar la
máquina. Se repiten al menos (a), (c) y (d) con `--solid=16` y con el tamaño que dé ~12 frames
(~11 MiB): la pregunta de verdad es si la recuperación **aguanta cuando el frame se hace pequeño**.
Un método que recupera a 32 MiB y se hunde a 11 MiB no sirve para lo que lo queremos.

### Trampas de esta zona que NO se vuelven a descubrir

- `--no-solid` es **inerte** con `--max`: el parser lo traduce a sólido=0 pero el carril de texto
  impone su propio tamaño y da los MISMOS frames y los MISMOS bytes que `--solid=32`. No se usa
  como condición de nada.
- `--solid` estaba en `clamp(32, 256)`: bajar de 32 exige tocar el clamp, y eso es cambio de
  **banco**, no del defecto (`VC_BANCO_SOLIDO`).
- Los BYTES son deterministas: **ningún umbral de ruido se aplica a bytes** (aplicarlo ya borró 25
  mejoras reales una vez, `03b8f61`). A cambio se exige entrada EXACTAMENTE la misma, no parecida.
- El reloj se mide en **ciclos**, y se comprueba `bin_sha` en cada fila: un checkout sin
  `cargo build` deja el árbol limpio y un binario de otro commit.

---

## 4. FALSADOR — declarado ANTES de correr

> Que la condición **(c) siga costando MÁS del 0,5% de bytes contra (d)**, y que además **ese
> coste CREZCA al bajar a ~12 frames**.
>
> Si eso pasa: el corte es **intrínseco**, A4 se queda en vía (b), el techo de F5 se confirma tal
> cual, y este encargo cierra en **NEGATIVO** — que es un resultado bueno, y se anota con su
> premisa escrita, porque un negativo caduca con su premisa.

**Premisa de este negativo (si sale):** que el cebado se hace desde el estado final del frame
inmediatamente anterior y que la ventana de historia es el frame anterior COMPLETO. Es la versión
más favorable que se puede implementar: si ni así recupera, ninguna versión más barata lo hará. Un
cebador que arrancara de una instantánea genérica —un diccionario— es otra cosa y no queda
falsado por esto.

---

## 5. Resultados

Todas las filas de abajo salen de `bench/historia.jsonl` (versionado) y se re-derivan con
`bench/f6q3-reparto.py <solid>`. Las cinco condiciones llevan el **mismo sello de binario**
`48a87e59a285` — el script se niega a restar filas de binarios distintos.

### 5.1 · Cuatro frames (`--solid=32`) — MEDIDO

`vcompresor info` da **4 frames** de carril 0 sobre el corpus congelado. Se mide el par
frame 0 + frame 1: frame 0 = **33.547.406 B**, frame 1 = **33.552.299 B**. El frame 0 en frío
comprime a **2.155.023 B** y es el mismo sumando en (a), (b) y (c).

| condición | F1 | total 2 frames | vs (d) | recupera | sobre el techo |
|---|--:|--:|--:|--:|--:|
| **(a) frío** — *es lo que pasa hoy* | 1.963.405 | **4.118.428** | +50.514 | — | **+1,242%** |
| **(b) tablas primadas** | 1.915.496 | **4.070.519** | +2.605 | 47.909 | **+0,064%** |
| **(c) primadas + historia** | 1.912.906 | **4.067.929** | **+15** | 50.499 | **+0,000%** |
| **(d) techo, un solo bloque** | — | **4.067.914** | 0 | 50.514 | (referencia) |

**El hueco de partir es 50.514 B = +1,242%** sobre el techo. (b) recupera **94,8%** del hueco;
(c) recupera **100,0%**.

### 5.2 · El reparto del coste de partir — MEDIDO, a 4 frames

Las tres piezas suman el hueco exacto (47.909 + 2.590 + 15 = 50.514):

| pieza | cómo se saca | bytes | del hueco | del techo |
|---|---|--:|--:|--:|
| **(i) tabla fría** | (a) − (b) | **47.909** | **94,84%** | +1,178% |
| **(ii) coincidencia larga perdida** | (b) − (c) | **2.590** | **5,13%** | +0,064% |
| **irrecuperable** | (c) − (d) | **15** | **0,03%** | +0,000% |

De esos 15 B irrecuperables, **10 B son estructurales** — (a)(b)(c) llevan dos cabeceras de flujo
de 10 B y (d) lleva una. El residuo de modelado que no explica ninguna de las dos causas es de
**5 B sobre 4,07 MB**. Sin «probablemente»: a 32 MiB de frame, **el coste de partir es recuperable
al 100,0%**, y está hecho de tabla fría en un 94,8%.

### 5.3 · Doce frames (`--solid=11`) — MEDIDO

`VC_BANCO_SOLIDO=1 --solid=11` da **12 frames** de carril 0, que es el tamaño que importa (llenar
la máquina). Los doce frames crudos están en `/tmp/f6q3/frames-11/`
(frame 0 = 11.479.652 B, frame 1 = 11.464.040 B). Las cinco condiciones ya están en el libro mayor
(mismo sello de binario `48a87e59a285`) y se re-derivan con `bench/f6q3-reparto.py 11`. F0
comprime a **846.279 B** y es el mismo sumando en (a), (b) y (c).

| condición | F1 | total 2 frames | vs (d) | recupera | sobre el techo |
|---|--:|--:|--:|--:|--:|
| **(a) frío** — *es lo que pasa hoy* | 612.167 | **1.458.446** | +40.551 | — | **+2,860%** |
| **(b) tablas primadas** | 574.304 | **1.420.583** | +2.688 | 37.863 | **+0,190%** |
| **(c) primadas + historia** | 571.629 | **1.417.908** | **+13** | 40.538 | **+0,001%** |
| **(d) techo, un solo bloque** | — | **1.417.895** | 0 | 40.551 | (referencia) |

**El hueco de partir es 40.551 B = +2,860%** sobre el techo. (b) recupera **93,4%** del hueco;
(c) recupera **100,0%**. De los 13 B que le quedan a (c) contra (d), 10 B son estructurales (dos
cabeceras de flujo contra una, igual que en §5.2); el residuo de modelado es de **3 B sobre
1,42 MB**. La forma en que se llegó a estas filas —driver de tanda pid 3208594, log
`/tmp/f6q3/tanda.log`— queda en `bench/historia.jsonl` (versionado), que es lo único que hace
falta para re-derivar esta tabla.

**Y el número que falta también es el del archivo entero.** El `+1,242%` de §5.1 es del **par de
frames**, no del archivo de 4 frames contra el de 1 — el equivalente directo del `+1,427%` de F5
es `--solid=32` contra `--solid=64` sobre el corpus completo, y esas dos corridas son las dos
últimas de la tanda. `todo-32.vc` y `todo-64.vc` siguen hoy en **0 B**: la tanda no ha llegado
todavía a esas dos corridas. `todo-16.vc` ya no está en ese caso — son **8.491.681 B**, fila
«F6.Q3 archivo entero solid=16» en el libro mayor —, pero no es el par que hace falta
(`--solid=32` contra `--solid=64`), así que el número del archivo entero sigue sin poder darse.

---

## 6. El precio de cebar

Las condiciones (b) y (c) miden lo que el cebado **compra** en bytes. No miden lo que **cuesta**
montarlo, y esa es la mitad que decide: A4 no existe para comprimir mejor, existe para usar los
doce núcleos. Se reproduce con [`bench/f6q3-precio-cebar.py`](../bench/f6q3-precio-cebar.py).

**El estado que hay que mover son 1.453 MiB, no 483 MiB.** F5 anota «483 MiB de tablas» y esa
cifra es de la configuración por defecto: `tam` fija `tmask`/`imask`, así que a bloques de 32 MiB
las tablas son otras. Los 1.453 MiB salen del RSS que el libro mayor tiene anotado para *estas*
corridas, menos la entrada, que también está residente.

**1 · El memcpy es barato, y no es lo que miden (b) y (c).** Conviene separarlo, porque si no el
precio sale 400 veces peor de lo que es. La forma en que (b)/(c) *miden* el cebado —volver a pasar
el frame 0 por el modelo— cuesta **+1.374,5 Gciclos** ((c) 2.743,10 contra (a) 1.368,57). La forma
en que se *implementaría* —guardar una instantánea al acabar el frame 0 y copiarla— cuesta un
memcpy de 1.453 MiB: **874 ms, 1,6 GiB/s, 3,4 Gciclos** por frame y por hilo. El coste de la
medida no es el coste del producto, y el que hay que poner en la balanza es el segundo.

**2 · La RAM aguanta.** Doce hilos son doce predictores vivos, y cebar añade una instantánea por
hilo: 17,0 GiB hoy → **34,1 GiB**, el 55% de los 62 GB de la máquina. Cabe. Esta no es la
objeción.

**3 · El camino crítico en serie sí lo es, y no hace falta medirlo: es una dependencia.** Cebar el
frame N desde el estado **final** del frame N−1 encadena los frames — el N no puede empezar hasta
que el N−1 termine:

| | hoy (frío, en paralelo) | cebado en cadena | |
|---|---|---|---|
| F=4 frames | `1,00·T` | `4·T` | **4,0× más lento** |
| F=12 frames | `1,00·T` | `12·T` | **12,0× más lento** |

El eje del paralelismo vale 1,76× y se buscaba 4-5×. Un cebado en cadena no lo sube: lo baja a 1×
**por construcción**, porque deja de haber paralelismo que medir. Se pagarían los 50 KB con la
totalidad de lo que A4 iba a comprar.

---

## 7. Recomendación

### 7.1 · El falsador, contra lo medido

El falsador de §4 pide **dos** cosas a la vez: que (c) cueste **más del 0,5%** contra (d), **y** que
ese coste **crezca** al bajar a ~12 frames.

| mitad del falsador | estado |
|---|---|
| (c) cuesta >0,5% contra (d) | **NO se cumple, ni a 4 ni a 12 frames**: +0,000% (15 B) a `--solid=32`, **+0,001%** (13 B) a `--solid=11` — §5.3, medido |
| el coste crece a ~12 frames | depende de la unidad: **en %, crece** (0,000→0,001); **en bytes, baja** (15→13) |

**La unidad que manda es la del propio falsador (§4): porcentaje contra (d), no bytes crudos** —
el falsador está escrito en «más del 0,5%» precisamente porque el techo cambia de tamaño con
`--solid`, y comparar bytes entre dos techos de tamaño distinto (4,07 MB contra 1,42 MB) no dice
nada. En esa unidad, el coste de (c) sí **crece** al bajar a 12 frames (0,000%→0,001%), así que la
segunda mitad del falsador se cumple. Pero la primera no: **+0,001% está tres órdenes de magnitud
por debajo del umbral de 0,5%**, y el falsador exige las dos a la vez. **El falsador no se
dispara** — con las dos mitades ya medidas, no con una pendiente.

Esto resuelve también el reparto: el coste de partir **no es intrínseco**, ni a 32 MiB de frame ni
a 11 MiB. A 4 frames es tabla fría en un 94,8%, coincidencia perdida en un 5,13%, e irrecuperable
en 15 B de 4,07 MB (10 B estructurales, 5 B de residuo). A 12 frames (c) recupera el **100,0%** del
hueco de 40.551 B, con 13 B irrecuperables contra (d) (10 B estructurales, 3 B de residuo). La
hipótesis (i) del encargo se confirma con holgura, y la (i)+(ii) cierra el hueco entero en las dos
geometrías medidas.

### 7.2 · El hallazgo que NO depende de la corrida pendiente

Y aun así **el eje del paralelismo no pasa de 1,76× a 4-5× cebando**, y esto se puede decir ya
porque es una dependencia, no una medida (§6):

- El memcpy **no** es la objeción: 1.453 MiB en **874 ms = 3,4 Gciclos**, contra los **1.368,57
  Gciclos** que cuesta un frame en frío a `--solid=32`. Es el **0,25% de un frame**. Barato.
- La RAM **no** es la objeción: 17,0 GiB → **34,1 GiB** de los 62 GB, el 55%. Cabe. ⚠️ Pero el
  **guardia de RAM no modela el cebado**: reserva `2048 + sólido` MB por hilo —2,06 GiB/hilo a
  `--solid=11`, 24,7 GB de un tope del 60% (37,2 GB)— cuando el consumo real cebando sería de
  **2,84 GiB/hilo**. Dejaría pasar una configuración que gasta más de lo que reservó. Es un arreglo
  de Q5, no un bloqueo de aquí.
- **La objeción es el camino crítico**, y sale del hecho de que el primer es *un frame entero de
  modelado*. Con 12 frames y 12 hilos:

| diseño | camino crítico | contra hoy |
|---|---|---|
| hoy — 12 frames en frío, en paralelo | `1,00·T` | (referencia) |
| **primer único** — un frame ceba, los 11 arrancan de esa instantánea | **`2,00·T`** | **2,0× más lento** |
| **cadena** — el frame N ceba del estado final del N−1 (lo que miden (b) y (c)) | `12,00·T` | 12,0× más lento |

El número que pedía el encargo —«el camino crítico es primer + un frame»— es **`2,00·T`**. Y ese es
el diseño **favorable**, no el medido: partir el trabajo y luego anteponerle un frame en serie
**divide por dos** el factor de paralelismo, o sea que el 1,76× de A4 se queda en ~0,88× — peor que
no partir. La cadena de §6 es todavía peor.

⚠️ **Y el diseño favorable no está medido.** (b) y (c) ceban el frame **1** desde el frame **0**,
que es el caso en que «primer único» y «cadena» coinciden. Cebar los frames **2…11** desde el
estado del frame 0 —más lejos— recupera **menos**, y cuánto menos no lo dice este experimento.

### 7.3 · La recomendación

**SEGUIR a Q5, pero no con lo que se ha medido: con un primer BARATO.** El resultado en bytes es
mejor de lo que nadie esperaba —el hueco se cierra entero— y por eso la vía no se cierra. Pero
cebar desde *un frame anterior completo* está falsado por el reloj y no por los bytes: cuesta un
frame en serie, y un frame en serie se come el paralelismo que A4 existe para comprar.

Lo que Q5 tiene que contestar, en este orden y **antes** de tocar el producto:

1. ~~Cerrar §5.3~~ — **hecho**: a 12 frames (11 MiB) (c) recupera el 100,0% del hueco y el falsador
   no se dispara (§7.1). No pasa a NEGATIVO; la vía (i)+(ii) aguanta en las dos geometrías medidas.
2. **Un primer que cueste ≪ T**, no `1·T`: un prefijo corto en vez del frame entero, o una
   instantánea genérica calculada una vez y fuera de línea. La pregunta es la curva
   *bytes recuperados vs tamaño del primer* — con un primer del 10% de un frame el camino crítico
   sería `1,10·T` y el paralelismo casi no se tocaría.
3. **Medir el cebado a distancia** — frames 2…11 desde el estado del frame 0 (§7.2), que es lo que
   de verdad se implementaría y lo que este encargo no midió.

⚠️ **A4 sigue siendo vía (b) hoy.** Nada de este documento la mueve a vía (a) todavía: para eso
hace falta un cebador que recupere los bytes **sin** añadir un frame al camino crítico, y ese no
se ha medido. El techo de F5 **no** se confirma —hay una vía abierta y con evidencia a favor— pero
**tampoco** se levanta.

**Y este encargo PARA aquí.** No se implementa el cebador en el producto: la regla de parada del
brief es explícita y Q5 es otro encargo, que leerá esto.
