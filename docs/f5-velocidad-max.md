# F5 — acelerar `--max`: lo medido, lo aplicado, lo rechazado y con qué falsador

Encargo del dueño (03-08-2026): dejar `--max` **muy rápido, por todas las vías**, sin perder el
ratio que nos hace ganar al campo. La regla dura del encargo: lo que cambie un solo byte **no
entra al defecto**; va al menú y lo elige `--price`.

Este documento se escribe a la vez que se mide. Cada apartado dice **qué lo falsaría** antes de
enseñar el resultado, que es lo único que separa un experimento de una corazonada con decimales.

---

## Fase 0 — la báscula (el gate)

Binario `b2ce172b1b04`, commits `86c4630`/`427d31f`, host `vertice`. Todo por `bench/mide.sh` e
integrado en `bench/historia.jsonl`.

### La línea de partida de HOY

| medida | valor |
|---|--:|
| árbol `corpus/codigo` (136.415.253 B) con `--max` | **597,6 s · 0,228 MB/s · 29.634 ciclos/byte** |
| bytes de salida | **9.043.664 B** (sha256 `35ace548fa4a32e25c0ca1542c0af28a5b074e6dedf21a542f5f52386de18507`) |
| CPU | 168% — o sea **1,7 núcleos de 12** |
| `codigo.10m`, un frame, un hilo | 30.785 ciclos/byte · 0,130 MB/s |
| referencia CM sobre 64 MiB | 33.334 ciclos/byte |

🔑 **Los 740,8 s del brief ya no existen.** v6 («los tres» fuera) y v7 (dispersión fuera)
devolvieron **−19,3% de reloj** antes de empezar este trabajo. Publicar la mejora contra los
740,8 habría sido cobrar dos veces lo que ya estaba cobrado.

### El campo, medido en la misma tanda y con el mismo instrumento

| | reloj | ciclos/byte | bytes |
|---|--:|--:|--:|
| **nosotros `--max`** | 597,6 s | 29.634 | **9.043.664** |
| zpaq `-m5` | 133,3 s | 6.280 | 10.901.842 |

**4,48× más lentos en reloj (4,72× en ciclos) con −17,05% de bytes.** No 5,3×: la cifra del brief
era de antes de v6+v7.

paq8px `-8` se cortó a los 68 minutos con 53 KB escritos de ~30 MB. A su velocidad medida
(0,008 MB/s) le quedaban ~5,5 horas **reteniendo el único carril de reloj del proyecto**, y su dato
es de contexto: marca el techo de ratio y el suelo de velocidad, no el listón que perseguimos.
Se dice que falta en vez de fingir que se midió.

### Cómputo o memoria — la pregunta que decide el orden de todo lo demás

`perf`, `codigo.10m`, tres corridas separadas para no pedirle al contador más de lo que puede dar:

| | valor |
|---|--:|
| IPC | **2,45** |
| instrucciones por byte | **75.463** |
| fallos de rama | 0,65% |
| ciclos ociosos de frontend | 5,25% |
| fallos de último nivel por byte | 0,139 |
| fallos de TLB de datos por byte | 0,044 |

**Veredicto: el cuello es CÓMPUTO.** Con IPC 2,45 el procesador está retirando trabajo casi todo
el tiempo; incluso cobrando cada fallo de caché a latencia de RAM pesimista, la memoria explica
≤14% del reloj. **La hipótesis 4 del brief queda refutada en su parte fuerte**: no estamos
esperando a la RAM, estamos ejecutando demasiado.

Y se entiende leyendo el bucle: `fija_cubos` trae el cubo **dos veces por byte** —ya en dos
pasadas y con prefetch— y los quince nodos del nibble se leen de esos 16 bytes, que están en L1.
Las páginas grandes ya están puestas. La parte de memoria del problema **ya estaba resuelta antes
de este trabajo**; lo que queda es aritmética.

**Consecuencia para el plan**: C4 (achicar tablas) baja de prioridad, A5 (tubería de acceso) ataca
un hueco pequeño, y suben las palancas que **ejecutan menos**: C3, C2, C1 y el reparto de A4.

### V0 — de dónde salió la regresión

Nueve puntos entre `fa1f379` y HEAD, compilando cada uno, misma entrada, **comparando ciclos/byte**
(el reloj no vale: la máquina lleva la flota encima). Integrados con `bench/integra.py`.

