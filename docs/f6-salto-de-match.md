# F6 · Q6 — el salto de match por byte

Encargo del dueño (17-08-2026). La frontera de F5 —«tocando solo el modelo, el techo es ~1,6x
pagando ~3% de ratio»— está bien medida, pero mide **ablación uniforme**: quitar un modelo EN
TODAS PARTES. Nunca se midió lo contrario: **quitarlos TODOS EN ALGUNAS PARTES**.

> Cuando un modelo de coincidencia lleva una racha larga confirmada, codificar el BYTE ENTERO con
> el match más una etapa SSE, y saltarse el resto del conjunto de modelos.

Es la idea de C2 (puerta dinámica) en el grano correcto: **una rama por byte** guiada por el
estado del match, no una rama por modelo y por bit guiada por los pesos del mezclador. Y es
determinista — el descompresor tiene exactamente la misma racha y toma la misma decisión, así que
**no hace falta señalizar nada en el archivo**.

**Falsadores declarados antes de correr** (basta que se cumpla uno):

1. que la fracción de bytes bajo coincidencia larga sea tan baja que el ahorro máximo **no llegue
   a 1,2x**;
2. que al aplicarlo el **coste en bytes pase del 1%** en cualquiera de las ocho muestras.

🔑 **Resultado, con las DOS tandas ya corridas (§7).** El **(1) NO se cumple**: el salto es real y
mide **1,700x** de ciclos en el corpus, muy por encima del 1,2x. El **(2) SÍ se cumple, y por
goleada**: **+3,340%** de bytes en el corpus, **cinco de las ocho** muestras por encima del 1% y
`logs.10m` **casi al doble (+98,0%)**. Basta que se cumpla uno ⇒ **el encargo cierra en NEGATIVO
para `--max`**, el defecto de escritura se queda en la **v8** (`MODELO = 8`) y la v9 queda en la
rama **como evidencia**, leíble y seleccionable con `VC_MODELO=9`. El detalle está en §8.

**Orden de trabajo impuesto por el encargo: MEDIR ANTES DE ESCRIBIR.** La Fase 1 es un gate. Si
la cobertura no da para 1,2x, el encargo cierra en negativo sin escribir una línea de modelo.

---

## 0. Veredicto del gate

> **EL GATE PASA.** Sobre las ocho muestras, el **54,00%** de los bytes se empieza a codificar con
> una racha de match confirmada de **≥16**; el 45,34% con ≥32 y el 35,61% con ≥64. La cota de
> Amdahl es **2,174x / 1,829x / 1,553x**. El falsador (1) pedía que no llegara a 1,2x y **no se
> cumple en ninguno de los tres umbrales**.

La tabla completa está en §2 y en `bench/f6q6-cobertura.tsv`. El matiz que la tabla obliga a decir
—y que es la lección de los órdenes clásicos otra vez— está en §3: **la palanca no existe en
prosa ni en binario ya denso**, y sí existe, muy grande, en código, logs, SQL y CSV/JSON.

---

## 1. El corpus: siete muestras históricas y una reconstruida (se dice, no se finge)

El juego canónico está declarado en `bench/muestras.tsv`: `texto5` (codigo, csvjson, enwik8, logs,
sql) + `no-texto3` (bin, pdf, png). El barrido de `/tmp` de este servidor —el mismo que documentan
`docs/f6-arranque-en-frio.md` §0 y `docs/f6-q4-c3-mezclador-compacto.md`— se llevó parte del
corpus. Lo que hay hoy, verificado en disco:

| muestra | juego | fichero | bytes | sha256 (16) | estado |
|---|---|---|--:|---|---|
| codigo.10m | texto5 | `/tmp/compresor-f1/samples/codigo.10m` | 10.485.760 | `132c06fedd3d6c29` | histórica |
| enwik8.10m | texto5 | `/tmp/compresor-f1/samples/enwik8.10m` | 10.485.760 | `5d7a724fcfbea8f6` | histórica |
| sql.10m | texto5 | `/tmp/compresor-f1/samples/sql.10m` | 10.485.760 | `a11cd4efaa116b36` | histórica |
| logs.10m | texto5 | `/tmp/x5/logs.10m` | 10.485.760 | `3b5e9832787abc8c` | histórica (fuera de su ruta declarada) |
| **csvjson-r.10m** | texto5-r | armada en este turno | 10.485.760 | `ec9b5d73a7b6f602` | **RECONSTRUIDA** |
| bin.8m | no-texto3 | `/tmp/nt/bin.8m` | 5.313.968 | `f2b9edb9892e76fe` | histórica |
| pdf.8m | no-texto3 | `/tmp/nt/pdf.8m` | 961.642 | `41343d3980017359` | histórica |
| png.8m | no-texto3 | `/tmp/nt/png.8m` | 141.864 | `267e871a9ae1c568` | histórica |

🔑 **Las cuatro de texto que sobreviven están PROBADAS, no supuestas.** `vc-cm c` sobre ellas
reproduce **exactamente** los cuatro sha256 del control de determinismo de F5
(`docs/f5-velocidad-max.md` §«Determinismo cruzado»):

| muestra | bytes de salida | sha256 (16) medido hoy | sha256 (16) en F5 | |
|---|--:|---|---|---|
| codigo | 787.080 | `af18464ffb482fa0` | `af18464ffb482fa0` | ✅ |
| sql | 225.810 | `ad5ece6052c087b9` | `ad5ece6052c087b9` | ✅ |
| enwik8 | 2.088.518 | `9af1e2cb5309e4d2` | `9af1e2cb5309e4d2` | ✅ |
| logs | 48.560 | `70497d35868d3a59` | `70497d35868d3a59` | ✅ |

O sea que no son ficheros parecidos con el mismo nombre: **son las muestras de F5**, y de paso
esto vuelve a validar el control de bytes entero.

⚠️ **`csvjson-r.10m` NO es la `csvjson.10m` histórica, y no se la hace pasar por ella.** Aquella no
sobrevivió y **no hay generador en el repo** que la reconstruya (`bench/trae-muestras.sh` trae los
`.deflate` de preflate-rs, que es otro juego). Se armó una muestra **de la misma clase de dato**
—«estructurado por separadores: volcados y respuestas de API», que es para lo que la declara
`bench/muestras.tsv`— con el método repartido de la casa, uno de cada tres y cortado al tamaño:

```
find . -maxdepth 6 -type f \( -name "*.json" -o -name "*.csv" \) -size +2k \
  | LC_ALL=C sort | awk 'NR%3==1' | xargs cat | head -c 10485760
```

(desde el directorio de trabajo, 3.939 ficheros de 11.817 candidatos.)

**Qué se puede y qué no se puede hacer con ella.** La Fase 1 mide una propiedad **del dato**
—cuánta de su masa vive dentro de rachas largas—, y para eso una muestra de la misma clase
contesta la pregunta. Lo que esta muestra **no** permite es comparar sus bytes contra ninguna fila
histórica del libro mayor: no es la misma entrada. Por eso va con juego propio (`texto5-r`) en
`bench/f6q6-cobertura.tsv` y no se le empareja nada.

---

## 2. El instrumento, y la prueba de que no mueve un byte

`crates/vc-cm/src/lib.rs`, `mod cobertura`, detrás de `--features cobertura` (declarada en
`crates/vc-cm/Cargo.toml`). Solo **lee** `mm_len`/`mm_ptr` y suma en globales; no toca ninguna
predicción, ningún contexto y ninguna tabla.

**La racha se lee AL EMPEZAR EL BYTE** (`cob_foto()`, al final de `set_byte_contexts`), que es el
único instante en el que compresor y descompresor tienen exactamente el mismo estado — o sea, el
instante en el que el salto tendría que decidir. Se cobra al cerrarse el byte, cuando ya se sabe
si el match acertó.

