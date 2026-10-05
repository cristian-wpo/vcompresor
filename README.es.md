# vcompresor

Archivador que **deshace los formatos comprimidos en vez de tragárselos**, y que devuelve
los ficheros byte a byte.

Un archivador normal ve un PNG, un `.docx` o un `.zip` como bytes opacos y no puede hacer nada
con ellos: ya vienen comprimidos. vcompresor los **despliega** —deshace el DEFLATE, deshace el
JPEG—, comprime lo de dentro con un modelo de contextos propio, y al extraer los **vuelve a
montar exactamente igual**, byte a byte. Ahí es donde está la diferencia grande, y es una
diferencia que los archivadores clásicos no pueden alcanzar sin implementar lo mismo.

```
900 ficheros de código, comprimidos dentro de un zip -9 de 2.130.467 bytes:

  vcompresor        854.908     ← despliega el zip y modela lo de dentro
  zstd -19        1.959.852
  xz -9           1.977.492
  el zip original 2.130.467

y devuelve EL MISMO ZIP, byte a byte.
```

## Tres propiedades, en orden de importancia

**1. Recompresión reversible.** DEFLATE (PNG, gzip, ZIP y todo lo que va en zip: docx, xlsx,
pptx, jar, apk, epub…) y JPEG (con parser propio: línea base y progresivo). Un flujo que no se
pueda desplegar se queda literal y no arrastra al resto del fichero. Y la reconstrucción se
**verifica al comprimir**: si no sale byte-idéntica, ese fichero va tal cual. Nunca se cambia
correctitud por ratio.

**2. Salida byte-idéntica en cualquier arquitectura.** No hay una sola operación de coma
flotante en el camino de la probabilidad. Comprimir el mismo dato en x86 y en ARM da el mismo
SHA-256, y cada uno descomprime lo del otro. Para archivo verificable, almacenamiento por
contenido o cumplimiento normativo, eso no es un detalle. paq8px y cmix no lo tienen.

**3. Se monta.** `vcompresor montar archivo.vc /punto` y navegas dentro sin desempaquetar.

Y de fondo: deduplicación por contenido (troceado de tamaño variable + blake3), reordenado de
ficheros por parecido antes de empaquetar, un carril que saca el timestamp de cabecera de los
logs y lo guarda en diferencias, tres modelos de coincidencias (dos seguidos y uno a
zancadas, para dato con registros), un filtro x86 para ejecutables, y extracción en paralelo.

## Los números

Dos comparaciones, y las dos importan por motivos distintos:

- **zpaq -m5** es el rival de nuestra *categoría*: coge árboles de directorios, deduplica, es
  un producto real. Es contra quien hay que medirse para decir «mejor archivador».
- **paq8px -8** es el techo de *ratio*. Come un fichero suelto, va 39× más lento y usa 2,4 GB.
  Es contra quien hay que medirse para saber cuánto falta.

Los dos medidos en esta máquina, no copiados de ninguna tabla publicada.

### Muestras de 10 MB — aquí nuestras ventajas de archivador valen CERO

Sin deduplicación entre ficheros, sin reordenado por parecido, sin carriles de recompresión.
Es sólo el modelo contra el modelo, y es el terreno que peor nos trata:

<!-- MEDIDO: texto5-max -->
| muestra | vcompresor `--max` | zpaq -m5 | | paq8px -8 | |
|---|---|---|---|---|---|
| código | **787.470** | 869.317 | -9,4% | 661.228 | **+19,1%** |
| CSV/JSON | **540.431** | 666.592 | -18,9% | 465.439 | **+16,1%** |
| enwik8 | **2.088.907** | 2.187.162 | -4,5% | 1.865.161 | **+12,0%** |
| logs | **48.823** | 75.717 | -35,5% | 31.385 | **+55,6%** |
| SQL | **226.104** | 351.320 | -35,6% | 158.372 | **+42,8%** |
| **total** | **3.691.735** | 4.150.108 | **-11,0%** | 3.181.585 | **+16,0%** |
<!-- /MEDIDO · bench/readme.py desde bench/historia.jsonl · árbol d04cee7df2c5 · 2026-08-03 -->