| commit | ciclos/byte | Δ | bytes |
|---|--:|--:|--:|
| `fa1f379` | 12.971 | — | 825.855 |
| `300a4a2` | 18.284 | **+41,0%** | 807.028 |
| `aadec9f` | 19.968 | +9,2% | 807.066 |
| `20998a6` | 19.034 | −4,7% | 807.066 |
| `08418ba` | 17.878 | −6,1% | 806.399 |
| `0f32bdb` | 17.765 | −0,6% | 805.998 |
| `01487c8` | 39.517 | **+122,4%** | 794.425 |
| `fed3d3f` | 39.696 | +0,5% | 788.275 |
| `427d31f` | 31.125 | −21,6% | 787.470 |

**No sale plano: la regresión es real y son 2,40× en el códec.** (Si hubiera salido plana, el
problema habría sido el instrumento y no el compresor — ese era el falsador, y no se cumplió.)

**Escalón A, +41,0%** (`fa1f379..300a4a2`): compró −2,28% de bytes. Es la construcción F2.34–F2.59
—match disperso y zancadas, cascada SSE de cinco etapas, modelo numérico, familia de texto— con la
precarga (−24%) dentro. Compra consciente y documentada.

**Escalón B, +122,4%** (`0f32bdb..01487c8`): compró −1,44% de bytes. De sus seis commits que tocan
`vc-cm`, el que carga el gasto es **`02bbb19` — la memoria de byte**: cada cubo recuerda el último
byte visto y su racha, con un mapa aprendido por modelo y por bit. Añade 36 entradas al mezclador
y dos tablas nuevas de aprendizaje.

El árbol añade ~1,6× por encima del códec (política de carriles y frames posterior al 28-07: cada
miembro de un ZIP a su carril, frames de 32 MiB). Atarlo con decimal es una tarde de medida y no
bloquea nada.

### Determinismo cruzado — 5/5

Mismo binario, mismas muestras, `vc-cm c` en x86_64 y en arm64:

| muestra | bytes | sha256 (16) |
|---|--:|---|
| codigo | 787.080 | `af18464ffb482fa0` |
| csvjson | 540.064 | `eedfa76e795cd24b` |
| enwik8 | 2.088.518 | `9af1e2cb5309e4d2` |
| logs | 48.560 | `70497d35868d3a59` |
| sql | 225.810 | `ad5ece6052c087b9` |

Estos cinco sha son **el control de toda la Fase 1**: cualquier palanca de la vía (a) que mueva uno
solo de ellos no entra, por mucho que corra.

---

## El lote de bytes en paralelo — cómo se midió, y por qué se puede

494 corridas, 6 a la vez, sobre las ocho muestras del juego (`texto5` + `no-texto3`).

**Por qué puede ir en paralelo**: los bytes son deterministas. La salida no depende de la carga, así
que N corridas simultáneas dan exactamente los mismos números que N en serie. Lo que **no** se
puede paralelizar es el reloj, y aquí no se cronometra nada.

**Por qué 6 y no 10**: con 10 procesos el lote iba a 6,9 horas. Cada uno carga 483 MiB de tablas;
diez son 4,8 GB peleándose por 32 MiB de L3 y por **6 núcleos físicos** (los otros seis son SMT y
no ayudan a un bucle limitado por cómputo). El paralelismo ×10 rendía ×2,7. Es la misma física que
midió la Fase 0, vista desde fuera.

⚠️ **Esto es un CRIBADO, no una decisión.** Sin reloj ni ciclos, ninguna fila puede entrar en
`historia.jsonl` — el libro empareja por ciclos y meter tiempos medidos en paralelo lo
envenenaría. Los ganadores se re-miden **en serie** con `mide.sh` y solo entonces se anclan.

**Prueba de sanidad del lote**: `VC_LIM=127` —que es el defecto— devolvió **0,000% exacto en las
ocho muestras**. Una configuración que iguala al defecto reproduce los bytes al bit; eso valida a
la vez el control, el analizador y que el paralelismo no toca los números.

---

## Lo medido hasta ahora

### El defecto corre 20 de los 36 modelos, no 36

Dos filas del lote lo prueban sin necesidad de leer el código:

- Apagar el racimo `sparse` (8,9,22,23,24) da **+0,000% exacto en las cinco de texto** y **+7,420%
  en `bin`**. Las de texto son de 10 MiB y cruzan `PODA_DESDE` (8 MiB), así que ya estaban
  apagados; `bin.8m` son 5,3 MB y no la cruza. La poda por tamaño hace exactamente lo que dice.
- Apagar los órdenes enteros y apagarlos **dejando el orden-3 encendido** dan bytes **idénticos**:
  el orden-3 ya lo apagaba `DUPLICADOS`.

