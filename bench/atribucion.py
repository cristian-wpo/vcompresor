# -*- coding: utf-8 -*-
"""DE QUIEN ES CADA COSA. Una sola tabla de atribucion para todo el banco.

POR QUE EXISTE. La atribucion vivia DOS veces —dentro de bench/panel.py y dentro de
bench/propias.py— y las dos copias ya no decian lo mismo: las medidas de perfilar zstd
(`vc-huecos`, `vc-zstd-perfil`) eran de «codec-zstd» para el panel y de vlz para el censo. La
MISMA fila del libro mayor tenia dos duenos segun a quien preguntaras. Una atribucion que
depende de quien la mira no es una atribucion, es una opinion con formato de tabla.

NORMA DEL DUENO (2026-07-31): «cada herramienta debe de tener sus mediciones, sus caminos y todo
aparte, porque se miden por separado, y deben aparecer todas sin excepcion».
  · «aparte» exige que esto sea EXACTO: lo que aqui se atribuya mal, sale mal en un panel que
    lleva el nombre de otra herramienta encima.
  · «todas sin excepcion» exige que HERRAMIENTAS sea la lista COMPLETA y que nadie la filtre
    por tener datos o no. Una herramienta con cero medidas sale con su cero delante. El cero es
    el dato: dice que nunca ha pasado por la bascula. Esconderla porque no tiene banco propio
    fue justo lo que dejo a vlz fuera del panel el dia que nacio.

REGLA PARA ANADIR UNA HERRAMIENTA: una linea en HERRAMIENTAS y nada mas. El censo, la pagina
propia, el reparto del libro mayor, su camino y la verificacion salen de aqui. Si hace falta
tocar otro sitio, ese otro sitio es un fallo de este modulo.
"""

# ── LAS HERRAMIENTAS NUESTRAS, TODAS ─────────────────────────────────────────────────────────
# clave · nombre · que es · a quien sustituye · donde vive el codigo · pestana · resumen de una
# linea (el que se lee en el selector de paneles).
#
# Lo unico escrito a mano es QUE existe y QUE sustituye. Todo lo demas —sus medidas, sus
# preguntas, sus tablas— se LEE del banco. Una cifra escrita a mano aqui seria una cifra que
# nadie vuelve a comprobar.
HERRAMIENTAS = [
 ("archivador", "el archivador",
  "La tubería entera: recorrer, enrutar, transformar, trocear, deduplicar, comprimir y "
  "escribir. Es lo que el usuario ejecuta.",
  "—  (es nuestro por definición)", "crates/vc-core", "Pirámide",
  "La tubería entera: dónde se va el tiempo, qué carril se paga y contra quién ganamos."),
 ("vdeflate", "vdeflate",
  "Nuestro DEFLATE: desenvuelve el DEFLATE que ya traen los PNG, los .gz y los .zip para poder "
  "comprimirlo mejor, y lo vuelve a montar idéntico.",
  "preflate-rs 0.7.6  ·  flate2", "crates/vc-core/src/inflate.rs, deflate_emit.rs", "Vdeflate",
  "Nuestro DEFLATE: sus fases, su cobertura del formato y su suelo físico."),
 ("vlz", "vlz",
  "Nuestro códec LZ: el que de verdad convierte los bytes en menos bytes. Busca repeticiones y "
  "las codifica con un codificador aritmético adaptativo.",
  "zstd  (58.395 líneas de C)", "crates/vlz", "—  (recién nacida)",
  "Nuestro códec LZ, recién nacido: lo que se ha medido de él y lo que todavía no."),
 ("vc-cm", "vc-cm",
  "Nuestro códec de mezcla de contextos, el de `--max`. Predice cada bit combinando 43 modelos. "
  "Comprime mucho más y va mucho más lento.",
  "zstd, en el modo --max", "crates/vc-cm", "Modelos",
  "Nuestro códec de mezcla de contextos: sus 43 modelos y lo que aporta cada uno."),
 ("vc-cdc", "vc-cdc",
  "Nuestro troceador: parte los ficheros por el contenido y no por posición, que es lo que "
  "permite deduplicar entre ficheros parecidos.",
  "fastcdc", "crates/vc-core/src/cdc.rs", "Piezas",
  "Nuestro troceador por contenido: ya gana a fastcdc en ratio y en tiempo a la vez."),
 ("vc-formato", "vc-formato",
  "Nuestro serializador del índice y de los frames.",
  "bincode + serde", "crates/vc-core/src/formato.rs", "Piezas",
  "El formato del índice y del contenedor: qué cuesta y qué se reescribe de más."),
 ("vc-jpeg", "vc-jpeg",
  "Transcodifica JPEG a JXL y vuelve, bit a bit. Hoy se apoya en libjxl.",
  "libjxl (C++)", "crates/vc-core/src/jxl.rs", "Piezas",
  "El parser JPEG propio: su cobertura byte a byte y lo que le falta para soltar libjxl."),
 ("vc-cripto", "vc-cripto",
  "El cifrado del archivo.",
  "chacha20poly1305 + argon2  (se quedan)", "crates/vc-core/src/cripto.rs", "Piezas",
  "El cifrado: lo único donde NO queremos código propio, y por qué."),
 ("vc-cli", "vc-cli",
  "El intérprete de argumentos y la decisión por dinero (`--price`).",
  "clap", "crates/vc-cli", "—",
  "La interfaz y la decisión por dinero: qué carril elige `--price` y con qué medida."),
 ("banco", "el banco de pruebas",
  "Los guiones que miden, la báscula y este panel. No entra en el producto, pero decide todo lo "
  "que entra.",
  "—  (es nuestro por definición)", "bench/", "Métodos",
  "El instrumento: los métodos declarados, el libro mayor y todo lo medido en crudo."),
]