Le ganamos a zpaq en las cinco y **paq8px nos gana un 21,9%**. Y fíjate dónde: el hueco es
*mayor* donde el dato es *más predecible* (SQL y logs, +71%), lo cual no es falta de modelos
sino de **calibración** — él encadena nueve etapas de SSE donde nosotros teníamos una.

### Corpus reales, árboles de miles de ficheros

Aquí sí juegan la deduplicación, el reordenado y los carriles. **zpaq -m5 medido en la misma
máquina y en la misma tanda**:

| corpus | entrada | vcompresor `--max` | zpaq -m5 | |
|---|---|---|---|---|
| código | 136.415.253 | **9.657.771** | 10.901.842 | −11,41% |
| logs | 262.120.432 | **3.233.152** | 3.716.176 | −13,00% |
| SQL | 157.277.338 | **8.205.349** | 8.981.541 | −8,64% |
| CSV/JSON | 117.435.995 | **1.618.499** | 1.828.613 | −11,49% |
| **total** | 673.249.018 | **22.714.771** | 25.428.172 | **−10,67%** |

Y en **Silesia**, el corpus público de referencia —doce ficheros sueltos, sin nada repetido
entre ellos, o sea el terreno donde la deduplicación no vale de nada y sólo cuenta el modelo:

| herramienta | bytes | ratio |
|---|---|---|
| **vcompresor `--max`** | **39.809.371** | **18,78%** |
| zpaq -m5 | 39.865.426 | 18,81% |
| 7z -mx9 | 48.715.234 | 22,99% |
| xz -9 | 48.776.804 | 23,01% |

Ahí el margen es estrecho a propósito de contarlo: **un 0,14%**. Silesia es doce ficheros
sueltos y binarios en buena parte, o sea justo donde nuestras ventajas no juegan. Y hay un
intercambio que conviene decir: la última poda de modelos gana un 0,15% en los corpus reales y
**pierde un 0,10% aquí**.

Verificado extrayendo el archivo entero y comparando con `diff -rq` contra el original: cero
diferencias en 27.804 ficheros del corpus real y en los 12 de Silesia, incluidos symlinks,
permisos, fechas y directorios vacíos.

En el corpus de código, contra el resto del campo: 7z -mx9 12.920.611 · xz -9 13.416.612 ·
mkdwarfs -l9 13.428.952 · zstd -19 --long 14.028.962.

### La velocidad, medida hoy

Este párrafo ha estado **mal publicado dos veces**, y las dos por lo mismo: una cifra
pública escrita a mano caduca y nadie se entera. Primero decía «~5× más lentos que zpaq, 1.156
segundos contra 220», medido con el servidor saturado por otro trabajo (carga media 8). Se
corrigió con la máquina en reposo y alternando A/B/A/B —145,0 s contra 132,8 el 28-07, de donde
salía «entre un 8 y un 11% más lentos»— y **el 01-08 el reloj se triplicó** y volvió a quedarse
vieja sin que nadie lo notara.

Así que esta tabla ya no se escribe: **la genera `bench/readme.py` leyendo
`bench/historia.jsonl`**, y lo que no sale del libro mayor no se publica.

<!-- MEDIDO: velocidad-codigo -->
| | bytes | segundos | |
|---|---|---|---|
| **vcompresor `--max`** | **9.053.556** | 740,8 | **5,6× más lento que zpaq** |
| vcompresor, por defecto | 13.843.167 | 22,4 | 6,0× más **rápido** que zpaq |
| zpaq 7.15 `-m5` | 10.901.842 | 133,3 | |

**Somos 5,6 veces más lentos que zpaq**, y es reciente: el 28-07 la misma medida daba 145,0 s contra 132,8. El **01-08 el reloj se triplicó** para comprar un 1,5% de bytes, y está medido y explicado en `docs/f4-velocidad-por-que-son-mas-rapidos.md`.
<!-- /MEDIDO · bench/readme.py desde bench/historia.jsonl · árbol d04cee7df2c5 · 2026-08-03 -->

Memoria: ~1,1 GB en `--max`.

## A quién NO le ganamos

Esto importa tanto como lo de arriba.