🔑 **Corrección de una cifra que este proyecto viene arrastrando**: «36 modelos» y «159 entradas por
bit» son las cifras del código, no las del defecto. Sobre bloques de ≥8 MiB corren **20 modelos de
tabla**; los otros 16 los apagan `DUPLICADOS` y `PODA_V2`. Y por tanto **el −14% de reloj que
`bench/ablacion.tsv` atribuía a los órdenes clásicos ya está cobrado**: lo que queda vivo del
racimo es el orden-0.

### Las cuatro vistas (V2) — las 144 entradas que el brief señalaba

| configuración | total | texto5 | no-texto3 | señal |
|---|--:|--:|--:|---|
| `VC_RACHA=0` | **+0,870%** | +0,788% | +1,048% (bin +2,605%) | 8/8 empeoran |
| `VC_VISTAS=2` | +0,268% | +0,602% | +0,007% | pierde en texto |
| `VC_VISTAS=1` | +0,213% | +0,200% | +0,243% | 8/8 empeoran |
| `VC_TRES=1` | **−0,102%** | −0,030% | −0,259% | 7/8 mejoran |

**La racha es el intercambio más caro del compresor.** Es el `02bbb19` del escalón B: **+122% de
ciclos por 0,87% de bytes**. Primera fila del perfil rápido, y de paso se lleva 20 entradas del
mezclador.

**Y `v6-tres-fuera` se decidió sobre texto**: en no-texto cuesta 5× más (−0,259% contra −0,030%).
No la invalida —el −24,3% de ciclos sigue siendo buen negocio— pero es la lección de los órdenes
clásicos otra vez: **el signo y la magnitud cambian entre texto y binario**.

### Racimos (B3r), las ocho muestras

| racimo apagado | bytes | texto5 | no-texto3 | reloj que ahorraba (medido antes) |
|---|--:|--:|--:|---|
| órdenes clásicos | +0,069% | −0,012% | +0,245% | ya cobrado (ver arriba) |
| familia de texto | +0,168% | +0,237% | +0,018% | sin medir |
| familia numérica | +0,223% | +0,316% | +0,022% | sin medir |
| anidamiento | +0,268% | +0,332% | +0,127% | sin medir |
| DMC | +0,574% | +0,753% | +0,184% | −4% |
| **fila** | +0,588% | +0,696% | +0,352% | **−15,8%** |
| match-disp | +0,697% | +0,754% | +0,572% (sql +7,910%) | sin medir |
| indirectos | +1,011% | +0,852% | +1,357% | −6% |
| palabra/parejas/forma | +1,811% | +2,471% | +0,372% | −3% |
| los cinco match | +2,514% | +3,243% | +0,927% (sql +25,730%) | −3% |

**`match-disp` queda cerrado: 8 de 8 en contra de quitarlo.** La fila `RECHAZADO` de
`decisiones.tsv` estaba marcada INCOMPLETA por haberse medido sobre dos muestras; ahora tiene las
ocho y el veredicto se sostiene.

### P6 — el tope del StateMap: un positivo caducado

| `VC_LIM` | total |
|---|--:|
| 191 | −0,003% |
| **127 (defecto)** | **0,000%** |
| 255 | +0,009% |
| 63 | +0,075% |
| 511 | +0,085% |
| 1023 | +0,304% |

El censo apuntaba «191 contra 1023 = −0,231%, medido y sin aplicar». Comparaba contra un defecto
**que ya no existe**: la v3 lo bajó a 127. Contra el defecto real, 1023 sale +0,304% *peor* y 191
empata. 🔑 **Ya estaba cobrado.** La lección conocida —un negativo caduca con su premisa— vale
igual al revés: **un positivo también caduca**, y este llevaba semanas prometiendo una mejora que
no se podía cobrar dos veces.

---

## El hallazgo de velocidad: el mezclador paga 160 carriles para 53 que aportan

`punto()` y `actualiza_pesos()` recorren **`NCTXP` = 160 carriles fijos**, por bit y por cada uno de
los `NMIX` = 4 mezcladores: **1.280 multiplicaciones por bit** entre predecir y actualizar.

Las entradas que aportan hoy (v8, bloque ≥8 MiB) son ~53: 20 modelos activos, sus 20 vistas de
racha, 7 derivados y 6 salidas intermedias de la cadena. **Las otras ~107 son ceros multiplicados
por pesos que no se mueven** — entre ellas los 72 carriles de «los tres distintos» y «determinista»,
apagados desde v6 y ocupando sitio.

