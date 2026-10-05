# F4 — por qué son más rápidos, leído en su código, y el plan para dejar de serlo

Encargo del dueño (03-08-2026): *«¿cómo podemos ser más rápidos? tenemos el código de todos. Léelos
todos y dime por qué son más rápidos, luego haces un plan de banco de pruebas para probar y
aplicar. No es posible que seamos más lentos teniendo el código de todos.»*

Fuentes leídas, todas en la máquina:

| fuente | dónde | qué se leyó |
|---|---|---|
| **zpaq 7.15** | `~/fuentes-rivales/zpaq715` (7.733 + 3.737 líneas) | `Predictor::predict0`, `update0`, `makeConfig`, el JIT, la cola de hilos |
| **zstd 1.5.7** | `~/.cargo/registry/.../zstd-sys-2.0.16+zstd.1.5.7/zstd/lib` | `zstd_lazy.c` (buscador SIMD), `compiler.h` (prefetch), `huf_decompress.c`, `zstdmt_compress.c` |
| **paq8px v216** | `/tmp/paq8px/src` (34.293 líneas) | `Mixer_AVX2`, `Bucket16`, `ContextMap2` |
| **preflate** | `crates/preflate/` (vendorizado) | ya minado el 02-08 |
| **zlib / zlib-ng** | leídos el 02-08 para vdeflate | descarte de 4 bytes, orden de inserción |
| **nosotros** | `crates/vc-cm/src/lib.rs` (2.811 líneas) | `Predictor::p()`, `fija_cubos`, `busca_cubo`, mezclador |

## El resultado, y no es el que yo esperaba

**No nos ganan por técnica de bajo nivel. Las tres técnicas que zpaq usa para ir rápido YA LAS
TENEMOS, y una de ellas la tenemos mejor.** Nos ganan porque **hacemos diez veces más preguntas
por bit**.

Medido hoy, misma máquina, mismo corpus de código (136.415.253 B):

| | zpaq ‑m5 | **nosotros `--max`** | paq8px ‑8 |
|---|---|---|---|
| entradas al mezclador **por bit** | **13–17** | **159** | ~500 |
| ciclos por byte | ~6.100 | **36.535** | — |
| **ciclos por entrada y por bit** | ~51 | **28,7** | — |
| MB/s de reloj | 0,98 | 0,18 | 0,008 |
| RAM pico | 1,68 GB | 3,09 GB | 2,41 GB |

**Por unidad de trabajo somos 1,8× más eficientes que zpaq.** Su cadena vale ~51 ciclos por
componente y bit; la nuestra 28,7. Somos 5,9× más lentos porque ejecutamos 159 entradas donde
ellos ejecutan 15.

Los 159 salen de la cuenta de `crates/vc-cm/src/lib.rs`: `NTAB=36` modelos **× 4 vistas cada uno**
(el estado del cubo, la racha, «los tres distintos» y el determinista → `IRACHA`, `ITRES`, `IDET`)
= 144, más 7 de la cadena ISSE, el DMC, 5 modelos de coincidencias y la salida de la cadena. Y esas
159 entradas pasan por **`NMIX=4` mezcladores**, o sea ~636 multiplicaciones para predecir y otras
~636 para actualizar los pesos, **por cada bit**.

Los 13–17 de zpaq salen de `makeConfig` (`libzpaq.cpp:7646`): el nivel 5 monta la cadena
`w1i1,c256ci1,1,1,1,1,1,2a` — un modelo de palabra, un CM, un ICM, siete ISSE, un match — más
como mucho dos modelos periódicos, y **un solo mezclador**.

---

## 🔑 CORRECCIÓN DE LIBRO MAYOR (10-08, F6·Q2) — el 1,8× era la división mal hecha

**Valor viejo**: la tabla de arriba y el párrafo de encima decían *«por unidad de trabajo somos
1,8× más eficientes que zpaq: 28,7 ciclos por entrada y bit contra sus ~51»*, dividiendo
`36.535 ciclos/byte ÷ 8 ÷ 159 entradas`.

