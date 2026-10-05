# F5 · El Mac como banco de BYTES, y el replay de la cola del predictor

Sesión del **2026-08-03**. Commits `dda0feb` · `dcbe514` · `c4d2833` · `b254845` · `4251741`.

> **Por qué esto vive en `docs/` y no en `bench/decisiones.tsv`.** Se intentó anotarlo como
> decisiones y el validador (`bench/decide.py`) lo rechazó, con razón: **las medidas de esta
> sesión no están en `bench/historia.jsonl`**, así que las decisiones habrían salido «SIN MEDIDA»
> y el panel gritaría en cada ejecución. Eso es exactamente el fallo que ya se corrigió una vez
> —*«un aviso que sale siempre no es un aviso»*—. Ver §6: meterlas en el libro mayor es una tarea
> con nombre, no un `append`.

---

## 1. El Mac es banco válido para BYTES — y sólo para bytes

La salida de vcompresor es bit-idéntica entre x86 y ARM (no hay una sola operación de coma
flotante en el camino de la probabilidad). Re-verificado en esta sesión:

```
512 KiB de codigo →  out=18066  ·  sha256 39a2e16087d4461109273ca62de243c4…
                     IGUAL en el Mac (arm64) y en el servidor (x86_64)
```

Eso permite repartir barridos de bytes entre los núcleos del Mac **sin tocar el servidor**.

⚠️ **El reloj NO.** Reloj, ciclos y memoria siguen saliendo del servidor y en serie.

⚠️ **Y el Mac no es una máquina limpia**: durante la sesión compitieron `mediaanalysisd`,
Jump Desktop, `vcompresor.app`, VS Code y WindowServer. Los procesos del banco tenían **~1,6
núcleos de 10**, y una compresión de 8 MiB que en solitario tarda **58 s** llegó a tardar **28
minutos**. Para bytes eso da igual (son deterministas); para *planificar* una tanda, no.

---

## 2. El replay de la cola — probar una configuración sin volver a correr los modelos

### Por qué se puede, y por qué es exacto y no una aproximación

**Ningún submodelo ve nunca la predicción final.** `self.pr` aparece en tres sitios y sólo tres:
devolverla, la traza de coste, y `errf` para los pesos del SSE. Los StateMap aprenden de
`target = bit * 65535`; cada mezclador de capa 1 de su propia salida (`mp[k]`); la capa 2 de la
suya (`mixer_p`); la cadena ISSE de la suya (`isse_pout[k]`).

Luego la trayectoria de `st[]` **no depende** de cómo esté configurada la cola, y volcarla una vez
permite reproducir cualquier configuración aguas abajo.

Y el replay **no re-implementa la mezcla**: se extrajeron `mezcla()` y `actualiza_mezcla()`, y los
llama a ellos. Ésa es la diferencia con las dos veces que esto ya ha mordido en este proyecto —el
«oráculo» que resultó ser un bug, el atajo que elegía configuraciones nunca corridas—: los dos
cuadraban consigo mismos.

```
VC_VOLCAR=<f>  vc-cm c muestra.bin /dev/null     vuelca st[] + sel[] + sel2 + cx[] por bit (350 B)
vc-replay <f>                                     la config por defecto (control)
vc-replay <f> --barrido --jobs 8                  la rejilla de la cola
vc-replay <f> --sse-w0                            un barrido de un CONTINUO
```

### Lo que cubre y lo que no

| cubre (vive aguas abajo de `st[]`) | **no** cubre |
|---|---|
| `VC_MIX_LR`, `VC_DECAY`, `VC_SSE` | `VC_ISSE_LR` — mueve `st[IISSE]`, cambia el propio volcado |
| `sse_w` (los 40 pesos de la calibración) | máscaras de ablación de modelos |
| pesos iniciales del mezclador | tamaños de tabla, contextos, CDC, carriles |

### Lo que cuesta

| | CPU por configuración |
|---|---|
| corrida real (64 KiB) | 0,87 s |
| replay | **0,217 s** → **4,0×** |

⚠️ **4×, no los ~100× que parecía de lejos**, y el motivo es un resultado en sí mismo: **la cola no
es barata** — cuatro productos escalares de 160 pesos por bit más cinco etapas de calibración. El
volcado ahorra el modelo, no la mitad del trabajo.

El pipeline sí multiplica: `VC_ISSE_LR` necesita compresión real, las otras tres salen del mismo
volcado ⇒ **11 compresiones + 84 replays = las 924 configuraciones** de una muestra.
**4.620 puntos con 44 compresiones, en 26,3 minutos.**

### Los controles (`bench/replay-control.sh`)

