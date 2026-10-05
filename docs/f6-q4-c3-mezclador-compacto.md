# F6 · Q4 — C3: compactar el mezclador de 160 carriles a 56

Encargo del dueño (17-08-2026): `punto()` y `actualiza_pesos()` recorren `NCTXP = 160` carriles
fijos, por bit y por cada uno de los `NMIX = 4` mezcladores. Las entradas que aportan hoy son
~53; las otras ~107 son **ceros multiplicados por pesos que no se mueven**. Compactar las activas
al principio del vector deja el producto escalar en ~56 carriles, y **el mismo resultado exacto**:
sumar ceros no cambia nada. Vía (a) pura, no cuesta un byte.

**Falsador declarado antes de medir:** que las instrucciones por byte **no bajen**.

⚠️ **El `75.463` del encargo es de ANTES de `a60ffff`, y por eso no cuadra con el 69.332 de aquí.**
El encargo cita «hoy son 75.463 por byte», tomado de `docs/f5-velocidad-max.md:236`, donde está
declarado en **relativo** («que las instrucciones por bit no bajen (hoy 75.463 por byte)»): el
número era el valor *de entonces*, **no un listón absoluto**. En medio entró `a60ffff`
(`target-cpu=x86-64-v3`, AVX2+BMI2+FMA), que explica el hueco del 8,1% contra el 69.332 medido
aquí. Por eso el lado *antes* se **re-deriva** en condiciones idénticas al *después* en vez de
compararse contra el 75.463: es cumplir el falsador como estaba declarado, no sustituirlo.

> **VEREDICTO: NEGATIVO. El falsador se cumplió.** El cambio es exacto —los cinco controles de
> bytes pasan, incluido el sha del árbol completo— pero **cuesta un 5,64% MÁS de instrucciones por
> byte**, no menos. No entra al defecto. Abajo está por qué, que es la parte que sirve para el
> siguiente intento.

---

## 0. Estado de partida: el cambio existía y estaba sin atestar

Este documento se escribe en un turno de **rescate**. La implementación (217 líneas en
`crates/vc-cm/src/lib.rs`) ya existía del intento anterior, commiteada por el auto-commit de
cierre `abbeb42` y empujada a `origin/feature/f6-q4-c3-mezclador-compacto`. Lo que faltaba era
**la atestación**: nadie había corrido los controles duros ni medido el falsador.

Todas las cifras de abajo están medidas en ESTE turno, con el binario de `abbeb42`
(`bin_sha acb3561a814b3645`) contra el de `97ab3b9` (`bin_sha aca51f5ba5002d8c`), compilado en
un worktree aparte. Las dos ramas del A/B salen del mismo `cargo build --release`.

---

## 1. Qué se implementó

### El mapa de índices, en un solo sitio

`st[]` **sigue teniendo 160 ranuras** con el significado de siempre: el censo, `VC_VOLCAR`, la
dispersión y `nombre_ctx()` no se tocan. Lo que cambia es lo que **ve el producto escalar**.

Dos funciones nuevas, y la segunda consume a la primera para que el filtro no viva dos veces:

- `tabla_activos(apagados) -> Vec<usize>` — los modelos de tabla vivos (de los 36, el defecto en
  bloques ≥8 MiB deja 20; los apagan `DUPLICADOS` + `PODA_V2`).
- `mezclador_activos(activos_tab, apagados, racha_on, tres_on, det_on, isse_int_on) -> Vec<usize>`
  — el mapa **carril-compactado → carril-original**, en el mismo orden en que ya vivían dentro de
  `st[]` (directo, racha, tres, determinista, ISSE intermedio).

El resultado se guarda una vez por predictor en `Predictor::new` como `mix_idx`, y su longitud
redondeada a múltiplo de 8 (para el SIMD) como `mix_n`. **`mix_idx` es la única correspondencia
que existe**: el producto escalar, `VC_LOAD_W`, `VC_DUMP_W` y el ancho de `w[]` pasan todos por
ella. Ese era el requisito explícito del encargo —no dos listas paralelas que se desincronicen—
y se cumple.

### Por qué es exacto: el invariante

La propiedad que hace legítimo saltarse un carril es **«excluido ⟹ su `st` es siempre 0»**.
Verificada leyendo el fuente, no supuesta:

| familia | ranuras | quién la escribe | por qué es 0 si se excluye |
|---|---|---|---|
| directo | `0..36` | bucle de `p()` sobre `activos_tab` | un modelo apagado nunca entra al bucle; `st[i]` se queda en su 0 inicial |
| derivados | `36..45` | `p()`, incondicional | `lib.rs:2472` los pone a 0 si `apagados` los tiene apagados — el mismo bit que filtra `mezclador_activos` |
| racha | `45..81` | mismo bucle de `activos_tab` | `= 0` si `!RACHA_ON`; si el modelo está apagado no se escribe nunca |
| tres | `81..117` | mismo bucle | `= 0` si `!tres_on \|\| !RACHA_ON` |
| determinista | `117..153` | mismo bucle | `= 0` si `!VISTAS_EXTRA.1` |
| ISSE int. | `153..159` | `p()` | sólo se escribe `if int_on` |
| relleno | `159` | nadie | 0 desde la construcción |

Nótese que **incluir** un carril que vale 0 es inocuo (suma 0); el único fallo posible sería
**excluir** uno que no valga 0. La tabla cubre esa dirección, que es la que importa.

### Los 16 modelos apagados fuera del recorrido

La segunda mitad del encargo: `fija_cubos()` (dos pasadas), `p()` y `update()` (y el bloque de
racha de `update()`) recorrían `0..NTAB` con un `continue` por cada modelo apagado. Ahora
recorren `self.activos_tab`. Con el defecto en bloques ≥8 MiB son **20 de 36**, o sea 16
iteraciones muertas por modelo, por bit y por sitio.

---

## 2. `VC_LOAD_W` — el riesgo que el encargo mandaba tratar, y qué se hizo

> «`VC_LOAD_W` lee pesos pre-entrenados POR POSICIÓN. Si se compactan los carriles hay que
> remapear ese fichero o dejará de casar EN SILENCIO, que es la peor forma de fallar.»

**Se trató, y el formato del fichero NO cambia.** La decisión fue dejar el fichero en el formato
de siempre —`NSEL[k] * NCTXP` enteros de 32 bits por mezclador, **un peso por carril original
0..160**— y hacer la conversión en memoria:

- **`VC_LOAD_W` (lectura):** se lee el fichero entero a un buffer del ancho antiguo (`NCTXP`) y
  se hace un **gather explícito por `mix_idx`**, carril por carril **por su id**, no por su
  posición nueva. Un fichero de antes de este cambio carga igual que cargaba: los pesos de los
  carriles que hoy siguen vivos entran en su hueco compacto; los de los carriles que ya no se
  recorren se leen y se descartan —nunca aportaban nada, porque su `st` era 0 siempre—.
- **`VC_DUMP_W` (escritura):** el **scatter inverso** por el mismo `mix_idx`. El fichero sale con
  el ancho de siempre, así que `bench/m4-pesos.py` y cualquier volcado antiguo siguen siendo
  legibles. Los carriles no recorridos salen a **0**, que es su aportación real, en vez de
  arrastrar lo último que se cargó ahí y que nunca se movió.

O sea: la compactación es **puramente interna**. No hay fichero que remapear a mano ni volcado
antiguo que se invalide, que era el modo de fallo silencioso que el encargo señalaba.

⚠️ **Lo que esto NO cubre, dicho aquí para que viaje con la decisión:** el mapa depende de
`apagados`, que depende del modelo y del tamaño de bloque. Un volcado hecho con un juego de
perillas y cargado con otro reparte los pesos a carriles distintos —igual que antes del cambio,
donde el desajuste era por posición absoluta—. El cambio no empeora eso, pero tampoco lo arregla.

---

## 3. Controles duros de bytes — los cinco, y el árbol

### El sha del árbol completo: IDÉNTICO

Es el control que el encargo declaraba decisivo. Árbol `/tmp/corpus-f6q3/codigo`
(136.475.130 B, 9.491 ficheros) con `--max`, binario de `abbeb42`:

| | valor |
|---|---|
| bytes de salida | **8.266.736 B** |
| sha256 | `584b72336bd0c6746ca06cc16c625587644ea9e3ca849652c102ce9abcb8c210` |
| referencia (antes del cambio) | `584b72336bd0c6746ca06cc16c625587644ea9e3ca849652c102ce9abcb8c210` |
| veredicto | **IDÉNTICO — 0 bytes de diferencia** |