**Por qué estaba mal**: los 159 son `NCTX` — las entradas *configuradas* en el mezclador 0
(confirmado leyendo `crates/vc-cm/src/lib.rs`: `NTAB=36`, `IISSE_INT=4·36+9=153`, `NISSE_INT=6`,
`159` exacto) — no las que aportan en ejecución. `docs/f5-velocidad-max.md` («El hallazgo de
velocidad: el mezclador paga 160 carriles para 53 que aportan») ya había medido que en ejecución
(v8, bloque ≥8 MiB) sólo ~53 tienen peso no nulo. Nadie volvió a dividir con ese número.
Instrumentado hoy con `VC_CENSO=1` sobre dos corpus reales (`codigo.10m`, `sql.10m`, `bin_sha
a5f01756`, commit `1d7f55d`), contando filas con `|w| != 0` en las 159: **53 activas en las dos
corridas**. `VC_CENSO` imprime por stderr (`censo()`, `crates/vc-cm/src/lib.rs:2464`) y
`mide.sh:179` sólo captura el stdout del comando, así que este censo no pasa por `mide.sh` ni por
`bench/historia.jsonl` (que sólo tiene las filas `CAMPO:` de ciclos/tiempo, no el censo) — la
salida cruda de las dos corridas queda commiteada en `bench/registros/censo/`.

**Valor nuevo** (medido hoy, mismo instrumento en las dos puntas — `CAMPO:`, nunca `RIVALES:`):

| | `codigo.10m` | `sql.10m` |
|---|--:|--:|
| nosotros c/B (mediana 3 corridas) | 40.122,8 | 37.118,0 |
| zpaq -m5 c/B (mediana 3 corridas) | 8.697,6 | 11.605,1 |
| ratio total (c/B) | **4,613×** | **3,198×** |
| entradas activas nosotros (censo) | 53 | 53 |
| entradas de zpaq (medidas: `makeConfig` real sobre el fichero, no leídas fijas) | 18 | 26 |
| **ratio crudo por entrada y bit** | **1,567× PEOR** | **1,569× PEOR** |
| ratio aislado a modelos (83,7%, `bench/camino-predecir.tsv`) | **1,311× PEOR** | **1,313× PEOR** |

Las dos lecturas — la cruda y la aislada a la parte de modelos — dicen lo mismo en las dos
muestras: **somos ~1,3×–1,6× PEORES por entrada y bit, no 1,8× mejores.** La lectura cruda mezcla
coste fijo (codificador aritmético, búsqueda de cubo) con el coste por entrada, así que exagera la
distancia; la aislada a modelos lo quita y aun así queda peor — la conclusión no depende de cuál se
use. (Con la fracción incorrecta 63,9% que usó un intento de medida anterior de este mismo encargo,
la lectura aislada casi empataba con zpaq — 63,9% no está en ningún fichero fuente del repo; el
83,7% sí, en la cabecera de `bench/camino-predecir.tsv`.) El número de zpaq tampoco era el fijo 15
del código: contando en ejecución con la propia `makeConfig()` de `libzpaq.cpp` sobre el fichero
real salen 18 y 26 según el corpus — zpaq también decide en ejecución, no sólo nosotros. La sonda
de sólo lectura que los cuenta y su salida cruda quedan commiteadas en
`bench/sondas/zprobe-ncomp/`: `makeConfig()` emite 23 componentes en `codigo.10m` y 31 en
`sql.10m`, menos los 5 de la etapa de mezcla (`mix`/`mix2`/`sse`) = 18 y 26. La fuente de zpaq
(`~/fuentes-rivales/zpaq715`) no se vendorea: la sonda enlaza contra ella y se rehace con las
órdenes de su `README.md`.

Identidad de consistencia (no es una prueba independiente, es álgebra): `(53/N_zpaq) × ratio_pe`
reproduce el ratio total medido — `(53/18)×1,567=4,613×` y `(53/26)×1,569=3,198×` — cuadra con las
dos filas de arriba. El 5,3× que citaba el encargo original como referencia histórica ya estaba
superado por `docs/f5-velocidad-max.md` («No 5,3×: la cifra del brief era de antes de v6+v7» →
4,48× en reloj / 4,72× en ciclos); el 4,613× de hoy en `codigo.10m` es coherente con ese 4,72×.