Compactar las entradas activas al principio del vector deja el producto escalar en ~56 carriles:
**~65% menos de trabajo en el mezclador**, y **el mismo resultado exacto** (sumar ceros no cambia
nada; los pesos son un renombrado de índices). Es vía (a) pura.

Esto convierte **C3** de «quitar una comprobación» en algo mayor: además de compactar el mezclador,
quita las 16 iteraciones muertas por modelo y por bit en predecir, actualizar y fijar cubos.

- **Control**: los cinco sha del determinismo cruzado, `idayvuelta` 20/20 y el sha del árbol.
- **Falsador**: que las instrucciones por bit no bajen (hoy 75.463 por byte). ⚠️ Y un riesgo
  concreto: `VC_LOAD_W` lee pesos pre-entrenados por posición — si se compactan los carriles hay
  que remapear ese fichero o dejará de casar en silencio.

### 🔑 CORRECCIÓN DE LIBRO MAYOR (10-08, F6·Q2) — este 53 nunca volvió a `docs/f4`

Este documento midió los ~53 de arriba, pero `docs/f4-velocidad-por-que-son-mas-rapidos.md`
seguía usando **159** para dividir «ciclos por entrada y bit» y concluía que somos 1,8× más
eficientes que zpaq por entrada — nadie rehizo esa división con el 53 que este mismo fichero ya
tenía medido. Instrumentado hoy con `VC_CENSO=1` sobre `codigo.10m` y `sql.10m` (bin_sha
`a5f01756`, commit `1d7f55d`): **53 de 159 activas en las dos** (censo completo commiteado en
`bench/registros/censo/`, no en `bench/registros/salidas/`: `VC_CENSO` imprime por stderr y
`mide.sh:179` sólo captura stdout), y con el 53 correcto el resultado se invierte — **~1,3×–1,6× PEORES
por entrada y bit, no 1,8× mejores**. La corrección completa, con las dos lecturas (cruda y
aislada a modelos vía `bench/camino-predecir.tsv`) y la cifra por corpus, vive en
`docs/f4-velocidad-por-que-son-mas-rapidos.md`, sección «CORRECCIÓN DE LIBRO MAYOR».

Consecuencia para este documento: el frente de «bajar el coste por pregunta» que la vieja cifra de
f4 daba por cerrado **no estaba agotado**, y eso reabre C1 por la vía de estructura de arreglos
(no `gather`, que ya está falsado abajo) como candidata real — con un coste por entrada ya
confirmado más alto que el de zpaq, hay margen aparte del que persigue C3.

---

## C1 — el falsador temprano se cumplió: `vpgatherdd` no sirve aquí

Antes de escribir una línea de C1 se midió la instrucción de la que dependía todo el diseño:
recolectar ocho valores dispersos. Mediana de nueve vueltas, 8 M de elementos por vuelta, con
`black_box` en entrada y salida para que el compilador no borrara el bucle.

| caso | escalar | gather | veces |
|---|--:|--:|--:|
| `sm` · 36 KB (en L1) | 1,21 c/e | 2,16 c/e | **0,56×** |
| `stretch` · 16 KB (en L1) | 1,11 c/e | 2,06 c/e | **0,54×** |
| tabla de 2 MiB (L2/L3) | 3,34 c/e | 3,24 c/e | 1,03× |
| 64 MiB (fuera de caché) | 19,34 c/e | 20,64 c/e | 0,94× |

**Pierde incluso en el caso más favorable.** En Zen 2 el gather se descompone en microoperaciones,
una por carril, mientras que las ocho lecturas sueltas las solapa el procesador. 🔑 **C1 no puede
ser «gather + aritmética AVX2»**: para vectorizar el 83,7% hay que reorganizar la memoria a
estructura de arreglos, que es mucho más trabajo, y eso se decide con la frontera delante.

Coste de saberlo: un minuto de máquina. Coste de no saberlo: escribir C1 entero para descubrirlo.

---

## LA FRONTERA — ciclos comprados por byte ahorrado

La mitad que faltaba. Bytes de las ocho muestras (lote paralelo); ciclos de dos (`codigo`, `sql`)
en serie bajo cerrojo. Control de la tanda de reloj: el defecto al abrir 31.519 / 31.752 ciclos por
byte y al cerrar 31.404 / 31.560 — dentro del suelo de ruido del reloj (1,25%).