⚠️ **De dónde sale la fila «referencia (antes del cambio)», dicho porque desde el repo no se puede
rastrear.** Ese lado *antes* **no se midió en este turno**: lo midió el validador de la pasada
anterior (16-08/17-08), y la corrida vive en el ledger de validación del arnés
(`clients/vcompresor/risks.md`, entrada «F6·Q4·C3 … validación tras el rescate»), no en este
árbol. Dentro del repo, `git log --all -S"584b72336bd0c674"` devuelve **sólo** `2bf1365` —este
mismo documento— y en el host queda **una sola** corrida de árbol (`arbol3.log`, que es el lado
*después*). O sea: la cifra es buena y está atestada, pero **su fuente es externa al repo** y por
eso se cita aquí en vez de dejar al lector futuro sin camino.

(`time: 1041,9 s` en esta corrida, pero el host tenía otras cargas: **ese reloj no es una medida**,
sólo el sha lo es.)

### Determinismo cruzado — 4 de 5 medidos, 1 sin muestra

Contra los sha publicados en `docs/f5-velocidad-max.md` §«Determinismo cruzado — 5/5»:

| muestra | bytes esperados | sha esperado (16) | medido ahora | veredicto |
|---|--:|---|---|---|
| codigo | 787.080 | `af18464ffb482fa0` | `af18464ffb482fa0` | ✅ |
| enwik8 | 2.088.518 | `9af1e2cb5309e4d2` | `9af1e2cb5309e4d2` | ✅ |
| sql | 225.810 | `ad5ece6052c087b9` | `ad5ece6052c087b9` | ✅ |
| logs | 48.560 | `70497d35868d3a59` | `70497d35868d3a59` | ✅ |
| csvjson | 540.064 | `eedfa76e795cd24b` | — | ⚠️ **sin muestra** |

⚠️ **El hueco de `csvjson`, sin adornar.** `csvjson.10m` **no existe en este servidor** y **no hay
generador en el repo** que lo reconstruya: se buscó en todo `/tmp` y `~`, y `bench/trae-muestras.sh`
—el único script del banco con pinta de generador— trae las muestras `.deflate` de preflate-rs,
no el juego `texto5`. Los otros cuatro se midieron porque su muestra sí sobrevivió
(`logs.10m` estaba en `/tmp/x5/`, que el intento anterior dio por perdido). **El quinto sha no se
midió y no se inventa.** Queda como el único agujero del control de bytes.

Refuerzo independiente: `codigo`, `sql` y `enwik8` se midieron además en los dos tamaños de
bloque (`-peq`, `-gra`), 9 sha en total, todos idénticos entre antes y después.

### Ida y vuelta — 8/8, no 20/20

```
  codigo   rapido OK   3.145.728 ->   211.266    codigo   maximo OK   3.145.728 ->   107.506
  logs     rapido OK   3.145.728 ->    46.541    logs     maximo OK   3.145.728 ->    17.009
  sql      rapido OK   3.145.728 ->   204.674    sql      maximo OK   3.145.728 ->    89.721
  bin      rapido OK   3.145.728 -> 1.114.521    bin      maximo OK   3.145.728 ->   762.953
  8 bien · 0 mal · 0 sin dato   ·   FIN idayvuelta OK   (rc=0)
```

⚠️ **No son 20/20 y hay que decir por qué:** `bench/idayvuelta.sh` corre 10 tipos × 2 modos contra
`/tmp/corpus-hilos`, que **ya no existe** (mismo barrido de `/tmp` que se llevó el corpus de F5, ver
`docs/f6-arranque-en-frio.md` §0). Se reconstruyó un corpus con los cuatro tipos de los que queda
dato real —`codigo`, `logs`, `sql`, `bin`— y se corrieron esos: **8 de 8, byte a byte contra el
original, en los dos modos**. Faltan `csvjson`, `pdfoffice`, `png`, `jpg`, `video`, `dup` por falta
de muestra, no por fallo. Los cuatro que sí corren cubren el camino cambiado (`--max` → vc-cm).

### `bench/regresion-fase3.sh` — NO SE PUDO CORRER

