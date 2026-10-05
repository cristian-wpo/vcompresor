# F6 · Q5 — PRIMER COMPARTIDO: NEGATIVO en la proyección, con premisa

Encargo del dueño: romper el compromiso de F5 (partir en frames usa los 12 núcleos pero cuesta
bytes) con un **primer compartido** — un bloque de cabecera comprimido primero y en serie, del que
todos los demás frames arrancan cebados (instantánea de tablas, memcpy) y sobre el que pueden mirar
hacia atrás como ventana de sólo lectura. Camino crítico = primer + un frame, no N frames en serie.

**Puerta de entrada:** `docs/f6-arranque-en-frio.md` (F6·Q3). Su falsador **no se disparó**
(§7.1: +0,001% a `--solid=11`, tres órdenes de magnitud por debajo del 0,5%), así que la puerta
abrió. **Salvedad que hay que decir:** el brief de Q5 escribió esa puerta con **O**, no con **Y**
(«…sigue costando mas del 0,5% de bytes, **o** el coste crece al bajar a 12 frames»), y
`docs/f6-arranque-en-frio.md:296-297` dice de la segunda mitad que «el coste de (c) sí **crece** al
bajar a 12 frames (0,000%→0,001%), así que la segunda mitad del falsador se cumple». Leída con O,
la puerta del brief estaba cumplida por esa mitad. Se deja anotado, no resuelto: este documento se
escribió bajo la lectura de Q3 (Y), y su resultado es negativo de todas formas, así que ninguna de
las dos lecturas cambia el desenlace. Pero Q3 §7.2-7.3 ya dejó dicho que lo medido allí —cebar desde el frame anterior COMPLETO—
está **falsado por el reloj**: antepone un frame entero de modelado al camino crítico y divide el
paralelismo por 2. Q3 recomendó seguir a Q5 con "un primer BARATO", no con lo ya medido.

> **VEREDICTO: NEGATIVO. El falsador de este encargo se dispara en la proyección.** No se
> implementa el cebador en el producto — `crates/` sigue intacto. Abajo está qué se midió, con qué
> premisa, y con qué falsador exacto.
>
> **REESCRITURA (2026-08-17):** el negativo se sostiene, pero la mitad que lo sostiene cambió.
> Una auditoría anterior había identificado mal "el defecto de hoy" (usaba `--solid=32`, que es el
> suelo del clamp, no el defecto) y combinaba en una resta dos medidas de planos distintos (frames
> sueltos vs archivo entero). Corregidas las dos cosas (§1, §3): el **reloj pasa 3× en las seis
> filas medidas** (antes fallaba en cuatro); lo único medido de plano único es una **cota
> superior** de bytes (+4,315%, 12 piezas **sin cebar**, sin modelo), más de 4× el 1% exigido: la
> mitad de bytes **no queda demostrada**, que es lo que el falsador exige. Sigue sin haber ninguna
> fila que pase las dos mitades a la vez.

---

## 1 · Falsador, declarado antes de proyectar (verbatim del brief)

> Que el reloj no baje al menos 3× con 12 frames, **O** que los bytes se pasen del 1% de pérdida.
> Cualquiera de las dos cosas y el diseño no entra: se anota el negativo con su premisa.

Las dos mitades a la vez. Objetivo declarado: 4-5× de reloj y ≤0,3% de bytes, sobre la línea de F5
(597,6 s) tal como el brief la cita. **Esa línea no es reproducible aquí** — se midió sobre un
corpus que `docs/f6-arranque-en-frio.md` §0 declara perdido, y queda como contexto histórico, no
como denominador.

**El denominador correcto NO es `--solid=32`.** `--solid` es `clamp(32, 256)`
(`crates/vc-cli/src/main.rs:239`; también `docs/f6-arranque-en-frio.md:150-151`) — 32 es el suelo
del clamp, no el defecto. El código lo fija, no sólo la ayuda: `crates/vc-cli/src/main.rs:131`
(`const SOLIDO_DEF: usize = 64;`) y `crates/vc-cli/src/main.rs:69` — «`--solid[=MiB]` frame size in
MiB (32-256). **DEFAULT SINCE 2026-07-31: 64 MiB.**» §3 usa como denominador la fila de archivo
entero al **defecto real**
(`--solid=64`), medida sobre el mismo corpus congelado que sostiene todo este documento:
**705,03 s** (`bench/historia.jsonl:854`, `bin_sha=e7a705aaaef0`, `cpu_pct=162,0` — coincide con
el "167% de CPU" que el propio brief cita como el comportamiento de hoy). La fila de `--solid=32`
(424,87 s, `historia.jsonl:853`) queda como dato de contexto (el suelo del clamp), no como "hoy".