🔑 **La prueba no es el comentario: es el sha256.** Misma entrada, mismo binario salvo la bandera:

| muestra | sha256 sin contador | sha256 con contador | |
|---|---|---|---|
| codigo.10m | `af18464ffb482fa01c7b962542b7d8cd` | `af18464ffb482fa01c7b962542b7d8cd` | ✅ |
| bin.8m | `d3a5154ec63b7205f37b9a96224264d1` | `d3a5154ec63b7205f37b9a96224264d1` | ✅ |
| pdf.8m | `a6d79a2a81a0fbc90dccdb44081396be` | `a6d79a2a81a0fbc90dccdb44081396be` | ✅ |
| png.8m | `f7dec0823c34d637bd25f0f315ede7aa` | `f7dec0823c34d637bd25f0f315ede7aa` | ✅ |

El encargo pedía **al menos una** muestra; se hicieron cuatro, una de ellas binaria.

---

## 3. La tabla de cobertura — el entregable de la Fase 1

Fracción de bytes que empieza a codificarse con una racha del match **principal** (el de 6 bytes,
el único que verifica la coincidencia desde la v4) de longitud ≥ umbral. `cota` = 1/(1−f), el
techo del ahorro **si el salto fuera gratis**.

| muestra | bytes | ≥16 | ≥32 | ≥64 | cota ≥16 | cota ≥32 | cota ≥64 |
|---|--:|--:|--:|--:|--:|--:|--:|
| codigo.10m | 10.485.760 | 54,64% | 45,91% | 37,66% | 2,20x | 1,85x | 1,60x |
| csvjson-r.10m | 10.485.760 | 77,67% | 72,71% | 68,22% | 4,48x | 3,66x | 3,15x |
| enwik8.10m | 10.485.760 | **5,71%** | **2,53%** | **0,88%** | 1,06x | 1,03x | 1,01x |
| logs.10m | 10.485.760 | 85,31% | 74,87% | 62,22% | 6,81x | 3,98x | 2,65x |
| sql.10m | 10.485.760 | 65,98% | 50,57% | 27,34% | 2,94x | 2,02x | 1,38x |
| bin.8m | 5.313.968 | 26,74% | 15,14% | 6,73% | 1,36x | 1,18x | 1,07x |
| pdf.8m | 961.642 | **2,07%** | **1,65%** | **1,25%** | 1,02x | 1,02x | 1,01x |
| png.8m | 141.864 | **1,38%** | **0,97%** | **0,41%** | 1,01x | 1,01x | 1,00x |
| **CORPUS (8)** | **58.846.274** | **54,00%** | **45,34%** | **35,61%** | **2,17x** | **1,83x** | **1,55x** |

**Y la cifra que decide si el salto es siquiera defendible: cuando el match cubre, ¿acierta?**
De los bytes cubiertos a ≥32, el match predijo el byte correcto en el **99,20%** (codigo), 99,70%
(csvjson-r), 99,18% (logs), 98,27% (sql), 97,09% (bin), 98,88% (pdf), 97,82% (png) y **96,13%**
(enwik8). Saltar el conjunto es apostar a esa cifra; el peor caso del corpus es enwik8, que es
justo donde la cobertura es del 2,53% y por tanto la apuesta casi no se hace.

La columna `cub5` de `bench/f6q6-cobertura.tsv` da la variante generosa —el mejor de los cinco
match— y no cambia el veredicto en ningún sentido: sube la cobertura (57,79% en codigo, 93,43% en
logs a ≥16) pero los otros cuatro match **no verifican** la coincidencia, así que enganchan más y
mienten más. El gate se juzga con el principal, que es el conservador.

---

## 4. El matiz que la tabla obliga a decir

El encargo insistía en medir sobre las OCHO y no sobre las cinco de texto, y tenía razón, aunque
el resultado cae **al revés** de lo que avisaba:

- **Con solo las cinco de texto delante**, la media saldría ~57,9% a ≥16 y la conclusión sería
  «palanca enorme, adelante». Es casi verdad, pero esconde que **enwik8 —prosa— tiene un 5,71%** y
  la palanca ahí no existe.
- **En no-texto, la respuesta se parte en dos**: `bin` tiene un 26,74% (cota 1,36x, palanca real)
  y `pdf`/`png` tienen 2,07% y 1,38% (cota 1,02x, no hay nada que ganar). Son datos ya densos: no
  se repiten, así que no hay rachas.

🔑 **Esto NO invalida el gate, y el motivo es que la palanca es ADAPTATIVA, no uniforme.** Ahí está
la diferencia con toda la Fase 5. Una ablación uniforme que no sirve en enwik8 hay que decidir si
entra o no entra para todo el mundo; **un salto guiado por la racha del match no se dispara donde
no hay racha**. En enwik8, pdf y png el salto sencillamente casi nunca ocurre: ni ahorra ni
cuesta. El riesgo en esas tres no es perder velocidad, es perder BYTES por los pocos saltos que sí
se den — y eso lo mide el falsador (2), no éste.

---

## 5. Cómo se reproduce

```
cargo build --release -p vc-cm --features cobertura
./target/release/vc-cm c <muestra> /dev/null      # la tabla sale por stderr, en TSV
```

Los ocho ficheros de salida crudos de esta tanda están resumidos en `bench/f6q6-cobertura.tsv`
(una fila por muestra y umbral, más las tres filas `CORPUS8`).

⚠️ **Esta tanda es de BYTES y de CONTADORES, no de reloj**: se corrió con 5 procesos a la vez, que
es lícito porque los bytes son deterministas (`bench/comun.sh`). Ninguna de estas cifras entra en
`bench/historia.jsonl`, que empareja por ciclos.

---

## 6. Cómo queda la v9 en disco: implementada, leíble y NO por defecto

La Fase 2 escribió la v9 y la dejó de camino de escritura por defecto. Eso daba por **cumplido el
falsador (2)** —«que el coste en bytes pase del 1% en cualquiera de las ocho muestras»— que **nadie
ha corrido todavía**: el A/B de la Fase 2 no existe en `bench/historia.jsonl`. Y con la constante
que se emitía la contradicción era doble, así que se corrigen las dos cosas antes del banco.

**(a) El umbral es 32, que es un valor MEDIDO.** `UMBRAL_SALTO` estaba en **512**, fuera del
conjunto {16, 32, 64} de `bench/f6q6-cobertura.tsv`: la cobertura a 512 no está en la tabla, así
que ni se sabe cuánto salta ni cuánto cuesta. El propio doc-comment de la constante —y el comentario
del `update`— argumentan por **32** («se compra el 8,7% de cobertura más barato a cambio del tramo
donde el match miente más»). Queda en 32. La v9 no ha salido de la rama, así que redefinirla antes
de publicar es legal y no rompe ningún archivo de nadie.

**(b) El defecto de escritura vuelve a la v8.** Flipar el defecto es una decisión que se toma con
el A/B delante, no un efecto colateral de implementar la palanca. En `crates/vc-cm/src/lib.rs`:

- `MODELO_MAX = 9` — la última versión IMPLEMENTADA. Manda en lo que el binario sabe **leer**, y de
  ella salen **las dos listas**: `MODELOS_LEIBLES` (1..=`MODELO_MAX`) y el reparto de
  `descomprimir`. Siguen derivadas la una de la otra, que es lo que impide que se desincronicen.
- `MODELO = 8` — la versión con la que se **escribe** por defecto. Un `const assert` impide que
  suba por encima de `MODELO_MAX`.