El script exige el banco `/tmp/deflate-banco2`, que **tampoco existe en este servidor** (`exit 2`,
«no hay banco»). No toca el mezclador —es el candado de `vc-fase3.rs`, el camino DEFLATE— pero el
encargo lo pedía y **no se corrió: se dice, no se finge**.

**Diagnóstico del `rc=2`, leyendo el guión (turno de cierre).** `regresion-fase3.sh` tiene
**dos** salidas con código 2 y conviene saber cuál es, porque significan cosas distintas:

1. `bench/regresion-fase3.sh:24` — `[ ! -d "$BANCO" ] → echo "no hay banco en $BANCO"; exit 2`.
2. la de más abajo — falta `bench/regresion-fase3.esperado`, o sea que nunca se fijó la referencia.

**Es la primera, y la segunda queda descartada:** `bench/regresion-fase3.esperado` **sí existe**
(64 B, del 02-08, `flujos=253 acierto=98.38 fallos=153542 coste=0.960 techo=153542`), y la salida
literal del guión es `no hay banco en /tmp/deflate-banco2`. El banco son 2.522 flujos `.raw`
(~2,77 GB según `bench/muestras.tsv`) que vivían en `/tmp` y que este host ha barrido; **no hay
generador en el repo** que los reconstruya —`bench/trae-muestras.sh` trae las `.deflate` de
preflate-rs, que es otro juego—. Es el **mismo modo de fallo que el hueco de `csvjson.10m`**: una
muestra de `/tmp` que no sobrevivió, no un defecto del árbol.

🔑 **Y es un `rc` que NO acusa al código.** El `rc=1` de este guión es el que significa «la fase 3
se ha movido, míralo»; el `rc=2` significa «no he podido ni mirar». Sale 2 **antes** de compilar o
ejecutar nada de `vc-fase3`, así que no hay ningún resultado suyo —ni bueno ni malo— en este
turno. **Ambiental, con causa exacta. No es un fallo real del árbol.**

---

## 4. La medida — y el falsador se cumple

`perf stat`, muestra `codigo.10m` (10.485.760 B), `--max`, **n=3 con vueltas alternadas**
(antes/después/antes/después/antes/después), sello de binario comprobado en cada lado.

| | instrucciones/byte | ciclos/byte |
|---|--:|--:|
| **antes** (`97ab3b9`, `aca51f5b`) | 69.317 · 69.378 · 69.302 → **69.332** | 43.496 · 47.333 · 39.851 → 43.560 |
| **después** (`abbeb42`, `acb3561a`) | 73.264 · 73.251 · 73.203 → **73.239** | 49.030 · 46.353 · 41.149 → 45.510 |
| **Δ** | **+5,64%** | +4,48% |

**Las instrucciones por byte SUBEN un 5,64%.** El falsador estaba declarado como «que no bajen», y
no bajan: suben. Y la señal no es ruido — la dispersión **dentro** de cada lado es del **0,11%**,
cincuenta veces menor que la diferencia entre lados.

⚠️ **Los ciclos de esta tanda no valen como medida** y no se usan para decidir: el host tenía otras
cargas (un navegador headless, PHP, node) y la dispersión entre las tres vueltas del mismo lado
llega al **18,8%**. Se publican por completitud y porque apuntan en el mismo sentido, no porque
sostengan nada. **Las instrucciones sí valen**: son inmunes a la contención, y por eso el falsador
se declaró sobre ellas.

Control cruzado de la tanda: las **seis** corridas dieron el mismo `out_sha` (`8728674d8a966c7d`),
o sea que el A/B compara dos binarios que producen el mismo archivo. Vía (a) confirmada por
tercera vez.

### Por qué sube, que es lo que sirve

La cuenta que el encargo hacía —«~65% menos de trabajo en el mezclador»— es correcta **sobre las
multiplicaciones**, y aun así el saldo es negativo, porque lo que se quita y lo que se pone no son
la misma clase de instrucción:

- **Lo que se quita:** 4 mezcladores × (160 − 56) = 416 multiplicaciones-acumulaciones por bit…
  pero eran **AVX2, de 8 en 8**. En instrucciones reales son 4 × 13 = **~52 vectoriales**, y sobre
  datos que ya estaban calientes y sin dependencias entre sí.