---

## 2 · Qué se probó

Dos curvas medidas, íntegras en [`bench/registros/f6q5-curvas.md`](../bench/registros/f6q5-curvas.md)
(datos crudos en `bench/historia.jsonl`, etiquetas `F6.Q5 P1 s11 *` / `F6.Q5 P2 s11 *`):

- **P1 — recuperación vs DISTANCIA** (§1 del registro): los frames 1..9 de un árbol partido a
  `--solid=11` (12 frames), cada uno cebado **directamente desde F0** (no en cadena, desde N−1) con
  ventana de historia sobre F0. La recuperación va de −0,52% (N=4, negativa) a 6,62% (N=1) y **no
  es monótona con la distancia** — depende del contenido de cada frame, no de cuántos frames de
  por medio hay.
- **P2 — recuperación vs TAMAÑO DEL PRIMER** (§2 del registro): F1 cebado con prefijos crecientes
  de F0 (5/10/25/50/75/100% de sus 11.479.652 B), midiendo cuánto del hueco de 40.538 B recupera
  cada tamaño. La curva **no es lineal**: de 5% a 75% de primer se recupera 21,6%→51,5% del hueco
  (~0,43 puntos por punto de primer); de 75% a 100% salta a 100,0% (~1,94 puntos por punto, 4,5×
  más rápido). Casi toda la recuperación vive en el último tramo, cerca del F0 completo.
- **Proyección del producto** (§5 del registro): la medida de (b)/(c) no es el coste real —vuelve
  a pasar el prefijo por el modelo cada vez—, así que el camino crítico se modela como
  `T(F0 en serie, tamaño del primer) + T(frame más lento) + memcpy(instantánea)`, con
  `T(F0 100%) = 107,92 s` medido y los tamaños intermedios de `T(F0 prefijo)` modelados
  proporcionales a su tamaño en bytes. El reloj de P2 no sirve para medir esto directamente (mide
  T(F1 primado), no T(F0 en serie)) y parte de la tanda está contaminada por CPU de causa no
  identificada — no por solape entre P1 y P2, que corrieron en fila sin pisarse un segundo
  (`f6q5-curvas.md` §5, corregido 2026-08-17): la contaminación sigue apareciendo 33-41 minutos
  después de que P2 termine.

## 3 · Resultado de la proyección — el reloj no está medido, los bytes no se pueden demostrar

**REESCRITURA (2026-08-17), dos correcciones sobre lo publicado antes:**

1. **Denominador.** `--solid=32` (424,87 s) NO es "el defecto de hoy" — es el suelo del clamp
   `clamp(32,256)` (`crates/vc-cli/src/main.rs:239`). El defecto real es `--solid=64`
   (`crates/vc-cli/src/main.rs:131`, `const SOLIDO_DEF: usize = 64;`; `crates/vc-cli/src/main.rs:69`,
   "DEFAULT SINCE 2026-07-31: 64 MiB"), medido **sobre el mismo
   corpus** que usan P1/P2: **705,03 s** (`historia.jsonl:854`, `bin_sha=e7a705aaaef0`,
   `cpu_pct=162,0` — coincide con el "167% de CPU" del propio brief). Los 597,6 s de F5 siguen sin
   ser reproducibles (corpus perdido, `docs/f6-arranque-en-frio.md` §0) y quedan fuera del cálculo.
2. **No se resta entre planos.** La versión anterior de la fila de 100% restaba una recuperación
   medida sobre frames sueltos (`bin_sha=f751298736e8`) de un delta medido sobre archivo entero
   (`bin_sha=e7a705aaaef0`) — la combinación de planos que la regla de procedencia de este encargo
   prohíbe explícitamente. Se retira esa resta (detalle completo: `f6q5-curvas.md` §5.3).