**Cómo se selecciona la v9: con el mecanismo que ya existía, `VC_MODELO=9`** (ver
`modelo_a_escribir`). No es una perilla nueva ni una excepción a «nada que cambie bytes viaja por
variable de entorno»: lo que el descompresor obedece sigue siendo **el byte que viaja en el
archivo**; la variable sólo elige qué número se escribe ahí. Por eso la v9 tiene que ser LEÍBLE
aunque no sea el defecto — si no, un archivo escrito con `VC_MODELO=9` no lo abriría ni el binario
que lo escribió.

**La ida y vuelta, comprobada a mano sobre 1 MiB de `codigo.10m`** (`vc-cm c` / `vc-cm d`):

| caso | byte de modelo | bytes | resultado |
|---|--:|--:|---|
| defecto (sin variable) | 8 | 39.520 | vuelve IDÉNTICO |
| `VC_MODELO=9` | 9 | 44.046 | vuelve IDÉNTICO |
| **v9 leída por un binario PRE-v9** | 9 | — | **falla LIMPIO**: «este archivo usa el modelo v9 y esta versión conoce hasta v8: actualiza vcompresor», rc=3, sin escribir salida |

El caso cruzado no es una simulación: se leyó con un binario `vc-cm` construido antes del commit de
la v9 (`MODELO_MAX` = 8), y el archivo por defecto de hoy **es byte a byte el de aquel binario**
—mismo sha256 `0bd2b3c806be5ede…`—, o sea que devolver el defecto a la v8 restaura exactamente el
codec anterior y no deja un tercer «v8» distinto.

⚠️ **Y lo que esa tabla insinúa, dicho sin fingir que es la medida:** en esa única muestra la v9
cuesta **+11,45%** de bytes, once veces el umbral del falsador (2). Es 1 MiB de una muestra, no el
A/B de las ocho, pero apunta en la misma dirección que el +5,96% que el validador reprodujo sobre
`logs.10m`. **El banco ya habló: §7.** Y confirma la dirección, con la sorpresa de que el coste
real es mucho peor que ese +11,45% justo donde la Fase 1 prometía más.

---

## 7. El A/B de la Fase 2: las ocho muestras, ciclos y bytes

Tanda de este turno (las tres primeras muestras venían de la tanda anterior, mismo binario). **Un
solo sello de binario en las dieciséis filas: `71a4e62e02a7`**, o sea que las dos mitades midieron
EL MISMO PROGRAMA y lo único que cambia es qué número de modelo se escribe. Todas las filas viven
en `bench/historia.jsonl` con `sucio_codigo: false`.

Control `VC_MODELO=8` (v8, sin salto) contra tratamiento `VC_MODELO=9` (v9, `UMBRAL_SALTO = 32`):

| muestra | v8 bytes | v9 bytes | Δ bytes | v8 Gciclos | v9 Gciclos | ciclos v8/v9 | cota ≥32 (§3) |
|---|--:|--:|--:|--:|--:|--:|--:|
| codigo.10m | 787.080 | 815.037 | **+3,552%** | 464,0 | 244,4 | 1,898x | 1,85x |
| csvjson-r.10m | 411.418 | 424.284 | **+3,127%** | 412,8 | 163,8 | 2,521x | 3,66x |
| enwik8.10m | 2.088.518 | 2.095.589 | +0,339% | 517,3 | 512,8 | 1,009x | 1,03x |
| logs.10m | 48.560 | 96.159 | **+98,021%** | 502,6 | 129,9 | 3,868x | 3,98x |
| sql.10m | 225.810 | 286.433 | **+26,847%** | 474,9 | 237,8 | 1,997x | 2,02x |
| bin.8m | 655.784 | 675.008 | **+2,931%** | 289,1 | 244,1 | 1,184x | 1,18x |
| pdf.8m | 903.965 | 904.160 | +0,022% | 54,3 | 60,4 | 0,898x | 1,02x |
| png.8m | 135.628 | 135.658 | +0,022% | 9,5 | 9,3 | 1,020x | 1,01x |
| **CORPUS (8)** | **5.256.763** | **5.432.328** | **+3,340%** | **2.724,4** | **1.602,5** | **1,700x** | **1,83x** |