- **Lo que se pone:** `st_compacto()` se llama **dos veces por bit** —una en `mezcla()` y otra en
  `actualiza_mezcla()`— y cada llamada pone a cero un array de 160 enteros y hace ~56 **cargas
  indexadas escalares** (`stc[j] = self.st[mix_idx[j]]`). Son del orden de **2 × (40 + 56) ≈ 190
  instrucciones escalares** por bit, con indirección.

Se cambian ~52 instrucciones vectoriales por ~190 escalares. El signo del resultado estaba escrito
en esa proporción, y ninguna medida de «carriles» lo habría enseñado: hacía falta contar
**instrucciones**, que es exactamente lo que el falsador pedía.

### 4.1 Reproducción independiente (performance-lead)

Regla del rol: *«un regresión debe reproducirse antes de marcarse»* — aplica igual a un negativo.
Repetido en turno aparte, binarios reconstruidos desde cero (no reutilizados), mismo host, mismo
`codigo.10m` (`132c06fedd3d6c29…`, 10.485.760 B), mismo `perf stat -e instructions,cycles`, n=3
alternadas, `bin_sha` comprobado en cada lado antes de medir:

| | instrucciones/byte | ciclos/byte |
|---|--:|--:|
| **antes** (`97ab3b9`, `aca51f5b…`) | 69.266 · 69.274 · 69.284 → **69.275** (disp. 0,03%) | 35.947 · 36.999 · 38.052 → 36.999 (disp. 5,69%) |
| **después** (`abbeb42`/`2bf1365`, `acb3561a…`) | 73.180 · 73.186 · 73.199 → **73.188** (disp. 0,03%) | 37.551 · 40.407 · 40.282 → 39.413 (disp. 7,25%) |
| **Δ** | **+5,65%** | +6,52% |

Confirma la cifra de §4 casi al byte (+5,65% contra +5,64%). `out_sha` idéntico en las 6 corridas
(`8728674d8a966c7d`), igual que en la tanda original. Los ciclos vuelven a subir en las tres
parejas por separado (no sólo en la media) — 37.550→40.407→40.282 contra 35.947→36.999→38.052 —
pero la tendencia creciente dentro de CADA lado a lo largo de la tanda (~11 min de corridas de
108 s) apunta a deriva del host (probable throttling térmico), no a ruido aleatorio; se publica
por la misma razón que en §4, sin usarla para decidir. `pgrep -f 'pruebas.sh|perf'` antes de medir
no encontró ninguna tanda ajena corriendo.

**Veredicto reproducido de forma independiente: el falsador se cumple.** No hay negativo que
revertir en la conclusión de §4.

---

## 5. Veredicto y qué queda vivo

**NO ENTRA AL DEFECTO.** El cambio es exacto —cuatro controles de bytes independientes lo dicen—
pero es más lento en la única métrica que el encargo declaró como juez. Se anota el negativo con
su premisa, como manda el encargo, y la rama `feature/f6-q4-c3-mezclador-compacto` queda como
registro medido, **no para fusionar**.

**La premisa que cae:** «compactar el vector de entradas del mezclador es barato». No lo es
mientras la compactación se haga **con un gather en el bit caliente**. El trabajo muerto que se
quita es vectorial y barato; el gather que lo sustituye es escalar y caro.

**La premisa que NO cae, y es la que abre el siguiente intento:** que los ~107 carriles sobran
sigue siendo cierto, y que sumarlos no aporta nada también. Lo que falla es *cómo* se evitan. El
camino que esta medida deja señalado es **escribir `st[]` ya compactado desde `p()`** —que el
bucle de predicción deposite cada valor directamente en su hueco compacto, en vez de llenar 160
ranuras y luego recogerlas dos veces—. Eso hace desaparecer el gather en vez de pagarlo, y con él
las ~190 instrucciones escalares que se han medido aquí. Es un cambio más invasivo (toca los cinco
sitios que escriben `st[]`, y `censo`/`VC_VOLCAR`/la dispersión leen `st[]` por índice original,
así que necesitarían el mapa inverso), y **no se ha medido**: es la hipótesis siguiente, no un
resultado.

### El asiento en `bench/decisiones.tsv` — sí lleva fila, y por qué