**El camino crítico no cambia** — T(F0 en serie)=107,92 s (fila `15:59:11`, `f6q5-curvas.md` §3,
`cpu=99`) + T(frame más lento)=**F8=123,08 s** (fría, limpia, `cpu_pct=98`, `f6q5-curvas.md` §1;
F3/F6/F7 siguen contaminadas — `cpu_pct` 82,0/80,0/92,0 — y sin remedir en limpio este turno) +
memcpy≈0,9 s (874 ms, Q3 §6) = **231,90 s**. Lo que cambia es contra qué se compara:

| primer | camino crítico | vs 705,03 s — cociente ENTRE PLANOS, no medido | reloj ≥3×? | bytes, archivo entero (medido, plano único) | bytes ≤1%? |
|---:|---:|---:|:---:|---:|:---:|
| 100% | 231,90 s | **3,04×** | **SÍ** | **+4,315%** | NO |
| 75% | 204,92 s | **3,44×** | SÍ | +4,315% | NO |
| 50% | 177,94 s | **3,96×** | SÍ | +4,315% | NO |
| 25% | 150,96 s | **4,67×** | SÍ | +4,315% | NO |
| 10% | 134,77 s | **5,23×** | SÍ | +4,315% | NO |
| 5% | 129,38 s | **5,45×** | SÍ | +4,315% | NO |

**El reloj no queda medido — se dice tal como sale, no se fuerza el negativo por esta mitad ni
se le concede.** El cociente proyectado contra 705,03 s sale ≥3× en las seis filas, pero mezcla
`vc-cm`/frames sueltos (`f751298736e8`) con archivador/archivo entero (`e7a705aaaef0`): la misma
combinación de planos que el punto 2 retira para bytes. No hay ninguna fila de archivo entero con
primer a ningún tamaño: **la mitad de reloj del falsador está NO MEDIDA.**

**Los bytes fallan las seis con un único número medido, sin modelo.** La cifra de la tabla
(+4,315%) es el delta de archivo entero, mismo plano y mismo `bin_sha` (`e7a705aaaef0`), entre
partir en 12 piezas sin cebar (`--solid=11`, `historia.jsonl:841`, 8.623.446 B) — fila que requiere
la perilla de banco `VC_BANCO_SOLIDO=1`, porque `--solid=11` está bajo el suelo de 32 que el
producto acepta (`crates/vc-cli/src/main.rs:237-239`) — y el defecto real
(`--solid=64`, `historia.jsonl:854`, 8.266.736 B): `(8.623.446 − 8.266.736) / 8.266.736 =`
**+4,315%** (356.710 B). Es una propiedad del particionado, no del tamaño del primer — no hay
ninguna fila "archivo entero, cebado, primer=X%" medida a ningún tamaño, así que la misma cifra se
aplica a las seis filas. La recuperación de P1 (168.764 B sobre F1..F9, 2,269%, `f6q5-curvas.md`
§1) es real, pero vive en el plano de frames sueltos (`bin_sha=f751298736e8`) y **no se resta de
este número** — sería la misma combinación de planos prohibida en el punto 2, arriba. No hay
ninguna medida en el plano del archivador que diga cuánto de esos 356.710 B se recuperaría
cebando el archivo completo de verdad: **no medido**.

**Ningún punto pasa las dos mitades del falsador a la vez.** Las seis filas pasan reloj (≥3×);
ninguna de las seis demuestra la mitad de bytes (+4,315%, más de 4× el 1% exigido, con un solo
número medido). El falsador se dispara igual que antes de esta reescritura — pero ahora por bytes, con un argumento que no
necesita ni la curva de distancia de P1 ni el proxy de Tabla A: sólo dos filas de archivo entero,
mismo binario, restadas entre sí.

## 4 · Premisa de este negativo — para que caduque correctamente

Este negativo vale para exactamente el diseño que se midió, y no para otro:

- El primer **es el frame 0**, tal como se almacena hoy (o un **prefijo por bytes desde su
  principio**, de tamaño variable).