CLAVES = [h[0] for h in HERRAMIENTAS]
NOMBRE = {h[0]: h[1] for h in HERRAMIENTAS}
RESUMEN = {h[0]: h[6] for h in HERRAMIENTAS}

# ── UNA FILA DEL LIBRO MAYOR ─────────────────────────────────────────────────────────────────
# `mide.sh -H` anota la herramienta desde el 2026-07-31. Las filas ANTERIORES no se reescriben
# —el libro mayor es append-only y falsear el pasado seria peor que no tenerlo—: se DEDUCEN al
# leer, y el panel dice cuantas van declaradas y cuantas deducidas.
#
# EL CARRIL LZ: perfilar zstd (`vc-huecos`, `vc-zstd-perfil`) no mide codigo nuestro, mide al
# rival — pero es la medida que decide sobre vlz, asi que es de vlz. Se escribe el criterio
# aqui en vez de dejarlo en dos guiones que ya se contradijeron. Hoy son 0 filas: el dia que
# haya una, tendra dueno antes de existir.
PISTAS_CMD = (
    ("vc-inflate",     "vdeflate"),
    ("vc-redeflate",   "vdeflate"),
    ("vc-fase3",       "vdeflate"),
    ("vc-prediccion",  "vdeflate"),
    ("vc-cobertura",   "vdeflate"),
    ("vc-hueco3",      "vdeflate"),
    ("vc-huecos",      "vlz"),
    ("vc-zstd-perfil", "vlz"),
    ("vlz",            "vlz"),
    ("vc-cm",          "vc-cm"),
    ("vcompresor",     "archivador"),
)
# Nombres declarados que son alias de una clave. `codec-zstd` es como se llamo al carril LZ
# antes de que vlz tuviera nombre.
ALIAS = {"codec-zstd": "vlz", "vc-core": "archivador", "vc-core (tuberia)": "archivador"}

SIN_CLASIFICAR = "sin clasificar"


def herr_de_fila(r):
    """(clave, declarada) de una fila del libro mayor. `declarada` = lo dijo quien midio."""
    h = (r.get("herramienta") or "").strip()
    if h:
        return ALIAS.get(h, h), True
    c = r.get("cmd", "") or ""
    for pista, nombre in PISTAS_CMD:
        if pista in c:
            return nombre, False
    return SIN_CLASIFICAR, False


# ── UNA PIEZA DE LAS MATRICES DEL CAMINO ─────────────────────────────────────────────────────
# Las matrices nombran las piezas por su fichero, no por nuestra clave. Lo que no case se dice,
# no se reparte a ojo.
MAPA_PIEZA = {"vc-core (tuberia)": "archivador", "vc-inflate": "vdeflate",
              "vc-deflate": "vdeflate", "vc-cm": "vc-cm", "vc-cdc": "vc-cdc",
              "vc-formato": "vc-formato", "vc-jpeg": "vc-jpeg", "vc-cripto": "vc-cripto",
              "vc-cli": "vc-cli", "el banco": "banco", "vlz": "vlz"}


