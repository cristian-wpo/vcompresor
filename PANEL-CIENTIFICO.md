# El panel científico — estándar común de vcompresor y NERVIO

> Este fichero vive **copiado** en los dos repos, no importado. Los dos proyectos tienen
> que poder abrirse, medirse y defenderse por separado: un `import` compartido acopla el
> banco de un producto que ya vende con el de otro que está antes de una solicitud de
> patente. Se comparte el **criterio**, no el código.
>
> Última revisión: 2026-07-30.

## Para qué es un panel aquí

No para enseñar resultados. Para **sostener un método que todavía se está inventando**.

El método publicado de este campo tiene huecos que a nosotros nos tocan de lleno: no hay
protocolo para «cuánto cuesta reparar solo la región que se perdió», ni para «qué paga un
receptor que engancha tarde», ni para «cuánto fondo se queda quieto de verdad en una
videollamada». Medimos esas cosas igual, porque son las que deciden el producto. La
consecuencia es que **buena parte de lo que este banco mide no tiene vara ajena contra la
que compararse**, y entonces el panel es lo único que separa una medida de una corazonada
con decimales.

De ahí salen las diez reglas. Ninguna es estética.

---

## Las tres piezas

Tres instrumentos, y **cada uno ve exactamente lo que los otros dos no pueden**. Por eso son
tres y no uno con tres pestañas.

| pieza | qué mira | qué se le escapa |
|---|---|---|
| **CAMINO** | lo que nunca preguntamos a algo **que ya funciona** | no dice si algo empeoró ni cuánto cuesta |
| **AGUJA** | si algo **empeoró respecto a ayer** — la misma batería en cada commit | un gasto que lleva ahí desde el primer día nunca ha empeorado: no lo ve |
| **PIRÁMIDE** | **dónde se va el gasto**, con reloj y CPU separados y serie o paralelo | ve el gasto, no el desperdicio: un peldaño caro puede ser caro con razón |

La frase que las ata, y que justifica que CAMINO exista habiendo ya aguja y pirámide:

> *Ninguna línea que encuentra CAMINO es un fallo. Por eso no la encuentra nada de lo que ya
> teníamos: los tests miran la salida, la aguja mira si algo empeoró respecto a ayer, y la
> pirámide mira dónde se va el tiempo. **Un gasto que lleva ahí desde el primer día y nunca ha
> empeorado es invisible para las tres.***

**CAMINO — `bench/preguntas-camino.tsv` + `bench/desperdicio.tsv`.** Doce preguntas que se le
hacen a una pieza que ya funciona. No son agujeros (eso es `preguntas.tsv`): aquí no falta
ningún dato, lo que falta es haber preguntado. **Las doce viajan intactas entre proyectos** —
ese es el instrumento; si hubiera que reescribirlas para cada uno no serían preguntas, serían
hallazgos con otro nombre. Lo único que cambia por proyecto es `que_encontro_aqui`.

Y el censo que llenan tiene **la regla de las tres columnas**: `que_gasta_de_mas`, `cuanto` y
`camino_corto`. Sin las tres no es un hallazgo, es una opinión sobre el código de otro — el
panel lo marca. `cuanto` va medido, con su comando (R4).

Un censo vacío significa **«nadie ha preguntado»**, nunca «no hay desperdicio».

**PIRÁMIDE — `bench/piramide.tsv`.** Cada peldaño con **reloj y CPU por separado**, y si va en
serie o en paralelo: un peldaño paralelo y uno en serie no cuestan lo mismo aunque marquen el
mismo número — el paralelo gasta CPU, el de la serie gasta **espera**. Esa es toda la
diferencia entre saber dónde va el tiempo y saber dónde va la espera, y un perfil normal no la
enseña. Medido dentro de cada fase, no derivado de los tiempos de fuera.

---

## R1 · Nada medido es invisible

Toda fila del libro mayor llega a la pantalla, aunque el generador no sepa qué es. Un
`tipo_medida` nuevo aparece el día que se mide, **sin tocar el generador**, en la vista
*TODO LO MEDIDO* (en vcompresor, *CRUDO*).