- **paq8px nos gana un 21,9%** en las muestras de 10 MB (tabla arriba). Es el rey de la mezcla
  de contextos determinista y lo hace **sin usar su red neuronal** — su LSTM es una opción
  aparte que `-8` ni activa. El precio que paga: 0,008 MB/s y 2,4 GB de RAM, o sea 39× nuestro
  tiempo. Y `-8` ni siquiera es su máximo: llega a `-12` con 29 GB.
  - Su ventaja es de **capacidad de modelo**, no de recursos: ~500 entradas de mezclador
    contra nuestras 33, con `IndirectModel` de 27 contextos contra nuestros 2 y un `DmcForest`
    de 10 modelos contra nuestro 1. Comprobado: cuadruplicarnos la memoria nos da 0,07%, y él
    a `-4` (660 MB, la mitad de la nuestra) nos sigue ganando un 18,2% en código.
- **Vídeo y audio.** No hay carril para H.264, AAC ni MP3. Un códec especializado nos gana.
- **`.rar` y `.7z`.** No deshacemos ni RAR ni LZMA: van tal cual con un 0,01% de sobrecoste.
  Si vas a archivarlos, desempaquétalos antes.
- **El modo por defecto** es rapidísimo pero comprime a nivel zstd, o sea que pierde contra 7z
  y xz. **La tabla de arriba es sólo del modo `--max`.**
- **No encoge lo que ya está comprimido.** Sobre vídeo, JPEG o un `.zip` el archivo será
  *ligeramente mayor* que la entrada: el suelo son 118 B más ~25 B por fichero, y sobre dato
  incompresible nunca se baja del 100%. Le pasa a todo archivador; aquí al menos se avisa por
  pantalla cuando ocurre.
- **No escribe a la nube.** Sólo a disco local. restic y borg hablan con S3 y con SFTP; aquí
  subir el fichero es cosa tuya.
- **`mount` sólo en Linux** (necesita FUSE) y **la interfaz gráfica sólo para Mac**.

## Compruébalo tú

```bash
bash bench/run.sh
```

Descarga el corpus público de su fuente original, compila, mide a vcompresor y a los rivales
**en la misma máquina y en la misma ejecución**, y comprueba cada round-trip con `diff -rq`
contra el original — no con un hash que nos damos a nosotros mismos. Si algo falla, la tabla
lo dice.

## Uso

```
vcompresor compress <entrada>... -o copia.vc        # guardar
vcompresor list     copia.vc                        # ver qué hay dentro
vcompresor extract  copia.vc -o dir                 # recuperarlo todo
vcompresor extract  copia.vc -o dir --file "*.pdf"  # recuperar sólo eso
vcompresor add      copia.vc <entrada>...           # otra instantánea
vcompresor forget   copia.vc <instantánea>          # borrarla y compactar
vcompresor verify   copia.vc                        # comprobar sin extraer
vcompresor mount    copia.vc <punto>                # navegar dentro (sólo Linux)
```

`<entrada>` se puede repetir: `compress a.mp4 b.mp4 notas/ -o copia.vc` guarda esas tres cosas
y nada más, con las rutas colgando de la carpeta que las contiene a todas. Una carpeta sola
guarda su contenido, como siempre.

Los nombres en español (`comprimir`, `descomprimir`, `añadir`…) siguen valiendo como alias.

`--max` usa el modelo de contextos propio en vez de zstd: comprime bastante más y va bastante
más lento. Es el modo con el que están medidas las tablas de arriba.

**Si no sabes cuál te conviene, dilo en dinero y lo elige él:**

```
vcompresor compress /datos -o copia.vc --price 5.55 --retention 36
```

Sondea **tu** dato, pone precio a cada ajuste —el disco más la CPU de conseguirlo— y usa el más
barato, enseñando la tabla entera. Medido: a precio S3 conviene el nivel 19; a precio Backblaze
el 17; en disco propio barato el 9, y ahí el 19 **pierde** dinero.

Otras opciones: `--exclude PATRÓN` (repetible) para saltar `node_modules` y compañía, y
`--solid` para frames grandes en datos muy repetitivos.

### Instantáneas

`add` guarda otra copia del mismo árbol reutilizando todo lo que no cambió. Sobre 403 MiB, la
segunda copia de un árbol idéntico añade **−24 B** (restic +821, borg +1.268), y con rotación
realista —inserciones, apéndices, ficheros nuevos— añadimos **4,23× menos que restic**.