# ── UNA TABLA DEL BANCO ──────────────────────────────────────────────────────────────────────
# Por prefijo del nombre del fichero, que es como estan puestos. Lo que no case NO se reparte:
# se devuelve None y quien pregunte decide si eso es un hueco que hay que cantar.
#
# `principal` NO es una herramienta: es lo TRANSVERSAL, lo que habla del proyecto entero y no de
# una pieza —de que dependemos, que le falta para ser producto—. Tiene bucket propio para no
# tener que meterlo con calzador en «el banco», que es otra cosa: el banco es el instrumento.
TRANSVERSAL = "principal"
PREF_TABLA = (
 ("vdeflate",   ("vdeflate", "suelo", "descomprimir")),
 ("vlz",        ("vlz", "ventana", "codec-zstd", "zstd-no-monotono")),
 ("vc-cm",      ("vccm", "pir_vccm", "piramide-vccm", "codec-bpb", "tasa-", "ablacion",
                 "decay", "coste", "m2-", "m4-", "poda", "talla-poda")),
 ("vc-cdc",     ("cdc", "talla-coste", "talla-tabla")),
 ("vc-jpeg",    ("f2.3-jpeg", "jpeg")),
 ("archivador", ("carriles", "economia", "econivel", "piramide", "bucles", "perfil", "censo",
                 "nivel-zstd", "incremental", "instantaneas", "rotacion", "multibench",
                 "comparativa", "vs-restic", "rivales", "nuestro", "escala", "cruce",
                 "camino-comprimir", "f2.7", "f264", "f265", "memoria", "espera-memoria",
                 "guardia-memoria", "determinismo", "json-jsonl", "muchas-instantaneas",
                 "velocidad-real", "frames", "control-cero", "fases", "tiempo-plano",
                 # el carril ZIP, los corpus y la robustez son de la tuberia entera
                 "zip-", "silesia", "pdfoffice", "robustez", "ultimo-resultado", "vuelo-unico")),
 ("banco",      ("metodos", "preguntas", "auditoria", "desperdicio", "herramientas", "terceros",
                 "tecnicas", "precomputar", "paneles", "plan", "ruido", "sonda", "veredictos",
                 "tallas", "syscalls", "loc", "ejecutables", "camino", "coste-huellas")),
 # LAS JUNTURAS SON TRANSVERSALES POR CONSTRUCCION, y son el contrapeso de este mismo fichero:
 # una bascula por herramienta es ciega a lo que pasa ENTRE dos, que es donde vivio el fallo mas
 # caro del 31-07. Repartir por herramienta sin dejar sitio a la juntura seria cambiar un punto
 # ciego por otro. Van al principal, que es la unica pagina cuyo objeto es el conjunto.
 # «techos» es lo mejor que se sabe hacer en cada cosa que hacemos, venga de quien venga: no es
 # de ninguna pieza nuestra, es la vara con la que se miden todas.
 (TRANSVERSAL,  ("dependencias", "producto-que-falta", "junturas", "techos")),
)


def herr_de_tabla(nombre):
    """Clave de la herramienta dueña de una tabla del banco, o None si no se sabe."""
    for k, pref in PREF_TABLA:
        if any(nombre.startswith(p) for p in pref):
            return k
    return None


# ── UNA LINEA DE CODIGO ──────────────────────────────────────────────────────────────────────
# Para el censo del desperdicio, cuya columna «donde» ya es la ruta del fichero: la ruta ES la
# prueba de quien es. Se mira de lo mas concreto a lo mas general — cdc.rs antes que vc-core.
PREF_RUTA = (
 ("bench/",                     "banco"),
 ("crates/vlz",                 "vlz"),
 ("crates/vc-cm",               "vc-cm"),
 ("crates/vc-cli",              "vc-cli"),
 ("vc-core/src/cdc.rs",         "vc-cdc"),
 ("vc-core/src/formato.rs",     "vc-formato"),
 ("vc-core/src/cripto.rs",      "vc-cripto"),
 ("vc-core/src/jxl.rs",         "vc-jpeg"),
 ("vc-core/src/jpeg.rs",        "vc-jpeg"),
 ("vc-core/src/inflate.rs",     "vdeflate"),
 ("vc-core/src/deflate",        "vdeflate"),
 ("vc-cli/",                    "vc-cli"),
 ("vc-cm/",                     "vc-cm"),
 ("vc-core",                    "archivador"),
)


def herr_de_ruta(ruta):
    """Clave de la herramienta dueña de una ruta de codigo. `archivador` es el ultimo recurso
    solo para vc-core: fuera de ahi se devuelve None y se canta."""
    r = (ruta or "").strip()
    for pref, k in PREF_RUTA:
        if pref in r:
            return k
    return None