| palanca | bytes | ciclos | **reloj por punto de ratio** |
|---|--:|--:|--:|
| **«los tres» apagado (v6, ya aplicado)** | −0,102% | **−21,65%** | **212** |
| familia numérica | +0,223% | −8,79% | **39,4** |
| la racha (memoria de byte) | +0,870% | **−27,23%** | 31,3 |
| órdenes clásicos | +0,069% | −2,15% | 31,2 |
| racimo fila | +0,588% | −14,71% | 25,0 |
| anidamiento | +0,268% | −4,39% | 16,4 |
| familia de texto | +0,168% | −1,55% | 9,2 |
| calibración a 3 etapas | +0,398% | −3,28% | 8,2 |
| calibración a 1 etapa | +1,090% | −7,13% | 6,5 |
| indirectos | +1,011% | −4,37% | 4,3 |
| match-disp | +0,697% | −0,69% | 1,0 |
| DMC | +0,574% | +0,96% | — |

⚠️ **La columna de ciclos solo vale para los modelos de TABLA (0–35).** El DMC y los `match` son
**derivados**: apagarlos por máscara les quita la voz ante el mezclador pero **su cómputo sigue
ejecutándose**, tal como avisa el propio código. Por eso salen en ~0% de reloj, y por eso su fila
no dice lo que parece decir.

🔑 **La mejor palanca del compresor ya estaba aplicada**: apagar «los tres distintos» (v6) devuelve
212 puntos de reloj por punto de ratio. Nada de lo que queda se le acerca.

### Las combinaciones — no son aditivas, y por eso hay que medirlas

| combinación | bytes | texto5 | no-texto3 | ciclos |
|---|--:|--:|--:|--:|
| racha + fila | +1,553% | +1,635% | +1,374% | **−33,78%** |
| perfil 2 de hoy | +2,561% | +3,398% | +0,739% | −24,65% |
| perfil 2 sin racha | +3,153% | +3,808% | +1,730% | **−38,17%** |

Racha y fila por separado suman −41,9% de ciclos; juntas dan **−33,8%**. Se solapan porque las dos
quitan entradas del mismo mezclador. **Sumar palancas medidas por separado habría sobreestimado la
ganancia en ocho puntos.**

### Y de aquí sale la conclusión que ordena todo el trabajo

**Tocando solo el modelo, el techo es ~1,6× pagando ~3% de ratio.** El listón económico del brief
—**8×** con ≤1,0% de ratio— **no se puede alcanzar por la vía (b)**: ni gastando tres veces el
presupuesto de ratio se llega a dos veces la velocidad.

Así que **el 8× tiene que salir casi todo de la vía (a)**, la que no cuesta un byte:

| de dónde | factor esperado |
|---|--:|
| A4 · reparto real de los 12 núcleos (hoy 1,7 usados) | hasta ~3× |
| A1+A2 · `target-cpu` y PGO | 1,1–1,3× |
| C3 · compactar el mezclador (160 → ~56 carriles) y quitar las iteraciones muertas | 1,05–1,1× |
| menú (racha + fila) si el usuario lo elige | 1,5× |

Es alcanzable pero **apretado, y depende críticamente de A4**. Esto es lo que había que saber antes
de escribir código, y ahora está medido en vez de supuesto.

### Dos modelos que sobran de verdad (confirmado en las ocho)

| modelo | bytes al apagarlo |
|---|--:|
| 15 · pareja-palabra | **−0,004%** |
| 30 · txt:trigrama | **−0,002%** |

Apagarlos **mejora** el archivo. Hoy ya los apaga `PODA_V2` en bloques ≥8 MiB, así que el hueco
está en los pequeños. Extenderles la poda a todos los tamaños **baja bytes** ⇒ va al **defecto**
(versión de modelo nueva), no al menú.

---

## Estado

- **Fase 0**: cerrada, gate entregado.
- **Lote de bytes**: **cerrado y válido** — 566 filas, cero fallos, control byte a byte idéntico al
  abrir y al cerrar, mismo `bin_sha` en las dos puntas.
- **Precio en ciclos**: cerrado, 33 medidas en el libro mayor.
- **C1**: falsado en su forma original (gather). Rediseño pendiente de decisión.
- **Fase 1**: preparada y **sin lanzar** — recompila, y no se recompila con medidas en vuelo.
  Es ahora la prioridad, porque la frontera dice que el 8× vive ahí.

---

# FASE 1 — el reloj gratis, MEDIDO (04-08)

Todas las variantes pasaron el mismo control antes de cronometrar: **los cinco sha del códec
idénticos**. Ninguna que cambiara un byte llegó a medirse — sería cronometrar otro compresor.