**Qué decisiones se tomaron sobre el valor viejo**: el 28,7/1,8× cerró la hipótesis 2 de este
documento — *«el bucle caliente nuestro está mejor construido que el suyo, el problema es cuántas
veces se ejecuta»* — y con ella el frente de «bajar el coste POR pregunta» (micro-optimizar cada
entrada del mezclador). Ese frente **no estaba agotado: estaba cerrado por una división mal
hecha.** Cada entrada ya cuesta más que la de zpaq, no menos, así que también hay margen en el
COSTE por pregunta, no sólo en el número de preguntas.

**Consecuencia**: reabre como candidata real la vectorización por estructura de arreglos (eje 3 /
C1 rehecho de `docs/f5-velocidad-max.md`) — hoy descartada sólo como «gather + aritmética AVX2»
porque el gather pierde incluso en el mejor caso (0,94×–1,03×), pero **no** como reorganización de
memoria a estructura de arreglos, que es la vía que C1 dejó pendiente de decisión y que un coste
por entrada más alto que el de zpaq vuelve a justificar medir. No implementado en este encargo:
sólo el número y la recomendación.

---

## Lo que YA tenemos (y por eso no hay nada que copiar aquí)

1. **Un cubo por nibble, no por bit.** zpaq busca el hash sólo cuando `c8==1 || (c8&0xf0)==16`
   (`libzpaq.cpp:1873`), o sea una vez cada cuatro bits; los otros tres leen del mismo cubo.
   Nosotros: `fija_cubos()` se llama exactamente dos veces por byte (`lib.rs:1763`).
2. **Y lo tenemos mejor.** Su `find()` recorre una tabla de dispersión; nuestro `busca_cubo`
   (`lib.rs:1329`) mira **cuatro cubos de 16 bytes = 64 = una línea de caché exacta**, y elige por
   cuánto sabe cada uno. Una sola falta de caché por modelo y nibble.
3. **stretch/squash son tablas** en los dos (`lib.rs:160`).
4. **Aritmética entera y SIMD**: `punto_avx2` y `actualiza_avx2` existen y son bit-idénticos al
   camino escalar. zpaq **no** vectoriza su mezclador: su `MIX` es un bucle escalar
   (`libzpaq.cpp:1911`).

O sea: el bucle caliente nuestro está mejor construido que el suyo. El problema es **cuántas veces
se ejecuta**.

## Lo que SÍ tienen y nosotros no

**1. zpaq mira el dato ANTES de elegir el modelo, y le cuesta una pasada** (`libzpaq.cpp:7654`):
cuenta los huecos entre repeticiones en una tabla de 4.096 enteros, busca el período con mejor
puntuación y **sólo añade modelos periódicos si `score>0.1`**. Nuestro modelo de fila está
**siempre encendido y cuesta el 14% del tiempo**. Nuestra sonda de familias se apagó el 02-08
porque costaba 4,6× el reloj — pero es que la nuestra **comprimía cuatro veces** para preguntar;
la suya sólo cuenta. La idea era buena y la implementación era la cara.

**2. zpaq compila su predictor a x86 en tiempo de ejecución** (`Predictor::assemble_p()`,
`libzpaq.cpp:3813`): la cadena concreta se convierte en código recto, sin `switch`, sin avanzar
punteros de componente. A nosotros no nos aplica tal cual —compilamos antes— pero **el equivalente
sí**: nuestro bucle recorre los 36 modelos con una comprobación de `apagados` dentro
(`lib.rs:1833`), y una versión especializada de la configuración activa se lleva ese coste.

**3. zstd, para el carril rápido, tiene cuatro cosas que vlz no tiene:**
- **Comparación SIMD de 16 etiquetas de golpe** en el buscador por filas: `_mm_cmpeq_epi8`
  (`zstd_lazy.c:996`) y su gemelo NEON (`:1019`).
- **Prefetch explícito** (`common/compiler.h:137-149`).
- **Cuatro flujos Huffman independientes** para que el procesador los solape
  (`huf_decompress.c:306`).
- **1.923 líneas de compresión multihilo** (`zstdmt_compress.c`).

**4. Los doce núcleos están parados.** Hoy, en el mismo corpus: nosotros `cpu_pct 167%`, zpaq
`224 s de CPU / 139,6 s de reloj = 160%`. **Los dos usamos 1,7 de 12 núcleos.** Ninguno de los dos
explota la máquina, y ahí no hay que copiar a nadie: es nuestro `--solid=64` partiendo 136 MB en
dos o tres bloques.

## Y nuestro propio banco ya dice qué preguntas no se pagan