La verificación **falla** —no avisa— si algún tipo de medida del libro mayor no llega a esa
vista. Un banco que esconde parte de lo que mide no es un banco, es una selección.

Y no vale con que una fila esté "citada": una pregunta que barre el libro mayor entero
(«¿alguna cifra no tiene comando?») está haciendo **higiene**, no ciencia. El panel separa
preguntas **transversales** de **específicas** con un umbral publicado en la propia página, y
solo las específicas cuentan como tener sitio propio.

## R2 · Un método es un objeto declarado, no una columna suelta

`bench/metodos.tsv`, una fila por `tipo_medida`:

| columna | qué es |
|---|---|
| `tipo_medida` | la clave que casa con el libro mayor |
| `nombre` · `origen` · `estado` | cómo se llama · `propio` \| `estandar` \| `propio-sobre-estandar` · `en-uso` \| `en-prueba` \| `retirado` |
| `que_mide` · `como_se_calcula` | qué cifra sale y por qué procedimiento |
| `unidad_principal` · `campo_principal` | la clave de `datos` que lleva la cifra de cabecera |
| `hueco_que_tapa` | **qué le falta al método estándar** para que este exista |
| `control` | qué lo sostiene: el control interno, el par congelado, la verdad sintética |
| `que_lo_falsaria` | qué resultado lo mataría |
| `fuente` | el guion o documento donde vive |

`que_mide`, `como_se_calcula`, `control` y `que_lo_falsaria` son **exigidas**. Un método
propio sin control es una corazonada con decimales, y uno que nada puede falsar no es una
medida, es una postura. Si faltan, el panel lo marca como *declaración a medias*.

Tres huecos se enseñan arriba del todo, y los tres son información:

- **en uso sin declarar** — se está midiendo algo cuya definición no existe.
- **declaración a medias** — falta el control o falta qué lo falsaría.
- **declarado y nunca medido** — hipótesis parada; dice por dónde seguir.

Inventar método es legítimo. No escribirlo, no.

## R3 · La incertidumbre viaja pegada a la cifra

Toda cifra agregada sale con su `n` y su dispersión, y la regla de honestidad vive **dentro**
de la función que agrega, no en quien la llama:

- `n = 1` → no hay dispersión que enseñar, y se dice.
- `n = 2` → rango sí, desviación no (con dos puntos la sigma es un adorno).
- `n ≥ 3` → desviación típica muestral.

Un delta por debajo del suelo de ruido se pinta **apagado**, nunca en negrita. Y el suelo de
ruido **se lee del propio experimento** (los controles de cero que ya están en la tanda), no
se estima a ojo: estimarlo a ojo ya salió mal una vez —±0,62 % publicado contra ±1,3 % real,
con seis controles de cero delante sin reconocerlos.

## R4 · Ninguna cifra sin su comando

Cada fila del libro mayor trae `comando` literal, `commit`, `maquina`, la carga al medir
(`uptime`) y su `fuente` (fichero + línea). El panel cuenta cuántas filas incumplen y
comprueba que **el fichero de origen siga existiendo**. No mide que el comando vuelva a dar
el mismo número: mide que esté **escrito** cómo reproducirlo, que es justo lo que se pierde a
los tres meses.

## R5 · Las contradicciones se conservan a la vista

Cuando dos agregaciones legítimas dan cifras distintas (`n` distinto, filtro distinto), se
publican **las dos** con el motivo, y se abre una pregunta. No se elige la que gusta. El
registro de preguntas de NERVIO ya lo hace: *«DISCREPANCIA de n conservada… no se elige
ninguna de las dos»*.

## R6 · Lo descartado con medida no se borra

Una idea probada y descartada se queda en el registro con su número y su fecha, en estado
`descartada`. Sin eso se vuelve a intentar dentro de dos meses. vcompresor tiene ahí sus
negativos caros (tablas más pequeñas: −74 % de RAM → **0 %** de velocidad).

## R7 · Nunca enseñar lo viejo como si fuera de ahora

Si el panel se sirve y el generador deja de responder, el indicador se pone **rojo**. Un
panel que sigue pintando datos de hace seis horas como si fueran de ahora es peor que no
tener panel: es una mentira educada.