| | |
|---|---|
| **A** exactitud | real 4413 B (códec 4403) = **replay 4403** |
| **B** reparto | 1 hilo ≡ 8 hilos, 84 filas idénticas ⇒ `reinicia_cola` deja la cola como el constructor |
| **C** defecto | el barrido reproduce los bytes del compresor ⇒ el replay no está desplazado |
| **cruce** | **191 de 191 puntos coinciden AL BYTE** con medidas brutas hechas por otro camino |

🔑 **El cruce falló a la primera**: 191 de 191 discrepaban… **todos en exactamente 10**, sin una
sola excepción. No era el replay fallando, era el replay exacto medido en otra unidad (bytes de
códec vs de archivo, 10 B de cabecera). **Un control que sólo mirase «¿son iguales?» habría tirado
el método entero.** Lo que salvó la conclusión fue mirar la *distribución* de las diferencias:
constante y sin varianza ⇒ unidad; dispersa ⇒ error de verdad.

---

## 3. Las respuestas que estaban abiertas

### ¿Interactúan las perillas del mezclador?

`bench/mezclador.sh` dice por escrito que ésta es «una pregunta que no tenemos». Rejilla completa,
5 muestras, 512 KiB:

| muestra | defecto | OFAT | rejilla | **lo que OFAT no ve** |
|---|---|---|---|---|
| codigo | 18.066 | 17.968 | 17.967 | −0,006% |
| csvjson | 52.944 | 52.909 | 52.909 | **+0,000%** |
| enwik8 | 117.576 | 117.442 | 117.442 | **+0,000%** |
| logs | 5.631 | 5.593 | 5.593 | **+0,000%** |
| sql | 26.493 | 26.431 | 26.431 | **+0,000%** |

**Interactúan** —`MIX_LR`×`DECAY` llega al 0,769% de desviación de la aditividad— **pero no en la
zona del óptimo**. En 4 de 5 muestras OFAT encuentra exactamente el mismo punto.

🔑 **La rejilla cara se ha gastado una vez para LICENCIAR el barrido barato.** Ése es el resultado,
más que cualquier configuración.

### Los tres niveles

```
defecto de hoy (una config para todas) : 220.710 B
GLOBAL   (la mejor config única)       : 220.474 B   −0,107%
INDIVIDUAL (la mejor POR muestra)      : 220.342 B   −0,167%
```

🔑 **Presupuesto de la selección adaptativa: −0,060%.** Es el techo de lo que puede cobrar una
sonda que detecte el tipo de dato y ajuste el mezclador. Con eso se puede decidir **sin
construirla**: para el mezclador, no se paga. (Para los *carriles* es otra pregunta y otro número.)

### ¿Se puede cribar un espacio grande con una muestra barata? **NO como se supone**

Correlación de rangos entre muestras sobre las 924 configuraciones: **rho 0,88–0,99**. Altísima.
Y aun así:

> **un cribado top-20 (el 2% mejor) tira el ganador en 17 de 20 pares.** El ganador de una muestra
> cae en el puesto **32–239 de 924** de otra.

Los buenos puntos están apelotonados en un rango de bytes minúsculo, y ahí la diferencia entre
muestras domina el orden. **Para no perder el ganador hay que pasar el top ~26%, no el top 2%.**
Eso destroza la aritmética de cualquier embudo agresivo.

---

## 4. Lo aplicado

### La dispersión se calculaba en cada bit para borrarla dos líneas después

La decisión `v7-dispersion-fuera` ya estaba tomada y anotada —y decía *«no cuesta reloj»*— pero la
implementación dejó el **cálculo** dentro: un min/max/suma sobre 43 entradas en cada bit que el
bucle de ablación ponía a cero acto seguido.

```
v7  out=18066  sha 39a2e160…   ← IDÉNTICO al de antes y al del servidor
v6  out=18084                  ← la dispersión sigue viva donde debe
round_trip=OK en las dos
```

⚠️ **Cuánto ahorra de reloj: SIN MEDIR.** Es medida de tiempo → servidor y en serie. No se publica
estimado.

### Dos trampas quitadas del barrido

- **`VC_DISP` fuera**: perilla **inerte**. `v6 18.084 · v6+VC_DISP=0 18.066 · v7 18.066 ·
  v7+VC_DISP=0 18.066` ⇒ `v7 ≡ v6+VC_DISP=0` byte a byte. Gastaba 2 compresiones por muestra y por
  talla en una columna que no podía informar de nada, y su aviso de «empatan» **no saltaba** porque
  sólo avisa cuando el defecto declarado no está entre los empatados — y estaba.
- **`VC_SSE` desde 1, no desde 3**: el 3 era **el borde**, así que lo único que el barrido podía
  decir era «más abajo es mejor». Extendido: `1:17.988 · 2:17.962 · 3:17.967 · 4:17.973 ·
  5:17.988`. **El óptimo es 2 y la curva se da la vuelta** — la señal que valida un barrido,
  invisible mientras empezara en el borde.