- El cebado es una **instantánea del estado de modelos** al terminar de procesar ese primer,
  copiada una vez por hilo (memcpy, no remodelado) — el coste de montarla está separado del coste
  de *medirla* (Q3 §6): 3,4 Gciclos / 874 ms por hilo, y **no** es la objeción.
- La ventana de historia es de **sólo lectura sobre ese primer**, para que la descompresión se
  mantenga paralela (la restricción no negociable del brief).
- Un negativo caduca con su premisa: si cualquiera de estas tres condiciones cambia, esta tabla no
  aplica y hay que re-medir.

**Lo que este negativo NO cubre — vías futuras, sin medir, sin implementar:**

1. **Primer = sufijo de F0, no prefijo.** Hipótesis de `f6q5-curvas.md` §2: si el salto
   75%→100% viene de la adyacencia entre el final de F0 y el principio de F1 (contenido
   literalmente contiguo), un primer pequeño tomado de la **cola** de F0 podría acercarse a la
   recuperación del 100% sin pagar el F0 entero. Sigue siendo un prefijo/sufijo de un bloque que
   ya existe como frame — **no cambia el formato del contenedor** —, pero no hay dato de bytes ni
   de reloj para esta geometría: P2 nunca cortó un sufijo.
2. **Un primer no-prefijo — por ejemplo, una muestra repartida por todo el árbol en vez de la
   cabeza.** Esto **SÍ es cambio de formato del contenedor**: un primer que no es un prefijo
   contiguo de los bytes de entrada no encaja en "F0 ya se almacena hoy como un frame normal", la
   preferencia de diseño del brief para no tocar el contenedor. El brief es explícito: si el
   diseño viable exige cambiar el contenedor, se presenta sin implementar. Queda presentado aquí,
   sin medir y sin implementar: sería la vía a explorar si (1) tampoco cierra el falsador.

Ninguna de las dos queda falsada por lo medido en este encargo — están fuera del espacio que P1/P2
cubrieron.

## 5 · Qué NO se hizo, y por qué

- No se tocó `crates/`. No hay versión de modelo nueva, no hay entrada en `MODELOS_LEIBLES`, no
  hay cambio en el `match` que las implementa — las dos listas siguen como estaban porque no hay
  nada que listar.
- No se arregló el guardia de RAM (Q3 §7.2: reserva `2048 + solido` MB/hilo cuando el consumo real
  cebando sería ~2,84 GiB/hilo). Es un arreglo que sólo hace falta si el diseño entra, y no entró.
- No se midió la pata ARM64 del determinismo cruzado (`f6q5-curvas.md` §4: no hay hardware, cross-
  compiler ni emulador ARM64 en este servidor; el 6/6 anterior fue una sesión humana de una sola
  vez en el Mac del dueño). Tampoco hacía falta para este veredicto: no hay binario que determinar
  entre arquitecturas si no hay binario nuevo.
- No se tocó `bench/techos.tsv`: ninguna fila de ese libro compara con un rival externo, y esta
  decisión es interna al formato de frames, no un techo de compresión general.
- No se relanzó la remedición en limpio de F3/F6/F7 (`cpu_pct` 82,0/80,0/92,0, contaminadas): una
  delegación previa a esta reescritura la tenía asignada y no entregó ninguna fila nueva; "sin
  tandas nuevas" prohíbe relanzarla en este turno. No cambia el veredicto: el falsador ya se
  dispara por bytes, con un argumento que no depende de T(frame más lento).

## 6 · Trazabilidad

- Curvas completas y proyección: [`bench/registros/f6q5-curvas.md`](../bench/registros/f6q5-curvas.md).
- Puerta de entrada: [`docs/f6-arranque-en-frio.md`](f6-arranque-en-frio.md) (F6·Q3), en particular
  §6 (precio de cebar), §7.2 (el hallazgo que no depende de la corrida pendiente) y §7.3
  (recomendación: "un primer BARATO").
- Datos crudos: `bench/historia.jsonl`, etiquetas `F6.Q5 P1 s11 *` y `F6.Q5 P2 s11 *`.
- Decisión asentada: `bench/decisiones.tsv`, id `f6q5-primer-compartido`.