## R8 · Determinista: mismos datos, mismos bytes

Dos generaciones sobre las mismas entradas producen ficheros **idénticos**. Ningún reloj de
pared dentro del HTML. Así un `git diff` del panel significa siempre *«han entrado medidas
nuevas»* y nunca *«se ha regenerado»*.

El reloj de pared vive en el acompañante `datos.json`, que por eso **no se versiona**.

## R9 · Se abre solo, sin nada instalado

Un fichero, doble clic, sin red, sin CDN, sin build. La verificación falla si aparece una
sola referencia externa. El panel tiene que abrirse igual dentro de un tar de hace tres
meses: un panel que necesita infraestructura para verse es un panel que dentro de un mes
nadie mira.

**El modo en vivo es una mejora opcional encima de eso, nunca un requisito**: servido por
http y con `datos.json` al lado, la página se repinta en sitio cada 8 s comparando el sello
(sha256 de las entradas) y conserva scroll, pestaña y selección. Sin el acompañante, o
abierto como `file://`, el panel es exactamente el de siempre.

## R10 · El criterio se publica, no se esconde

Cada umbral que decide un color o una etiqueta va **escrito en la propia página**: el umbral
de contraste (4,5:1 de WCAG AA, verificado en generación — si alguien baja un color, la
generación **falla**), el umbral de pregunta transversal, el suelo de ruido y de dónde sale.
Un criterio que no se puede leer no se puede discutir.

## R11 · Las herramientas están conectadas, con ida y vuelta perfecta

**La ley, del dueño (2026-07-31):** *todas las herramientas deben estar conectadas y pasarse la
información de forma perfecta, ida y vuelta.*

Tiene dos filos y los dos se comprueban:

- **Conectadas** — ninguna herramienta recalcula lo que otra ya calculó, y ninguna decide con
  menos de lo que la de al lado ya sabe.
- **Ida y vuelta perfecta** — lo que A le pasa a B vuelve idéntico, y eso se **demuestra**. Un
  «debería volver» no es una ida y vuelta.

**Por qué hace falta una regla para esto y no basta con medir bien cada pieza.** El fallo más caro
encontrado hasta hoy no vivía *dentro* de ninguna herramienta: el archivador elegía frames de
64 MiB y le pasaba al códec una llamada sin un solo parámetro, así que el códec usaba su ventana
por defecto de 4 MiB. Dieciséis a uno. Las dos piezas medían bien por separado —el archivador da
sus bytes y su reloj, zstd hace exactamente lo que le piden— y el número que las descuadra **no
sale en ninguna de las dos básculas**. Una báscula por herramienta es ciega a la juntura por
construcción.

**Cómo se comprueba.** `bench/junturas.py`, y tiene dos partes:

1. **El detector, automático.** Casi toda librería de fuera tiene dos caras: una función de un
   disparo que se lo traga todo por defecto y una versión configurable. Llamar a la de un disparo
   es, literalmente, no pasarle lo que sabemos. El guion **recorre el código** y las señala. Una
   lista escrita a mano no se habría enterado — de hecho el mismo detector, la primera vez que se
   corrió, encontró que la sonda de `--price` había quedado prediciendo un producto que ya no
   existía.
2. **El censo.** Una fila por traspaso: qué debería cruzar, si cruza, si la ida y vuelta está
   demostrada y **con qué**, y cuánto cuesta el hueco. Un coste con cifras y sin medida al lado
   **no entra**: el generador aborta.

## R12 · El techo conocido, y si podemos pasarlo

Las preguntas del CAMINO preguntan *«¿lo estamos haciendo bien?»*. Ninguna preguntaba *«¿y cuánto
es lo mejor que se sabe hacer?»*. Sin esa, se puede estar puliendo una pieza que ya iba por
delante de todo el mundo, o peleando contra un muro que resulta ser **de otro** y no de la física.

**Dos reglas de admisión, y las dos abortan la generación:**

1. **Un techo sin fuente no es un techo.** Si no se puede decir de dónde sale el número, es un
   rumor con formato de dato. Y la fuente preferida es *medido aquí*, no citado de un README.