---

## 5. El negativo, que vale más que el positivo que parecía ser

`sse_w` arranca en 49152 = 3/4. Ese 3/4 venía de cuando la mezcla era **fija**; al hacerse
aprendida pasó a ser sólo **el punto de partida**, y nadie volvió a preguntárselo.

| tamaño | óptimo | contra 49152 |
|---|---|---|
| 64 KiB | 16384 | −1,02% (validado con corrida real y round-trip OK) |
| 512 KiB | 24576–32768 | −0,35% en código, **gana en las 5 muestras** |
| **8 MiB** | **da igual** | **0,000% — 20 puntos, ni una variación** |

Con 67M bits los pesos convergen y el arranque se olvida. **No hay nada que tocar.**

🚨 **Cuarta vez que el tamaño de la muestra decide la respuesta** (`VC_ISSE_LR`, `cuantos`,
`familia`, ésta). En este proyecto **un barrido a ≤512 KiB es sospechoso por defecto**.

🚨 **Y el corolario que hace peligroso el replay**: 33 puntos en **7,6 s** es exactamente la
velocidad que hace falta para producir tablas convincentes más deprisa de lo que uno se acuerda de
dudar de ellas — con curvas que se dan la vuelta en el sitio correcto, que es la señal que uno usa
para fiarse. **La velocidad no arregla el problema del tamaño: lo amplifica.** El volcado tiene que
ser del tamaño en que se va a decidir.

---

## 6. 🚧 Para que esto entre en el libro mayor hace falta una tarea, no un `append`

`bench/historia.jsonl` tiene **696 filas y todas son `host: vertice`**. Meter filas del Mac es
peligroso hoy:

- **32 guiones** leen el fichero y **ninguno filtra por `host`**.
- `bench/readme.py:85` hace `z["seg"] / nd["seg"]` — **divide por el reloj**, y el README publica
  esas cifras.

Mezclar bytes-del-Mac con relojes-del-servidor en la misma serie contaminaría el libro mayor y el
README. **La tarea es: hacer `host` discriminante en los consumidores** (o marcar las filas sin
reloj y que quien divida por `seg` las descarte). Hasta entonces, las medidas del Mac viven en sus
propios TSV, versionados:

- `bench/rejilla.tsv` — 4.620 puntos, 5 muestras, 512 KiB
- `bench/rejilla-bruta-control.tsv` — 191 filas medidas a lo bruto (control externo permanente)
- `bench/ofat8.tsv` — el barrido a 8 MiB (ver §7)

---

## 7. 🚧 Lo que quedó a medias

Dos medidas quedaron corriendo en el Mac y **probablemente no sobrevivan al cierre de la ventana**.
Ninguna es reanudable; hay que relanzarlas. Con el Mac despejado tardan **minutos**, no horas:

```sh
# 1) EL BARRIDO QUE DECIDE — OFAT a 8 MiB, 5 muestras, VC_SSE de 1 a 5.
#    Es lo único que convierte la rejilla de 512 KiB en valores aplicables.
python3 bench/ofat8.py --jobs 8

# 2) EL CONTROL QUE FALTA a la conclusión de sse_w0 (§5).
#    `bench/comun.sh` tiene `distingue_o_aborta`: «dos casos que dan EXACTAMENTE el mismo
#    resultado casi nunca significa "no hay diferencia": significa que la entrada es demasiado
#    pequeña o QUE LA BANDERA NO LLEGÓ». Los 20 puntos idénticos a 8 MiB son ese caso y no se
#    les aplicó su propia prueba. Si ni los extremos (0 y 65536) mueven un byte, no es
#    convergencia: es que la perilla no llega.
for w in 0 4096 24576 49152 65536; do
  VC_SSE_W0=$w ./target/release/vc-cm c /tmp/b_codigo.bin /tmp/ctl8.vc 2>&1 | grep -o "out=[0-9]*"
done
```

⚠️ A favor de que **sí** llega: la misma variable **sí** cambia los bytes a 64 KiB (4368 vs 4413) y
a 512 KiB (18003 vs 18066). Pero eso es un argumento, no el control.

### Y lo que no se midió en toda la sesión

- **Nada sobre corpus real.** Todo son las 5 muestras sintéticas de 10 MB, y encima cortando con
  `head -c` **los primeros bytes** — no `muestra_repartida()`. En árboles de miles de ficheros
  juegan además la deduplicación, el reordenado y los carriles, que aquí valen cero.
- **Nada de reloj.** Ni siquiera lo que ahorra el cambio aplicado en §4.
- **Round-trip sólo en tres casos** (v6, v7 y `VC_SSE_W0=16384`), no en todas las configuraciones.