**La cota de Amdahl de la Fase 1 predijo el RELOJ casi clavado.** Compárense las dos últimas
columnas: 1,90 contra 1,85 en código, 2,00 contra 2,02 en SQL, 3,87 contra 3,98 en logs, 1,18
contra 1,18 en bin. La Fase 1 no se equivocó en lo que medía —la cobertura sí predice el ahorro— y
ese es justo el motivo por el que el negativo de aquí es informativo y no un fallo de puntería.

⚠️ **Los ciclos de `no-texto3` NO son estables, y los bytes sí.** Las tres muestras binarias se
midieron **tres veces** (la 2ª y la 3ª por motivos de etiquetado que se explican en §8), y esas
tres tandas son el mejor control de determinismo del turno:

| muestra | bytes v8/v9 — **las tres tandas** | ciclos v8/v9 — tanda 1 / 2 / 3 |
|---|---|---|
| bin | 655.784 / 675.008 · **idéntico ×3** | 1,184x / 1,096x / 1,055x |
| pdf | 903.965 / 904.160 · **idéntico ×3** | 0,898x / 0,975x / 1,018x |
| png | 135.628 / 135.658 · **idéntico ×3** | 1,020x / 0,907x / 0,978x |

Los bytes salieron **idénticos al byte en las tres**, y la razón de ciclos bailó en las tres
—**discrepando hasta en el SIGNO** en `pdf` y en `png`—. Son muestras pequeñas (`png` son 141 KB,
~6-7 Gciclos, dominados por el arranque) medidas en una máquina con carga. **Por eso la decisión
la toma la columna de BYTES, que es determinista**, y no la de ciclos. Es la misma regla que ya
está escrita en `bench/comun.sh` y la que el commit `03b8f61` incumplió cuando borró 25 mejoras
reales aplicándoles el suelo de ruido del cronómetro.

Las seis filas de la 3ª tanda llevan `sucio: true` con **`sucio_codigo: false`** y el mismo
`arbol_sha` (`b0fc64fea2c7`) que las demás: lo sucio eran `docs/` y `bench/` —este informe y su
fila— y nunca `crates/`. Se dice porque el flag está ahí y no se explica solo.

---

## 8. La decisión: NEGATIVO para `--max`, y por qué

**Se aplica la regla declarada antes de correr, sin reinterpretarla.**

- **Falsador (1) — NO se cumple.** Pedía que el ahorro no llegara a **1,2x**. Llega: **1,700x** en
  el corpus, y 3,87x en el mejor caso. Sobre datos reales, no sobre la cota. **El salto de match
  funciona como acelerador.**
- **Falsador (2) — SÍ se cumple, y con margen enorme.** Pedía que el coste en bytes no pasara del
  **1% en ninguna** de las ocho. Lo pasa en **cinco de las ocho**: logs **+98,0%**, sql **+26,8%**,
  código **+3,6%**, csvjson-r **+3,1%**, bin **+2,9%**. El agregado del corpus es **+3,340%**,
  **once veces** el objetivo de `<= 0,3%` que fijó la orden del 03-08.

Basta con que se cumpla uno ⇒ **`--max` no adopta el salto**. `MODELO = 8` se queda como defecto de
escritura y **no ha habido que tocar el código para cerrar**: el defecto ya volvió a la v8 en el
commit `da77797`, antes de que existiera este A/B, precisamente para no dar por bueno un falsador
que nadie había corrido.