Una versión anterior de este documento decía que el negativo se quedaba **sin** fila a propósito,
con este argumento: `decisiones.tsv` registra decisiones de **ratio**, `decide.py` empareja control
contra tratamiento por bytes, este cambio no mueve un solo byte y por tanto una fila suya saldría
`SIN MEDIDA` para siempre. El diagnóstico técnico era correcto —y se cumple: la fila sale
`SIN MEDIDA`—, pero la conclusión no, por dos razones:

1. **El encargo lo pide explícitamente** («cada decisión con su fila en `bench/decisiones.tsv`
   anclada al fuente»), y una decisión sin asiento es una decisión que se vuelve a intentar. El
   valor de la fila no es la cifra —que vive aquí— sino la **premisa** y el **`caduca_si`**, que es
   lo único que evita que alguien recompacte el mezclador dentro de seis meses sin saber que ya se
   midió.
2. **Ya hay precedente en el propio fichero**: `f6q3-cebado-entre-frames` está asentada y sale
   `SIN MEDIDA`, con el motivo escrito en su `por_que` («no es que falte la medida: es que no
   encaja la FORMA»). Es exactamente este caso, y el criterio del banco es documentar la limitación
   del instrumento, no ocultar la decisión.

Fila añadida: **`f6q4c3-mezclador-compacto`** · `RECHAZADO` · juego `texto5` · anclada a
`crates/vc-cm/src/lib.rs` con el patrón de la firma de `punto()` **sin compactar**
(`^fn punto\(w: &\[i32\], st: &\[i32; NCTXP\], simd: bool\) -> i64 \{$`), que casa exactamente una
vez tras el revert. El ancla es el candado: **el día que deje de casar es que la compactación
volvió a entrar por la puerta de atrás.**

`decide.py` recomputa con **rc=0 y cero errores de declaración** (15 filas). El recuento de avisos
pasa de 5 a 6, y el sexto es esta fila en `SIN MEDIDA`: es el estado **honesto** —el falsador se
midió con `perf stat` en un worktree aparte y no por `mide.sh`, así que no hay filas suyas en
`bench/historia.jsonl`— y está explicado en el `por_que` de la propia fila.

### El código, revertido

El cambio **no puede entrar al defecto**, y no bastaba con no fusionar: la compactación estaba
**activa en el build por defecto** de la punta de la rama (`mix_idx`/`mix_n` se calculan siempre en
`Predictor::new`; no había ninguna perilla que la apagara). Así que se revierte con un commit de
`git revert` sobre `abbeb42`. Tras él, `git diff 97ab3b9 -- crates/` es **vacío**: el fuente vuelve
a ser, byte a byte, el de la base.

La rama **conserva** el informe, el asiento y toda la historia —incluido el `abbeb42` original— como
evidencia del negativo. **Se revierte el código, no la lección.**

**Comprobación de que el revert deja el binario donde estaba** (no sólo el fuente): recompilado
desde cero (`cargo build --release`, rc=0) y comprimida la muestra rápida `codigo.10m`:

```
./target/release/vcompresor compress /tmp/v6x/codigo.10m -o /tmp/p-revert.vc --max
787.470 B · sha256 8728674d8a966c7d7bf2a2d99f32a86beb46dc5916551d6fa3d7aabdeabd73c8
```

Ese `8728674d8a966c7d…` es **exactamente** el `out_sha` que §4.1 registra en las **seis** corridas
del falsador —las tres de antes y las tres de después—, que es la otra cara del resultado de esta
fase: el cambio era exacto, así que quitarlo también lo es. El binario de HEAD produce el `.vc` de
la base, byte a byte.

---

## 6. Reproducir

```bash
# árbol completo, el control decisivo
./target/release/vcompresor compress /tmp/corpus-f6q3/codigo -o /tmp/x.vc --max
sha256sum /tmp/x.vc   # 584b72336bd0c674...

# determinismo, una muestra
./target/release/vc-cm c /tmp/x5/logs.10m /tmp/logs.vccm   # 48.560 B, 70497d35868d3a59...

# ida y vuelta (corpus reconstruido: sólo los 4 tipos con dato real)
VC_IV_TIPOS="codigo logs sql bin" bash bench/idayvuelta.sh /tmp/iv-corpus

# el falsador
perf stat -e instructions,cycles ./target/release/vcompresor compress /tmp/v6x/codigo.10m -o /tmp/p.vc --max
```