`forget` borra una instantánea **y compacta**: en la prueba liberó el 98,8% del archivo. Sin eso
no hay política de retención posible y el archivo crece para siempre.

### Cifrado

```
vcompresor compress /datos -o copia.vc --encrypt
```

ChaCha20-Poly1305 con clave derivada por Argon2id. Se cifran los frames **y el índice**: éste
lleva las rutas de todos los ficheros, y un respaldo que filtra el árbol de directorios no está
cifrado de verdad.

La contraseña se pide por el terminal —sin eco, y dos veces al crear—, o se toma de
`--password-file` o de `VC_PASSWORD`. **Nunca como argumento de línea de comandos**: sería
visible en `ps` para cualquier usuario de la máquina y quedaría en el historial del shell.

Sin esa contraseña no hay forma de recuperar nada. **No hay recuperación, por diseño.**

### Interfaz gráfica (macOS)

En `app-macos/` hay una app nativa: arrastras una carpeta —o varios archivos sueltos, y guarda
esos y no la carpeta que los contiene— o abres un `.vc` y ves qué hay dentro con buscador para
recuperar uno o varios ficheros. La barra de avance va por bytes, no por número de ficheros. `bash app-macos/empaqueta.sh`
la construye. Va firmada *ad-hoc*: para repartirla a terceros haría falta cuenta de
desarrollador de Apple y notarización.

### Lo que conserva

Permisos, fechas, enlaces simbólicos, **enlaces duros** (comparten inodo al restaurar),
**ficheros dispersos** (siguen dispersos en vez de inflarse), **dueño y grupo**, fifos y
dispositivos, y **atributos extendidos** — en macOS ahí viven el comentario del Finder y las
etiquetas de color; en Linux, SELinux y las *capabilities* de los ejecutables.

## Formato y compatibilidad

`VCMP0016`. Se leen todos los anteriores desde `VCMP0012`, y eso está verificado en cada cambio
construyendo el binario *anterior* aparte para producir un archivo viejo de verdad. Los archivos
ya escritos siguen abriéndose: es una promesa, no una intención.

## Estado

Proyecto de I+D. Los round-trips se verifican en cada paso —fuzzer propio, corpus reales,
`diff -rq`— pero no lo he sometido todavía a años de uso ajeno. Si vas a confiarle datos que
no tengas en otro sitio, no lo hagas aún.

Dependencias: mínimas y a propósito. Sin serde, sin clap, sin walkdir, sin fastcdc. Se
mantienen blake3 (una red de seguridad hecha en casa que falle sutilmente certificaría como
bueno un round-trip roto), zstd, y libjxl/preflate para los carriles de recompresión.

Documentación técnica de cada paso, con las mediciones y **los experimentos que no
funcionaron**, en [`docs/`](docs/).

---

## Licencia

vcompresor está bajo **doble licencia**:

- **[AGPL-3.0](LICENSE)** — gratis, para siempre, con una condición: si distribuyes un programa
  que lo incorpore, **o si lo ofreces a través de una red**, tienes que publicar el código
  fuente de tu programa bajo la misma licencia. Uso interno, investigación, herramientas
  propias y software libre están cubiertos sin obligaciones añadidas.
- **[Licencia comercial](LICENSE-COMMERCIAL.md)** — de pago, sin obligación de publicar nada.
  Para productos propietarios, servicios en red y motores empotrados.

**Comprimir tus ficheros no te obliga a nada.** La licencia habla del programa, no de lo que
produzcas con él: los `.vc` que generes son tuyos y no arrastran ninguna condición.

Las aportaciones van bajo el **[CLA](CLA.md)**, que es lo que permite que la doble licencia
funcione. Es una **cesión** de los derechos de explotación: la titularidad queda concentrada y
auditable. A cambio se te devuelve una licencia sobre tu propia aportación tan amplia como
permite la ley, así que **puedes seguir usando tu código donde quieras**, incluso en productos
cerrados. Tus derechos morales de autor no se tocan: son irrenunciables y siguen siendo tuyos.

Dependencias de terceros y una declaración explícita sobre paq8px —del que **no se ha copiado
ni derivado código**— en [NOTICE](NOTICE).