**Y la lección, que es más interesante que el veredicto: el coste en bytes es MÁXIMO justo donde
la cobertura es máxima.** No es una casualidad ni una regresión: en `logs.10m` el 85% de los bytes
vive bajo racha larga, así que el salto se dispara casi siempre — y el v8 comprimía esa muestra a
**0,037 bpb**, un régimen al que el match más UNA etapa SSE sencillamente no llega. Saltarse el
conjunto ahorra el 74% de los ciclos y **cuesta el doble de archivo**. Donde la cobertura es baja
(enwik8, pdf, png) el salto casi no ocurre: ni ahorra ni cuesta, tal como anticipaba §4. Es decir,
**la adaptatividad funcionó exactamente como se diseñó; lo que falla es el precio del sustituto**,
no su puntería. Ese es el hallazgo que sobrevive a este negativo.

**Sobre `--price`: el mecanismo EXISTE, y aun así la palanca no va ahí.** Está leído en el código,
no supuesto: `crates/vc-cli/src/main.rs:200` acepta `--price|--precio`, y desde 2026-07-31 ya no
elige solo el nivel de zstd sino que **puede APAGAR carriles** (`main.rs:433`, `vc-core/src/lib.rs:1452`).
La regla del encargo manda la palanca a `--price` cuando el coste queda **entre 0,3% y 1%** —el
tramo en que es un intercambio que decide el disco del usuario—. Aquí no estamos en ese tramo:
con `logs` al **+98%** no hay precio de disco que compre ese cambio, porque no es un ajuste fino
sino un codec peor para esa clase de dato. **No se implementa ninguna perilla nueva** y no se
recomienda llevarlo a `--price` en esta forma.

**Qué queda vivo (sin medir, y dicho como hipótesis).** El negativo mata *esta* forma del salto —un
umbral fijo de racha—, no la idea. Lo que la tabla señala es que la decisión de saltar debería
mirar **cuánto vale el conjunto en ese byte**, no solo cuánta racha lleva el match: saltar cuando
el match ya es tan bueno como el conjunto, y no saltar cuando el conjunto está sacando 0,037 bpb.
Eso es otra palanca, no está implementada y no se decide aquí.

### Cómo queda todo en disco

| pieza | estado |
|---|---|
| `MODELO` (defecto de escritura) | **8** — la v8, sin salto |
| `MODELO_MAX` (lo que sabe leer) | **9** — la v9 es LEÍBLE |
| `MODELOS_LEIBLES` y el reparto de `descomprimir` | **derivados de `MODELO_MAX`**, no pueden desincronizarse |
| `UMBRAL_SALTO` | 32, el valor medido de §3 (era 512, fuera de la tabla) |
| cómo se pide la v9 | `VC_MODELO=9`, mecanismo que ya existía |

**La fila del libro de decisiones, y la racha de cuatro decisiones `SIN MEDIDA` que aquí se rompe.**
Se anota en `bench/decisiones.tsv` con veredicto `RECHAZADO`, en **dos filas** porque se midió
sobre dos juegos y ésa es la unidad de la tabla.

La de **`no-texto3` la deriva `bench/decide.py` él solo: `OK`, 3/3 muestras, `+1,147%` de bytes y
`-4,4%` de ciclos** (`nt-bin +2,931% / -5,2% · nt-pdf +0,022% / -1,7% · nt-png +0,022% / +2,2%`).
Es la primera fila derivada desde `f6q3-cebado-entre-frames`, y costó **dos** correcciones, las dos
por la misma causa —hacer que la medida encaje en la FORMA que el instrumento sabe leer—:

1. **Una sola etiqueta**, con el `config` distinguiendo las condiciones. `decide.py` empareja
   control contra tratamiento **dentro de UNA etiqueta** (`decide.py:256`), y la tanda original
   nació con **dos** (`CONTROL v8…` y `TRATAMIENTO v9…`). Es exactamente la lección que la fila
   `f6q3-cebado-entre-frames` dejó anotada para no repetir — y se había repetido.
2. **El nombre de muestra de `bench/muestras.tsv`**, que es lo que faltaba y no estaba escrito en
   ninguna parte: el juego `no-texto3` se llama `nt-bin` / `nt-pdf` / `nt-png`, **no**
   `bin.8m` / `pdf.8m` / `png.8m`. `decide.py` cruza los nombres del juego **exactamente**, así
   que con el nombre de fichero la fila seguía saliendo `SIN MEDIDA` aun con la etiqueta ya
   arreglada.