`bench/ablacion.tsv` (3 MB de código; «ratio» = cuánto EMPEORA al quitarlo, «tiempo» = cuánto baja):

| grupo apagado | ratio | tiempo |
|---|---|---|
| **sparse de paso** | **−0,34% (MEJORA)** | **−5%** |
| **órdenes (0-5, 8, 12)** | +0,27% | **−14%** |
| fila / separador | +1,28% | **−14%** |
| indirectos | +1,51% | −6% |
| dmc | +1,00% | −4% |
| palabra | +1,21% | −3% |
| forma del texto | +4,34% | −1% |
| coincidencias | +6,18% | −3% |

Quitando **sparse + órdenes** se va un **19% del tiempo por 0,27% de bytes** — y el sparse los
devuelve. Esa tabla es del 29-07 y hay que re-medirla con el binario de hoy: es el experimento V1.

---

# EL PLAN DE BANCO

Reglas que valen para los ocho experimentos, y no son burocracia:

- **Se mide en CICLOS, no en reloj.** La máquina lleva la flota encima y el reloj varía el doble.
  `mide.sh` ya anota ciclos, IPC, `bin_sha` y `arbol_sha`.
- **Todo cambio pasa `bash bench/idayvuelta.sh` (20/20) y `bash bench/regresion-fase3.sh`** antes de
  contarse. Un compresor rápido que no devuelve el fichero no es un compresor.
- **Nada de guiones nuevos si hay uno que sirve**: `ablacion-arbol.sh`, `racimos.sh`, `mezclador.sh`,
  `hilos-y-memoria.sh`, `escala.sh` y `pruebas.sh` ya existen.
- **Cada experimento declara qué lo falsaría.** Sin eso es una corazonada con decimales.

### V0 · ¿De dónde salieron los 595 segundos? — PRIMERO, y bloquea a los demás

El 28-07 el corpus costaba 145 s; hoy 740,8 s. zpaq da hoy 139,6 s contra 132,8 s de entonces
(+5%), así que **la máquina no lo explica**. Antes de optimizar hay que saber qué se compró.

- **Cómo**: `git checkout` de ~10 commits entre `fa1f379` y `71b777d`, `cargo build --release`, y
  una corrida de `corpus/codigo --max` sobre un **corte fijo de 16 MB** (el árbol entero son 12 min
  por punto: no cabe). `mide.sh -e "V0 regresion de reloj"`.
- **Control**: el `bin_sha` distinto en cada punto y el mismo corte de entrada. Se compara
  **ciclos/byte**, que no dependen de la carga.
- **Qué lo falsaría**: que los ciclos/byte salgan planos. Entonces el 740,8 s es un artefacto de
  medida y el que hay que arreglar es el instrumento.
- **Coste**: ~40 min. **Esperado**: dos o tres commits cargando casi todo el aumento.

### V1 · La frontera: ciclos comprados por byte ahorrado, modelo a modelo

- **Cómo**: `VC_APAGA=k` para los 36 modelos, uno a uno, con `ablacion-arbol.sh` sobre una muestra
  fija; se anotan bytes y **ciclos**. Salida: una tabla ordenada por *ciclos por byte ahorrado*.
- **Control**: la máscara vacía (nada apagado) medida al principio y al final de la tanda; si las
  dos no coinciden byte a byte, la tanda no vale.
- **Qué lo falsaría**: que todos los modelos estén por encima de la línea. Entonces no hay nada que
  podar y la velocidad tiene que salir de V3/V4.
- **Coste**: ~45 min. **Esperado**: la ablación vieja dice 19% del tiempo por 0,27% de bytes.

### V2 · Las cuatro vistas por modelo: ¿pagan las cuatro? — el experimento más grande

144 de las 159 entradas son **cuatro vistas del mismo modelo**. Las perillas ya existen
(`RACHA_ON`, `TRES_ON`, `VISTAS_EXTRA`).

- **Cómo**: apagar cada vista **en todos los modelos a la vez** y medir bytes y ciclos. Cuatro
  corridas más el control.
- **Control**: con las cuatro encendidas tiene que salir el archivo de hoy byte a byte.
- **Qué lo falsaría**: que cada vista aporte más que su coste. Sería el mejor resultado posible:
  significaría que el gasto está justificado y que hay que ir a por el paralelismo (V4).