2. **Las dos cifras tienen que ser la misma magnitud.** Esta nace de un error: la fase 3 de
   vdeflate se comparó como «13,14% nuestro contra 1,70% de preflate» y de ahí salió un *no se
   construye*. Pero el 13,14% era una **cota superior** de un modelo de coste que cobra las
   correcciones a tarifa plana —8 bits por fallo, falle por 1 byte o por 200— y el 1,70% es el
   **resultado real** de preflate con esas correcciones codificadas por entropía adaptativa. Dos
   números correctos, ninguno comparable con el otro, y una decisión de no construir tomada
   encima. **Una fila que no puede jurar que compara lo mismo no da veredicto.**

Y hay un tercer estado que hay que poder escribir: **el techo que nadie sabe**. El límite real de
un dato es su complejidad de Kolmogorov, que es incomputable. Decirlo importa, porque significa
que nunca habrá una prueba de que ya no queda nada por ganar: la única pregunta contestable es la
comparativa.

Se comprueba con `bench/techos.py`.

---

## Contrato de datos

```
bench/historia.jsonl   una línea por medida:  tipo_medida · fecha_utc · commit · maquina ·
                       comando · uptime · fuente{fichero,linea,blob} · id_fila · datos{}
bench/metodos.tsv      el registro de métodos (R2)
bench/preguntas.tsv    el registro de preguntas; `filtro_historia` se evalúa EN GENERACIÓN
                       y el panel embebe las filas exactas que responden a cada pregunta
bench/panel.html       el panel, versionado (R8)
bench/datos.json       acompañante del modo en vivo, NO versionado (R7/R8)
```

## Cómo se comprueba cada regla

```bash
python3 bench/genera_panel.py --datos     # genera panel + acompañante
python3 bench/genera_panel.py --verifica  # R1, R2, R9: vistas, cobertura total, cero red
# R8, determinismo:
python3 bench/genera_panel.py && sha256sum bench/panel.html   # dos veces, mismo sha
```

## Estado (2026-07-30)

| regla | NERVIO | vcompresor |
|---|---|---|
| R1 nada invisible | ✅ *TODO LO MEDIDO* + verificación que falla | ✅ 30-07: carga genérica + tablas vacías visibles + la verificación falla si aparece un formato nuevo |
| R2 registro de métodos | ✅ 11 declarados, 1 a medias | ✅ 30-07: pestaña *9 · MÉTODOS* — 13 declarados y **56 midiendo sin declarar** (la cola de trabajo, ahora a la vista) |
| R3 incertidumbre | ✅ n / rango / desviación con regla de honestidad | parcial (suelo de ruido en la vista de modelos) |
| R4 comando y fuente | ✅ 0 filas sin comando, fuentes comprobadas | parcial (auto-auditoría de tablas, no de filas) |
| R5 contradicciones | ✅ en el registro de preguntas | parcial |
| R6 negativos | parcial | ✅ estado `descartada` en el registro |
| R7 anti-obsolescencia | ✅ indicador rojo | ✅ (de aquí salió) |
| R7b una sección rota no tumba la página | ✅ 12 vistas blindadas | ✅ 26 secciones blindadas |
| R8 determinismo | ✅ verificado byte a byte | ✅ 30-07: fuera el reloj, la carga y los procesos en vuelo; sello = sha256 del contenido |
| R9 autocontenido | ✅ | ✅ |
| R9b enlaces profundos estables | ✅ `#camino`, `#metodos`… | ✅ (el `#pN` viejo mentía al reordenar) |
| **verificación que falla** | ✅ `--verifica` | ✅ 30-07: 6 comprobaciones, probadas contra 6 paneles rotos |
| R10 criterio publicado | ✅ contraste + transversalidad | parcial (suelo de ruido) |
| **CAMINO** | ✅ portado 30-07 · 4/12 preguntadas, censo vacío | ✅ **de aquí salió** · 12/12, censo con hallazgos |
| **AGUJA** | ✅ *AGUJA* + *Aguja por commit* | ✅ |
| **PIRÁMIDE** | vista puesta, tabla **vacía a propósito** (falta cronometrar la tubería) | ✅ 14 fases medidas con `VC_FASES=1` |