La fila de **texto5 sale `SIN MEDIDA` (0/5), y se declara en vez de disimularlo**: sus diez filas
nacieron con las dos etiquetas y re-medirlas son ~35 minutos de máquina que **no cambiarían ni un
byte del veredicto** —los bytes ya están medidos, son deterministas y están en el libro mayor con
un solo `bin_sha`—. Se arregló donde era barato (las tres binarias, minutos) y no donde era caro.
⚠️ Y aunque se re-midiera, `texto5` saldría `INCOMPLETA`: la `csvjson.10m` canónica no sobrevivió
al barrido de `/tmp` y lo que se midió es la reconstrucción declarada `csvjson-r.10m` (§1).

---

## 9. Los controles, con lo que salió de verdad

**`cargo test --release` — 73 de unidad en verde, 6 doctests en rojo, y los 6 son PREVIOS.**

| suite | resultado |
|---|---|
| unidad (`vc-core`, `vc-cm`, `vc-cli`, …) | **49 + 16 + 8 = 73 passed, 0 failed** |
| doctests de `vc-cm` | **6 failed** |

⚠️ **Los 6 doctests rojos NO son de este turno y no los toca este cierre.** No se ha modificado ni
una línea de `crates/` (el `git status` de `crates/`, `Cargo.toml` y `Cargo.lock` sale **vacío**),
y `git blame` fecha las seis líneas ofensoras el **02 y el 03 de agosto**, dos semanas antes de que
existiera esta rama: `lib.rs:303` (`a7da7643`), `:676` (`1282e99d`), `:1394` (`6fe65931`), `:3179`
(`d54c6a4d`), `:3200` y `:3211` (`ace755f4`). La causa es la misma en las seis: **tablas de cifras
en prosa dentro de una valla ```` ``` ```` de un doc-comment**, que `rustdoc` intenta compilar como
Rust (`error: expected one of ! or ::, found 987.994`). Se arregla marcando las vallas como
` ```text `. **No se hace aquí a propósito**: es ortogonal a esta decisión y tocarlo metería
`crates/` en un commit cuya propiedad principal es justamente que **no toca `crates/`**. Queda
dicho para que alguien lo arregle a sabiendas, no descubierto por sorpresa.

**`bash bench/idayvuelta.sh` — 14 de 14 en verde, y NO son 20 de 20. Se explica el porqué.**

```
codigo   rapido OK · maximo OK      sql       rapido OK · maximo OK
csvjson  rapido OK · maximo OK      bin       rapido OK · maximo OK
logs     rapido OK · maximo OK      pdfoffice rapido OK · maximo OK
png      rapido OK · maximo OK
                       14 bien · 0 mal · 0 sin dato
```

Sobre el corpus por defecto (`/tmp/corpus-hilos`) el guion sale **`0 bien · 0 mal · 10 sin dato`**:
ese corpus **no existe en este servidor**, se lo llevó el mismo barrido de `/tmp` que documenta §1.
`FIN idayvuelta OK` con cero comprobaciones **no es un verde**, y no se presenta como tal. Lo que se
hizo fue rearmar un corpus con las muestras que **sí** sobreviven (3 MB de cada una, `pdf` y `png`
enteras) y correr el control de verdad sobre él: **7 tipos × 2 modos = 14, todos byte a byte
idénticos a la vuelta**. Los tres que faltan para 20 —`jpg`, `video`, `dup`— **no tienen muestra en
este servidor**; no se inventa un verde por ellos.

🔑 **Los dos controles cruzan lo que importa para este cierre**: `maximo` (`--max`) devuelve el
fichero **idéntico** en los siete tipos con el defecto de escritura **v8**, que es exactamente el
estado que esta decisión deja fijado.