- **Esperado**: `ITRES` cuesta dos lecturas de `memo` y una búsqueda en `tres_sm` por modelo y bit;
  si aporta menos del 0,5% se lleva 36 entradas de golpe.

### V3 · Cuatro mezcladores de 160 pesos: ¿hacen falta cuatro?

- **Cómo**: `NMIX` a 4/2/1 y la segunda capa dentro/fuera, con `mezclador.sh` (⚠️ hoy sólo corre
  dos tipos: `TIPOS="codigo sql"` — hay que ampliarlo, es una variable).
- **Control**: los pesos son deterministas; misma configuración debe dar el mismo archivo.
- **Qué lo falsaría**: que quitar un mezclador cueste más ratio del que ahorra en ciclos.

### V4 · Doce núcleos, 1,7 usados — la ganancia de reloj sin tocar el modelo

- **Cómo**: barrido de `--solid` (64/32/16/8 MiB) midiendo **bytes, reloj y ciclos**. Menos sólido =
  más bloques = más hilos = peor ratio. Ya hay `hilos-y-memoria.sh` y `frames.sh`.
- **Control**: el ratio a `--solid=64` tiene que reproducir el de hoy.
- **Qué lo falsaría**: que el reloj no baje al partir. Significaría que el cuello no es el reparto
  sino la memoria compartida, y entonces la respuesta es otra.
- **Esperado**: de 1,7 a 6-8 núcleos ⇒ reloj entre 3× y 5× mejor por un ratio medible y elegible
  con `--price`.

### V5 · Elegir el modelo mirando el dato (lo de zpaq), pero barato

- **Cómo**: una pasada O(n) por bloque contando huecos entre repeticiones (su tabla de 4.096) y
  encender **fila/periódicos** sólo si hay período con puntuación. Es su
  `libzpaq.cpp:7654-7680` traducido.
- **Control**: en un bloque sin período, la salida tiene que ser idéntica a la de hoy con `fila`
  apagado; en uno con período, idéntica a la de hoy.
- **Qué lo falsaría**: que la pasada cueste más que el 14% que ahorra, o que acierte poco. Se mide
  el acierto contra la ablación de V1: ¿enciende `fila` justo donde `fila` aporta?

### V6 · El bucle especializado (el equivalente a su JIT)

- **Cómo**: monomorfizar el recorrido de los 36 modelos para la configuración activa, sin la
  comprobación de `apagados` por modelo y por bit, y con las vistas apagadas eliminadas en
  compilación. Se mide **instrucciones por bit** antes y después (hoy: 10.246).
- **Control**: salida byte a byte idéntica, y `determinismo x86/ARM` 6/6.
- **Qué lo falsaría**: que las instrucciones no bajen. El compilador puede estar sacando ya la
  comprobación del bucle.

### V7 · El carril rápido: vlz contra zstd, y las cuatro técnicas de zstd

- **Cómo**: (a) medir **por fin** el ratio y la velocidad de vlz contra `zstd -1..-19` sobre el
  banco — hoy `techos.tsv` dice «SIN MEDIR» y es la única incógnita que bloquea la independencia;
  (b) con la medida delante, aplicar en orden: comparación SIMD de etiquetas, prefetch, y flujos
  de entropía independientes.
- **Control**: ida y vuelta byte a byte en cada paso.
- **Qué lo falsaría**: que vlz no se acerque a zstd ni con las tres técnicas. Entonces la respuesta
  honesta es que el carril rápido se queda con zstd y se dice.

## El orden, y por qué

**V0 → V4 → V1 → V2 → V3 → V5 → V6 → V7.**

V0 primero porque optimizar sin saber qué se rompió es pagar dos veces. V4 segundo porque es la
única ganancia grande **que no toca el modelo** —no arriesga ni un byte de ratio que no elijamos—.
V1 y V2 después porque son los que quitan trabajo de verdad, y necesitan la frontera medida para
no cortar a ciegas. V6 y V7 al final porque son código nuevo, y el código nuevo se escribe cuando
ya sabes qué tiene que hacer.

**La frase que resume el hallazgo, para no perderla:** *no somos lentos por no tener su código;
somos lentos por hacer 159 preguntas donde ellos hacen 15, y nuestro propio banco ya dice que
varias de esas preguntas no se pagan.*