## A1 · compilación (n=3, vueltas alternadas)

| variante | codigo c/B | sql c/B | media | qué exige |
|---|--:|--:|--:|---|
| base | 29.593 (±0,1%) | 30.388 (±0,6%) | — | nada |
| **`x86-64-v3`** | 29.399 (±0,6%) | 29.568 (±0,2%) | **−1,68%** | Intel 2013+ / AMD 2015+ (psABI) |
| **`znver2`** | 28.930 (±0,5%) | 29.339 (±0,6%) | **−2,84%** | ⚠️ AMD Zen 2 2019+, **solo AMD** |

🔑 **La primera pasada, con n=1, descartó `x86-64-v3` con +0,63%** — y después se demostró que n=1
daba −2,2% y +3,5% **sobre el mismo binario**. Un negativo obtenido con un instrumento que luego se
prueba inválido no es un negativo: es una medida que no se hizo. Con n=3 alternado, gana.

**Recomendación**: `x86-64-v3` en el binario que se distribuye (requisito de una línea, cubre una
década) y `znver2` en el build del servidor, donde el 2,84% se cobra entero.

## A2 · PGO — RECHAZADO

+0,57% sobre base y −3,18% en `pgo+znver2`, o sea **indistinguible de `znver2` solo**. El falsador
era «<2% de mejora» y no llega. Y se entiende: con **0,65% de fallos de rama**, el predictor del
procesador ya acierta casi siempre — no queda nada que enseñarle con un perfil.

## A3 · BOLT — no se mide

`llvm-bolt` no está en la máquina. Se dice en vez de fingirlo.

## A4 · los núcleos parados — y NO es vía (a)

| config | reloj | vs defecto | CPU | bytes |
|---|--:|--:|--:|--:|
| 1 hilo | 953,9 s | 0,61× | 99% | — |
| defecto (64 MiB, 4 hilos) | 581,9 s | — | 167% | — |
| 8 hilos | **581,9 s** | **1,00×** | 167% | — |
| **`--solid=32`** | 330,3 s | **1,76×** | 312% | **+1,427%** |
| `--no-solid` | 323,5 s | 1,80× | 316% | +1,427% |

🔑 **El corpus da DOS frames** (`vcompresor info`), así que el techo de CPU es 200% y ya vamos al
84%. **Ocho hilos dan exactamente el mismo tiempo que cuatro**: no hay trabajo que repartir. Para
usar más núcleos hacen falta frames más pequeños, que es la perilla del ratio ⇒ **A4 es vía (b)**.

⚠️ **`--no-solid` es INERTE con `--max`**: da los mismos 4 frames y los mismos bytes que
`--solid=32`. El parser lo traduce a `solido = 0`, pero en modo máximo el carril de texto impone su
propio tamaño. Otra perilla que no hace lo que su nombre promete.

## El DMC — hipótesis falsada

Se sospechaba que valía ~1,7× (el V0 dice que su tramo dobló las instrucciones; la frontera decía
~0% porque **la máscara quita la voz pero no el cómputo**). Se parcheó `vc-cm` con una perilla de
banco que salta `dmc.p()` y `dmc.update()` — con dos controles: con la perilla apagada los sha
intactos, y con ella puesta el archivo marcado 254 (ilegible a propósito).

    con DMC  29.886 / 30.163 c/B     sin DMC  29.230 / 29.600 c/B
    ciclos −2,03%  ·  bytes +0,807% (codigo), +2,425% (sql)

**Devuelve el 2%, no el 70%.** ⚠️ Y el escalón +122% del V0 **sigue sin explicarse**: la racha da
1,37× y el DMC 1,02×. Queda hueco sin atribuir — cabo suelto, no caso cerrado.

# El balance del titular

| paso | reloj | bytes acumulados |
|---|--:|--:|
| defecto hoy | 597,6 s | — |
| `--solid=32` | 330,3 s | +1,43% |
| `znver2` | ~321 s | +1,43% |
| racha + fila (menú) | **~225 s** | **~+3,0%** |
| *zpaq -m5* | *133,3 s* | *(nos saca −14,4% en bytes)* |

**El titular no cae con lo medido**: de 4,48× más lentos pasamos a **1,7×**, manteniendo −14% de
bytes. Es 2,65× real y verificado, no la igualdad. Lo que queda es caro: C3 (compactar el mezclador
de 160 carriles a ~56), C2 (puerta dinámica por bit) y C1 rehecho con estructura de arreglos.
