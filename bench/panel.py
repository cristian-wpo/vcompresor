#!/usr/bin/env python3
"""EL PANEL. Lee todo lo que hay en bench/ y saca UNA pagina html sin dependencias.

    python3 bench/panel.py            # -> bench/panel.html

No hay servidor, no hay build, no hay red. La pagina lleva sus datos dentro, asi que se abre
igual en el Mac que en el movil que dentro de un tar de hace tres meses, y sigue diciendo lo
mismo. Eso importa mas de lo que parece: un panel que necesita infraestructura para verse es
un panel que dentro de un mes nadie mira.

Fuentes (todas opcionales; lo que falte sale marcado como pendiente, no inventado):
    bench/historia.jsonl   una linea por ejecucion, la escribe bench/mide.sh
    bench/rivales.tsv      cache de rivales — se mide una vez, no cambia nunca
    bench/ablacion.tsv     que aporta y que cuesta cada grupo de modelos
    bench/perfil.tsv       donde se va el tiempo, por funcion
    bench/censo.tsv        tamano y ocupacion real de cada tabla
    /tmp/mapa2.json        el mapa de bits (bench/extrae_mapa.py)
"""
import hashlib, json, os, re, subprocess, sys, time, glob

RAIZ = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
B = os.path.join(RAIZ, "bench")
sys.path.insert(0, B)
import atribucion as ATR      # de quien es cada medida, cada tabla y cada linea de codigo
import comun_banco as CB      # las reglas de que cuenta como dato, compartidas con decide.py
SOLO_VERIFICA = "--verifica" in sys.argv

def lee(p, d=""):
    try: return open(os.path.join(B, p), encoding="utf-8").read()
    except OSError: return d

def tsv(p):
    """Filas de un tsv, saltando comentarios y lineas vacias. Primera fila util = cabecera."""
    filas = [l.split("\t") for l in lee(p).splitlines()
             if l.strip() and not l.lstrip().startswith("#")]
    return filas

def sh(c):
    try: return subprocess.run(c, shell=True, cwd=RAIZ, capture_output=True,
                               text=True, timeout=30).stdout.strip()
    except Exception: return ""

# Lo que cambia solo por mirar el reloj o la maquina: NO entra en el html (ver mas abajo).
VIVOS = ("carga", "vuelo")

# Los .jsonl que el panel sabe pintar, cada uno en su vista. Se declaran para que el dia que
# aparezca otro la verificacion lo cante en vez de dejarlo invisible.
JSONL_CUBIERTOS = ("historia.jsonl", "aguja.jsonl")

D = {}
# El reparto de secciones en pestañas. Si una seccion nueva no aparece aqui, cae en CRUDO:
# visible, no perdida. Un reparto que tira lo que no reconoce seria peor que no tenerlo.
# Las pestañas se numeran SOLAS por su posicion. Estuvieron con el numero escrito dentro
# de la etiqueta y al meter una nueva delante salieron dos «2»: un numero que hay que
# acordarse de cambiar a mano acaba mintiendo el dia que alguien no se acuerda.
_PEST = [('Herramienta', 'LA HERRAMIENTA'), ('Linea', 'LÍNEA DE TIEMPO'),
         ('Nuestras', 'NUESTRAS HERRAMIENTAS'), ('Agujeros', 'AGUJEROS'),
         ('Camino', 'CAMINO'), ('Vdeflate', 'VDEFLATE'), ('Piezas', 'PIEZAS'),
         ('Piramide', 'PIRÁMIDE'), ('Velocidad', 'VELOCIDAD'), ('Rivales', 'RIVALES'),
         ('Modelos', 'MODELOS'), ('Aguja', 'AGUJA'), ('Metodos', 'MÉTODOS'),
         ('Crudo', 'CRUDO')]
PESTANAS = [(k, f'{n+1} · {e}') for n, (k, e) in enumerate(_PEST)]
_R0 = {'Agujeros': 0, 'Cómo leer este panel': 0, 'Las piezas: lo de fuera y lo nuestro': 1, 'Nuestras herramientas': 1, 'La pirámide': 2, 'La pirámide de vc-cm — nuestra alternativa a zstd': 2, 'Los bucles, en orden': 2, 'Dónde se va el tiempo': 2, 'Velocidad real contra la que decimos tener': 3, 'El dinero': 3, 'Contra todos los archivadores conocidos': 4, 'Contra los rivales': 4, 'El carril ZIP': 4, 'Libro mayor de modelos': 5, 'Mapa de bits': 2, 'La aguja': 6, 'Robustez y puntos ciegos': 6, 'Precisión del propio banco': 6, 'Todas las medidas': 8, 'Ejecuciones': 8, 'Inventario': 8}
# Todo lo que estaba de la 1 en adelante baja un puesto para hacer sitio a CAMINO en la 1.
REPARTO = {k: (v if v == 0 else v + 1) for k, v in _R0.items()}
# Todo baja un puesto mas: NUESTRAS HERRAMIENTAS entra la primera, porque la primera pregunta
# que se le hace a este panel es «¿que hemos hecho nosotros y como va?».
REPARTO = {k: v + 1 for k, v in REPARTO.items()}
# VDEFLATE entra en la 2: todo lo que estuviera de la 2 en adelante sube un puesto.
REPARTO = {k: (v if v <= 1 else v + 1) for k, v in REPARTO.items()}
REPARTO['El registro de métodos'] = 10
REPARTO['Nuestras herramientas, una por una'] = 0
REPARTO['Las junturas'] = 0
REPARTO['El techo conocido'] = 0
REPARTO['Las preguntas del camino'] = 2
REPARTO['El resultado es correcto, el camino no'] = 2
REPARTO['vdeflate — nuestro DEFLATE'] = 3
REPARTO['Dónde se han hecho ya'] = 2
REPARTO['Cinco formas de ir más rápido'] = 2
REPARTO['¿Qué carril se paga, y a qué precio?'] = 5
REPARTO['Precomputar: cuándo sí y cuándo empeora'] = 2
REPARTO['El libro mayor, agregado y auditado'] = 10
REPARTO['Cómo se resuelve'] = 2
# LA HERRAMIENTA entra la primera de todas y empuja al resto un puesto. Es la pestaña que solo
# existe en las paginas de herramienta: su ficha, su camino, sus medidas y sus tablas. En el
# principal no hay ninguna seccion que caiga aqui, asi que la pestaña no llega a dibujarse — que
# es como tiene que ser: una pestaña vacia promete algo que no esta.
REPARTO = {k: v + 1 for k, v in REPARTO.items()}
REPARTO['La herramienta, de un vistazo'] = 0
# Y LA LINEA DE TIEMPO entra la segunda, en todas las paginas. Es la pregunta que no contestaba
# ninguna vista: las medidas decian «cuanto cuesta HOY» y la aguja «cambio algo respecto a ayer»,
# pero no habia donde ver el recorrido — que commit mejoro el producto y cual lo empeoro.
REPARTO = {k: (v if v < 1 else v + 1) for k, v in REPARTO.items()}
REPARTO['Qué mejoró y qué empeoró, commit a commit'] = 1
# En la misma pestaña y DEBAJO de la curva de deltas, porque es la pregunta que esa curva deja
# sin contestar: un recorrido sin la posicion absoluta no dice si vamos por delante o por detras.
REPARTO['Contra los rivales, en el tiempo'] = 1
# En la MISMA pestaña que la linea de tiempo y ENCIMA de ella, porque contesta lo que uno viene a
# preguntar antes de mirar la curva: en que version estamos y que ha entrado. Y porque cuando la
# curva sale plana, la explicacion tiene que estar al lado, no en la cabeza de nadie.
REPARTO['Qué está aplicado y en qué versión estamos'] = 1
# LA PRIMERA DE LA PESTAÑA, encima incluso de las versiones. Es la unica que contesta «¿esto que
# se midio, en que quedo?», y esa pregunta va antes que cualquier cifra: una medida sin decision
# es una anecdota, y una decision sin medida es una corazonada.
REPARTO['Qué se decidió, y con qué medida'] = 1
# EL RELOJ DE PARED NO ENTRA EN EL HTML. Si entra, cada regeneracion cambia el fichero
# aunque no haya entrado ni una medida nueva, y entonces un `git diff` del panel ya no
# significa "hay datos nuevos" sino "he vuelto a ejecutar el generador" — que es no
# significar nada. El sello de abajo es del CONTENIDO; la hora vive solo en datos.json,
# que es el acompanante del modo en vivo y por eso no se versiona.

# --------------------------------------------------------------------------------------------
# LA VERIFICACION. Aqui no hay navegador, asi que se comprueba lo que se puede comprobar sobre
# el fichero escrito — y se comprueba SIEMPRE, no solo cuando alguien se acuerda:
#
#   python3 bench/panel.py              genera y verifica
#   python3 bench/panel.py --verifica   solo verifica el que ya esta escrito
#
# No es higiene: es lo unico que impide que el panel se pudra en silencio. Un panel que un dia
# empieza a pedir algo a la red, o que se queda sin una pestana, o cuyo bloque de datos deja de
# parsear, sigue ABRIENDOSE. Parece bien y no lo esta, que es el peor estado posible para un
# instrumento de medida.
# --------------------------------------------------------------------------------------------

# Un enlace a 127.0.0.1 NO es una referencia externa: no sale de la maquina y no se pide solo
# —es el selector de herramientas, y solo va si haces clic—. La regla que hay que sostener es
# "el panel no depende de nadie para verse", no "la cadena http:// esta prohibida".
_LOOPBACK = re.compile(r"https?://(?:127\.0\.0\.1|localhost)(?::\d+)?[\w./?=-]*")
# Lo que se prohibe es que la pagina PIDA algo de fuera, no que la cadena "http://" aparezca:
# la url de una dependencia dentro de los datos no descarga nada. Por eso se buscan REFERENCIAS
# (src, href, @import, url(), fetch) y no cadenas sueltas — una regla mas estrecha que la de
# NERVIO porque aqui el libro mayor SI lleva urls dentro.
_EXTERNO = re.compile(r"<script[^>]+src=|<link[^>]+href=|@import\s|url\(\s*[\"']?https?:"
                      r"|fetch\(\s*[\"']https?:|XMLHttpRequest")


def verifica(ruta):
    """Lo que se puede comprobar sobre el fichero escrito — y se comprueba SIEMPRE.

    Ya no hay once ficheros que cotejar entre si: hay UNO con once vistas dentro, asi que lo que
    antes cruzaba paginas ahora cruza ramas del json. La pregunta de fondo es la misma, y es la
    que costo dos semanas ver: ¿de verdad cada herramienta lleva LO SUYO, o hay alguna enseñando
    los numeros de otra?"""
    if not os.path.exists(ruta):
        print(f"VERIFICACION FALLA: no existe {ruta}", file=sys.stderr); return 1
    html = open(ruta, encoding="utf-8").read()
    fallos = []

    # 1. El bloque de datos parsea y tiene la forma nueva: comun + vistas.
    i = html.find("let RAW = ")
    j = html.find(";\n", i)
    datos = None
    if i < 0 or j < 0:
        fallos.append("no se encuentra el bloque de datos embebido")
    else:
        try: datos = json.loads(html[i + len("let RAW = "):j])
        except json.JSONDecodeError as e:
            fallos.append(f"el bloque de datos embebido NO parsea: {e}")
    if datos is not None and ("comun" not in datos or "vistas" not in datos):
        fallos.append("el bloque de datos no tiene la forma {comun, vistas}")
        datos = None

    if datos is not None:
        comun, vistas = datos["comun"], datos["vistas"]
        for clave in ("camino", "subs", "sello", "commits"):
            if clave not in comun:
                fallos.append(f"lo comun no trae '{clave}'")
        # 2. DETERMINISMO: nada que cambie solo por volver a generar.
        for volatil in VIVOS + ("gen",):
            if volatil in comun:
                fallos.append(f"'{volatil}' esta dentro del html y no deberia: cambia sin que "
                              "cambie ningun dato y ensucia el git diff (vive en datos.json)")

        # 3. TODAS LAS HERRAMIENTAS TIENEN VISTA. Sin excepcion y sin condiciones: ni «si tiene
        #    banco propio» ni «si tiene medidas». Esa condicion existia —el censo viejo exigia
        #    banco propio para tener ficha— y dejo a vlz fuera del panel el dia que nacio, que es
        #    justo el dia en que mas falta hace verla.
        faltan = [k for k in ATR.CLAVES if k not in vistas]
        if faltan:
            fallos.append(f"herramientas del censo sin vista propia: {faltan}")
        if "principal" not in vistas:
            fallos.append("no hay vista principal")

        # 4. CADA VISTA LLEVA LO SUYO Y NADA MAS. La prueba es el libro mayor, que es donde
        #    conviven todas: si una vista trae mas filas de las que su ficha dice que son suyas,
        #    hay dato de otro dentro. Ese era el fallo — publicar las medidas del archivador bajo
        #    el nombre de vdeflate.
        for sl, v in vistas.items():
            if sl == "principal": continue
            f = v.get("ficha") or {}
            if f.get("clave") != sl:
                fallos.append(f"la vista '{sl}' no trae su ficha (trae {f.get('clave')!r})")
            if not v.get("pestanas"):
                fallos.append(f"la vista '{sl}' se queda sin ninguna pestaña")
            if sl == "banco": continue      # el banco audita el libro mayor entero: es su tema
            fil = (v.get("mayor") or {}).get("filas")
            if fil is not None and f.get("medidas") is not None and fil != f["medidas"]:
                fallos.append(f"la vista '{sl}': el libro mayor trae {fil} filas y su ficha dice "
                              f"{f['medidas']} medidas suyas — hay dato de otro dentro")

        # 5. DOS VISTAS IGUALES SON UNA VISTA MAL CORTADA. Esto se veia en el sello y nadie lo
        #    leia: las cinco paginas salian con el mismo. Ahora falla.
        iguales = {}
        for sl, v in vistas.items():
            iguales.setdefault(json.dumps(v, sort_keys=True, ensure_ascii=False), []).append(sl)
        for _c, sls in iguales.items():
            if len(sls) > 1:
                fallos.append(f"vistas con EXACTAMENTE el mismo contenido: {sorted(sls)}")

        # 6. R1 · NADA MEDIDO ES INVISIBLE, sobre el conjunto: sumando las tablas de todas las
        #    vistas no puede faltar ninguna .tsv. Repartir trae un riesgo que antes no existia
        #    —que una tabla no le toque a nadie y desaparezca de las once a la vez— y esta es la
        #    comprobacion que lo cubre.
        cubiertas = set()
        for v in vistas.values():
            cubiertas |= set(v.get("tablas") or {}) | set(v.get("tablas_vacias") or {})
        sueltas = [f for f in sorted(os.listdir(B))
                   if f.endswith(".tsv") and f[:-4] not in cubiertas]
        sueltas += [f + " (formato que el panel no sabe pintar)" for f in sorted(os.listdir(B))
                    if f.endswith(".jsonl") and f not in JSONL_CUBIERTOS]
        if sueltas:
            fallos.append("ficheros de medidas que no llegan a NINGUNA vista: " + ", ".join(sueltas))

        # 7. NI UNA SECCION HUERFANA NI SIEMPRE OCULTA. Cada seccion declara dueño, ese dueño
        #    existe, y su pestaña sale en alguna vista donde la seccion se vea. Una seccion
        #    presente y siempre oculta es una seccion perdida con mejor disfraz — asi salio el
        #    panel en blanco.
        secs = re.findall(r'<section data-t="(\d+)" data-h="([^"]*)">', html)
        if not secs:
            fallos.append("no hay ni una seccion marcada con pestaña y dueño")
        conocidos = set(vistas) | {CADA_HERRAMIENTA, TODAS_LAS_PAGINAS}
        for t, d in secs:
            if d not in conocidos:
                fallos.append(f"seccion con dueño desconocido: {d!r}"); continue
            if not [sl for sl in vistas if _visible(d, sl)
                    and any(p[0] == int(t) for p in vistas[sl].get("pestanas", []))]:
                fallos.append(f"seccion data-t={t} data-h={d}: no se ve en NINGUNA vista")
        n_html = len(set(re.findall(r"<h2>([^<]+)</h2>", html)))
        n_plan = len(set(re.findall(r"<h2>([^<]+)</h2>", HTML)))
        if n_html < n_plan:
            fallos.append(f"faltan secciones en el html: {n_html} de {n_plan}")

    # 8. Cero red.
    externos = sorted({m.group(0) for m in _EXTERNO.finditer(_LOOPBACK.sub("", html))})
    if externos:
        fallos.append(f"referencias externas en un panel que debe ser autocontenido: {externos}")

    # 9. El blindaje sigue puesto: ninguna seccion puede tumbar la pagina.
    if html.count("seguro(function(){") < 20:
        fallos.append("hay secciones sin blindar: una que falle se llevaria por delante el resto")

    # 10. Y QUE NO QUEDEN RESTOS DEL PANEL TROCEADO. Un h-*.html viejo abre tan tranquilo y
    #     enseña el dato del dia que se genero, sin decir que es un fantasma.
    restos = sorted(os.path.basename(p) for p in
                    glob.glob(os.path.join(B, "h-*.html")) +
                    glob.glob(os.path.join(B, "datos-*.json")))
    if restos:
        fallos.append(f"quedan ficheros del panel troceado, que abren y mienten: {restos}")

    # 11. Y EL LIBRO MAYOR ESTA INTACTO. Todo lo de arriba comprueba el fichero GENERADO; esto
    #     comprueba la FUENTE, que es lo unico que no se puede rehacer. `bench/libro.py` caza
    #     filas duplicadas, lineas que no parsean y filas sin los campos con los que se identifica
    #     una medida. Es barato -una pasada sobre el JSONL- y tapa el unico agujero por el que hoy
    #     se puede corromper el banco sin enterarse: el libro es append-only, no tiene clave unica,
    #     y el 03-08 habia en /tmp nueve filas ya integradas junto a la orden impresa de volver a
    #     anadirlas. Duplicar filas no rompe nada de golpe: promedia, y las cifras salen un poco
    #     mal para siempre.
    _lib = subprocess.run([sys.executable, os.path.join(B, "libro.py"), "--verifica"],
                          cwd=RAIZ, capture_output=True, text=True)
    if _lib.returncode != 0:
        fallos.append("el libro mayor no pasa bench/libro.py --verifica:\n" + _lib.stdout.strip())

    if fallos:
        for f in fallos:
            print(f"VERIFICACION FALLA: {f}", file=sys.stderr)
        return 1
    locales = len(_LOOPBACK.findall(html))
    print(f"verificacion OK: {os.path.basename(ruta)} {os.path.getsize(ruta)} bytes · "
          f"UN fichero con {len(datos['vistas'])} vistas · datos embebidos parsean · "
          f"sello {datos['comun'].get('sello')} · 0 referencias externas"
          + (f" · {locales} enlaces a 127.0.0.1 (no se piden solos)" if locales else "")
          + f" · {html.count('seguro(function(){')} secciones blindadas")
    return 0


# Verificar sin regenerar: util para comprobar en frio el panel que ya esta escrito.
if SOLO_VERIFICA:
    sys.exit(verifica(os.path.join(B, "panel.html")))

D["commit"] = sh("git rev-parse --short HEAD")
D["asunto"] = sh("git log -1 --pretty=%s")
D["pestanas"] = [e for _, e in PESTANAS]
D["sucio"] = bool(sh("git status --porcelain"))
D["host"] = sh("uname -srm") + " · " + sh("nproc") + " hilos"
# HASTA DONDE LLEGA EL LIBRO MAYOR. Se calcula aparte y barato -una pasada por el
# fichero- porque tiene que salir en la cabecera de TODAS las vistas, incluso abiertas
# como fichero suelto, donde no hay bucle de refresco que consultar. Es el unico numero
# que contesta «¿esto que estoy mirando es de hoy?», y no salia en ningun sitio.
try:
    _ts = [json.loads(_l).get("t", "") for _l in
           open(os.path.join(B, "historia.jsonl"), encoding="utf-8") if _l.strip()]
    D["libro_hasta"] = max(_ts)[:16].replace("T", " ") if _ts else ""
    D["libro_filas"] = len(_ts)
except (OSError, json.JSONDecodeError):
    D["libro_hasta"], D["libro_filas"] = "", 0


# QUE CUENTA COMO DATO: vive en bench/comun_banco.py porque bench/decide.py tiene que aplicar
# EXACTAMENTE la misma regla. Si el panel y el que decide discreparan en que filas valen, el mismo
# experimento saldria con dos cifras segun quien lo calcule — que es el fallo que este trabajo
# viene a cerrar, no uno que convenga reintroducir por comodidad.
valida = CB.valida

# ---- ejecuciones -------------------------------------------------------------------------
D["ejec"] = []
try:
    for l in open(os.path.join(B, "historia.jsonl"), encoding="utf-8"):
        l = l.strip()
        if l and valida(json.loads(l)): D["ejec"].append(json.loads(l))
except OSError: pass
D["ejec"] = D["ejec"][-120:]

# ---- la aguja: la MISMA bateria en cada commit ---------------------------------------------
# El resto del banco mide cosas sueltas y sirve para decidir un cambio concreto. Esto sirve
# para lo otro: ver si ese cambio movio la aguja EN TODO LO DEMAS. Un cambio que mejora una
# metrica y empeora tres tiene que verse en la misma pantalla, no dos vueltas despues.
D["aguja"] = []
try:
    for l in open(os.path.join(B, "aguja.jsonl"), encoding="utf-8"):
        if l.strip(): D["aguja"].append(json.loads(l))
except OSError: pass
D["aguja"] = D["aguja"][-14:]

# ---- rivales -----------------------------------------------------------------------------
riv = {}
for f in tsv("rivales.tsv"):
    if len(f) < 4: continue
    ent, nom, ver, by = f[0], f[1], f[2], f[3]
    seg = f[4] if len(f) > 4 else "-"
    try: by = int(by)
    except ValueError: continue
    riv.setdefault(ent, []).append({"nom": nom, "bytes": by,
                                    "seg": (None if seg in ("-", "") else float(seg))})
D["rivales"] = riv

# ---- lo nuestro, por muestra (de historia si esta; si no, de multibench) -------------------
D["multi"] = tsv("multibench.tsv")
# ---- el registro de preguntas: lo que sabemos y LO QUE NO ------------------------------------
_pr = tsv("preguntas.tsv")
D["preguntas"] = _pr
D["ejes"] = {
  "A": "¿Cuánto comprime?",
  "B": "¿Cuánto cuesta? — tiempo, CPU, memoria, dinero",
  "C": "¿Se puede confiar? — integridad, compatibilidad, fallos",
  "D": "¿Es un producto? — funciones, interfaz, reparto",
  "E": "¿Dónde perdemos?",
  "F": "Lo que no hemos medido NUNCA",
}

# AUTO-AUDITORIA. Cuantas tablas del banco traen el guion que las reproduce.
#
# Es la cuenta que lleva NERVIO (sin_comando: 0 de 246 filas) y que nosotros no llevabamos. El
# criterio es EXPLICITO y se dice para que nadie lea mas de lo que mide: se busca en las lineas
# de comentario de cada .tsv una referencia a un fichero de bench/ QUE EXISTE. Una tabla puede
# ser perfectamente reproducible y no decirlo; esto no mide reproducibilidad, mide si esta
# ESCRITO como reproducirla. Que es justo lo que se pierde a los tres meses.
_guiones = {}
for _g in sorted(os.listdir("bench")):
    if _g.endswith((".sh", ".py")):
        try: _guiones[_g] = open("bench/"+_g, encoding="utf-8", errors="replace").read()
        except OSError: pass
_dicho, _hay, _ninguno = [], [], []
for _f in sorted(os.listdir("bench")):
    if not _f.endswith(".tsv"): continue
    _cab = [l for l in open("bench/"+_f, encoding="utf-8", errors="replace").read().split("\n")[:12]
            if l.startswith("#")]
    _refs = re.findall(r"bench/[\w.-]+\.(?:sh|py)", " ".join(_cab))
    if any(os.path.exists(r) for r in _refs):
        _dicho.append(_f)
    else:
        # No lo dice: ¿lo ESCRIBE algun guion? MENCIONAR NO ES ESCRIBIR: el registro de preguntas
        # nombra trece tablas en su columna «de» y eso no las produce. Se exige un contexto de
        # escritura de verdad — redireccion, tee, o un open(...,"w").
        _esc = re.escape(_f)
        _pat = re.compile(r'(?:>\s*|tee\s+(?:-a\s+)?|open\(\s*.)(?:bench/)?' + _esc)
        # panel.py es el CONSUMIDOR: pinta tablas, nunca las escribe. Se excluye o se cuela
        # como productor de todo lo que lee.
        _quien = [g for g, t in _guiones.items() if g != "panel.py" and _pat.search(t)]
        (_hay if _quien else _ninguno).append(_f if not _quien else f"{_f} ← {_quien[0]}")
D["auditoria"] = {"dicho": _dicho, "hay": _hay, "ninguno": _ninguno}

# UNA sola tabla con TODOS los rivales. Estaban en dos ficheros con formatos distintos y
# ninguno los tenia todos: rivales.tsv (largo: zpaq, paq8px, xz, zstd, brotli) y
# comparativa-completa.tsv (ancho: zip, rar, 7z). Se fusionan aqui.
_cc = tsv("comparativa-completa.tsv")
_todo = {}          # entrada -> {rival: bytes}
_orig = {}          # entrada -> bytes originales
_ver = {}           # rival -> version medida

# el ancho
if _cc:
    _cab = _cc[0]
    for _f in _cc[1:]:
        _e = _f[0]
        _todo.setdefault(_e, {})
        for _i, _c in enumerate(_cab):
            if _i == 0: continue
            _v = _f[_i] if _i < len(_f) else ""
            if _v in ("", "-", "—"): continue
            try: _n = int(float(_v))
            except ValueError: continue
            if _c == "original": _orig[_e] = _n
            else: _todo[_e][_c] = _n

# el largo
# SIN `[1:]`, y no es un detalle: la cabecera de rivales.tsv va COMENTADA, asi que `tsv()` ya la
# tira y ese recorte se comia LA PRIMERA FILA DE DATOS —codigo.10m contra zpaq—. No se veia
# porque la tabla ancha trae esa misma celda; el dia que un rival salga solo en la primera linea
# de este fichero, desaparece sin avisar. Y arriba (D["rivales"]) el mismo fichero se lee sin
# recorte: el generador tenia dos convenios distintos para el mismo fichero.
for _f in tsv("rivales.tsv"):
    if len(_f) < 4: continue
    _e, _r, _vv, _b = _f[0], _f[1], _f[2], _f[3]
    try: _n = int(float(_b))
    except ValueError: continue
    _todo.setdefault(_e, {})[_r] = _n
    if _vv: _ver[_r] = _vv

# lo nuestro, de bench/nuestro.tsv, que es la cifra viva
for _f in tsv("nuestro.tsv")[1:]:
    if len(_f) < 2: continue
    try: _todo.setdefault(_f[0], {})["nuestro_max"] = int(float(_f[1]))
    except ValueError: pass

_cols = sorted({_r for _v in _todo.values() for _r in _v},
               key=lambda r: (not r.startswith("nuestro"), r))
_filas = [["entrada", "original"] + _cols]
for _e in sorted(_todo):
    _filas.append([_e, str(_orig.get(_e, ""))] +
                  [str(_todo[_e].get(_c, "")) for _c in _cols])
D["comparativa"] = _filas
D["versiones"] = _ver
# Cuantas celdas faltan. No se rellenan: una comparativa con huecos inventados no vale nada.
D["huecos"] = sum(1 for _e in _todo for _c in _cols if _c not in _todo[_e])
D["economia"] = tsv("economia.tsv")
# 🚨 ESTO LLEVABA SIN LEERSE DESDE QUE SE ESCRIBIO. Decia `lee("bench/economia-nivel.txt")`, pero
# `lee()` ya prefija `bench/`, asi que resolvia a `bench/bench/economia-nivel.txt`, no existia, y
# `lee()` se traga el OSError y devuelve "". Resultado: 2,6 KB de analisis por nivel que el panel
# nunca ha enseniado, sin un solo aviso. Es el precio de tragarse los errores: no se nota la
# diferencia entre «no hay nada que contar» y «no lo estoy leyendo».
D["econivel"] = lee("economia-nivel.txt")
# Quien es un PRODUCTO y quien no. Cambia como se lee la tabla entera y estaba solo en un
# comentario del .tsv que nadie ve.
D["quien_es"] = {
  "zip -9":"producto", "rar -m5":"producto", "rar max":"producto", "7z -mx9":"producto",
  "xz -9e":"producto", "zstd -19":"producto", "brotli -11":"producto", "brotli -q11":"producto",
  "restic":"producto", "borg":"producto",
  "xz -9":"producto",
  "zpaq -m5":"a medias",
  "paq8px -8":"no es producto", "paq8px -4":"no es producto", "nncp-lstm":"no es producto",
  "nuestro_defecto":"nosotros", "nuestro_max":"nosotros",
}
# GUARDIA: una columna sin clasificar cuenta como producto sin querer en el marcador, y eso ya me
# dio hoy una cifra enganosa («ganamos 4 de 9» metiendo a paq8px con WinRAR). Si aparece alguna,
# el panel lo DICE en vez de tragarsela en silencio.
D["sin_clasificar"] = [c for c in _cols if c not in D["quien_es"]]
D["vdeflate"] = tsv("vdeflate.tsv")
D["camino"] = tsv("preguntas-camino.tsv")
D["auditcam"] = tsv("auditoria-camino.tsv")
D["plan"] = tsv("plan.tsv")
D["desperdicio"] = tsv("desperdicio.tsv")
D["tecnicas"] = tsv("tecnicas.tsv")
D["carriles"] = tsv("carriles.tsv")
D["propias"] = tsv("herramientas-propias.tsv")
D["junturas"] = tsv("junturas.tsv")
D["undisparo"] = tsv("junturas-undisparo.tsv")
# QUE ESTA APLICADO Y EN QUE VERSION ESTAMOS. Las versiones se LEEN DEL FUENTE de Rust y los
# commits de `git log`: en bench/aplicado.py no hay ni un dato escrito a mano. La columna de
# medidas cuenta entradas de historia.jsonl por commit, asi que un 0 no dice «este commit no hizo
# nada», dice «su medida no paso por la bascula y por eso la linea de tiempo no puede dibujarlo».
D["aplicado"] = tsv("aplicado.tsv")
D["aplicado_version"] = tsv("aplicado-version.tsv")
# QUE SE DECIDIO Y SI LA MEDIDA LO SOSTIENE. La contraparte que le faltaba a `aplicado`: habia
# registro de lo aplicado y NINGUNO de lo rechazado, asi que los negativos vivian en comentarios
# de crates/ y en cuerpos de commit, donde nadie los puede recomputar. Ni una cifra de esta tabla
# esta escrita a mano: bench/decide.py las saca de historia.jsonl. Lo declarado a mano -la
# identidad del experimento- vive en bench/decisiones.tsv y no lleva numeros a proposito.
D["decisiones"] = tsv("decisiones-medidas.tsv")
# El juego canonico (bench/muestras.tsv) NO se carga aparte: llega ya masticado en la columna
# `medidas` («4/5») y en `faltan`. Cargarlo suelto dejaba una clave sin dueno y la verificacion
# lo canto — bien cantado: una clave que nadie pinta es peso muerto que un dia alguien pinta mal.
D["techos"] = tsv("techos.tsv")
D["camino_comprimir"] = tsv("camino-comprimir.tsv")
D["precomputar"] = tsv("precomputar.tsv")
D["terceros"] = tsv("terceros.tsv")

# ========== R3 · LA INCERTIDUMBRE VIAJA PEGADA A LA CIFRA ·· R4 · NI UNA CIFRA SIN SU COMANDO
#
# El libro mayor tenia 162 filas y el panel las pintaba de una en una, como una lista de cosas
# que pasaron. Pero UNA medida no dice nada: lo que dice algo es «esta configuracion, N veces,
# con esta dispersion». Enseniar una cifra sin su N es exactamente como se publica un 3% que
# era ruido —y aqui ya paso: ±0,62% publicado contra ±1,3% real, con seis controles de cero
# delante sin reconocerlos—.
#
# LA REGLA DE HONESTIDAD VIVE AQUI DENTRO, en la funcion que agrega, y no en quien la llama.
# Puesta fuera se olvida justo el dia que corre prisa.
def _agrega(vals):
    n = len(vals)
    if n == 0: return None
    m = sum(vals) / n
    # n = 1: no hay dispersion que enseniar, y se DICE que es una sola vuelta.
    if n == 1: return {"n": 1, "media": m, "disp": None, "clase": "una vuelta"}
    # n = 2: rango si, desviacion no. Con dos puntos la sigma es un adorno con decimales.
    if n == 2: return {"n": 2, "media": m, "disp": (max(vals) - min(vals)) / 2, "clase": "rango"}
    # n >= 3: desviacion tipica MUESTRAL (n-1), que es la que no subestima con pocas vueltas.
    _v = sum((x - m) ** 2 for x in vals) / (n - 1)
    return {"n": n, "media": m, "disp": _v ** 0.5, "clase": "desviación"}

_HIST = []
_hp = os.path.join(B, "historia.jsonl")
if os.path.exists(_hp):
    for _l in open(_hp, encoding="utf-8"):
        _l = _l.strip()
        if not _l: continue
        try:
            _f = json.loads(_l)
            if valida(_f): _HIST.append(_f)
        except json.JSONDecodeError: pass

# Se agrupa por lo que define un experimento: QUE se midio, SOBRE QUE y CON QUE configuracion.
# Dos filas con la misma etiqueta pero distinta muestra no son dos vueltas de lo mismo.
# EL NOMBRE DE LA VUELTA NO ES PARTE DEL EXPERIMENTO. Los guiones etiquetan las repeticiones
# como «... (vuelta 1)», «... (vuelta 2)», asi que agrupar por la etiqueta cruda dejaba CADA
# vuelta en su propio grupo: 162 experimentos de 171 filas, 153 de ellos con n=1, y el suelo de
# ruido salia vacio por no haber ni un solo grupo con tres vueltas. La agregacion se estaba
# anulando a si misma, que es peor que no agregar: parecia que no habiamos repetido nada.
_VUELTA = re.compile(r"\s*\(vuelta\s*\d+\)\s*$")
# Y EL COMMIT ENTRA EN LA CLAVE. Sin el, dos versiones distintas del MISMO codigo se promediaban
# como si fueran repeticiones: «2a instantanea con salto» salia con n=4 y ±75,5 s porque juntaba
# las dos vueltas de la implementacion rota (148 s) con las dos de la buena (17,7 s), y de ahi
# salia un suelo de ruido del 90,9% que no era ruido, era un cambio de codigo. Dos vueltas del
# mismo commit son repeticiones; de commits distintos, no.
# ── DE QUE HERRAMIENTA ES CADA MEDIDA ───────────────────────────────────────────────────────
#
# Norma del dueño, y sale de un fallo real: `--price` estimaba la CPU con una tabla que mide EL
# CODEC SOLO para decidir sobre EL ARCHIVADOR ENTERO. Los dos numeros eran correctos —34,8x de
# diferencia entre niveles para el codec, 1,79x para el archivador— y ninguno servia para lo del
# otro. La cifra cruzo de herramienta porque nada anotaba de cual era.
#
# Desde hoy `mide.sh` lo anota. Las filas ANTERIORES no se reescriben —el libro mayor es
# append-only y falsear el pasado seria peor que no tenerlo—: se DEDUCE al leer, y se dice
# cuantas estan deducidas y cuantas declaradas.
#
# LA ATRIBUCION YA NO VIVE AQUI: vive en bench/atribucion.py, una sola vez para todo el banco.
# Estaba escrita dos veces —aqui y en propias.py— y las dos copias ya no coincidian: las medidas
# de perfilar zstd eran de «codec-zstd» para este fichero y de vlz para el censo. La misma fila,
# dos duenos, segun a quien preguntaras.
_por_herr = {}
for _r in _HIST:
    _h, _decl = ATR.herr_de_fila(_r)
    _e = _por_herr.setdefault(_h, {"filas": 0, "declaradas": 0})
    _e["filas"] += 1
    if _decl: _e["declaradas"] += 1


def _grupos_de(filas):
    """Los grupos —un experimento repetido N veces— de un subconjunto del libro mayor.

    Se saca a funcion porque ahora se llama UNA VEZ POR HERRAMIENTA: cada panel agrega SOLO sus
    filas. Antes se calculaba una vez sobre todo y las cinco paginas ensenaban el mismo total,
    que es como el panel de vdeflate acabo publicando las 228 medidas del archivador."""
    _g = {}
    for _r in filas:
        _et = _VUELTA.sub("", _r.get("etiqueta", ""))
        _g.setdefault((_et, _r.get("muestra", ""), _r.get("config", ""),
                       _r.get("commit", "?")), []).append(_r)
    _gr = []
    for (_e, _m, _c, _cm), _rs in _g.items():
        _num = lambda k: [r[k] for r in _rs if isinstance(r.get(k), (int, float)) and r[k] > 0]
        _cargas = [r.get("carga") or 0 for r in _rs]
        _gr.append({"etiqueta": _e, "muestra": _m, "config": _c, "commit": _cm,
                    "seg": _agrega(_num("seg")), "ciclos": _agrega(_num("ciclos")),
                    "carga_min": min(_cargas) if _cargas else 0,
                    "carga_max": max(_cargas) if _cargas else 0,
                    "sucio": any(r.get("sucio") for r in _rs),
                    "commits": sorted({r.get("commit", "?") for r in _rs})})
    _gr.sort(key=lambda g: (-(g["seg"]["n"] if g["seg"] else 0), g["etiqueta"]))
    return _gr


_grupos = _grupos_de(_HIST)

# EL SUELO DE RUIDO, LEIDO DEL PROPIO EXPERIMENTO y no estimado a ojo (R10). Sale de los grupos
# que se repitieron 3 veces o mas: su dispersion relativa ES lo que la maquina mete de su parte
# cuando no cambia nada. Un delta por debajo de eso no es una mejora, es la maquina respirando.
# EL SUELO SOLO PUEDE SALIR DE REPETICIONES QUE DE VERDAD MIDAN LO MISMO. Tres condiciones, y
# la tercera es la que duele:
#   · n >= 3, porque con dos puntos no hay dispersion que valga
#   · cargas comparables, porque a carga 6,7 y a 37,8 no es la misma maquina
#   · ARBOL LIMPIO, porque con el arbol sucio el commit NO identifica el codigo — y eso no es
#     teorico: las dos tandas del salto incremental, la rota (148 s) y la buena (17,7 s), estan
#     las dos bajo el commit 6ed723e, y promediarlas daba un «suelo de ruido» del 90,9% que no
#     era ruido, era un cambio de codigo disfrazado de repeticion.
# Si esto sale vacio, se DICE que sale vacio. Un suelo de ruido inventado es peor que ninguno:
# con el se aprueban mejoras que no existen.
def _mayor_de(filas, grupos):
    """El libro mayor auditado de un subconjunto de filas. Una llamada por herramienta.

    EL SUELO DE RUIDO SE RECALCULA CON LAS FILAS DE CADA UNA, no se hereda del global: el ruido
    de la maquina midiendo la tuberia entera no es el mismo que midiendo un codec, y darle a una
    herramienta el suelo de otra es aprobarle mejoras que no existen. Si una no tiene con que
    calcularlo, sale vacio y lo dice."""
    rel = sorted(g["seg"]["disp"] / g["seg"]["media"]
                 for g in grupos if g["seg"] and g["seg"]["n"] >= 3 and g["seg"]["media"] > 0
                 and g["carga_max"] - g["carga_min"] <= 4 and not g["sucio"])
    return {
        "filas": len(filas),
        "grupos": grupos,
        # R4: no se comprueba que el numero se repita —eso seria volver a medirlo—, se comprueba
        # que este ESCRITO como reproducirlo. Es justo lo que se pierde a los tres meses.
        "sin_cmd":    sum(1 for r in filas if not r.get("cmd")),
        "sin_commit": sum(1 for r in filas if not r.get("commit") or r["commit"] == "?"),
        "sucias":     sum(1 for r in filas if r.get("sucio")),
        "carga_alta": sum(1 for r in filas if (r.get("carga") or 0) > 8),
        "rc_malo":    sum(1 for r in filas if r.get("rc") not in (0, None)),
        "n1":         sum(1 for g in grupos if g["seg"] and g["seg"]["n"] == 1),
        "herramientas": sorted(({"nombre": k, **v} for k, v in _por_herr.items()),
                               key=lambda x: -x["filas"]),
        "suelo": {"n": len(rel),
                  "mediana": rel[len(rel) // 2] if rel else None,
                  "peor": rel[-1] if rel else None},
    }


D["mayor"] = _mayor_de(_HIST, _grupos)

# ── LA LINEA DE TIEMPO: QUE COMMIT MEJORO EL PRODUCTO Y QUE COMMIT LO EMPEORO ────────────────
#
# Encargo del dueño (31-07-2026): «una linea de tiempo global de cuanto ha mejorado el producto
# o empeorado con cada commit, e igual para cada herramienta».
#
# NO SE PUEDE RESTAR UNA MEDIDA CUALQUIERA DE OTRA. Lo unico comparable es EL MISMO experimento
# —misma etiqueta, misma muestra, misma configuracion— medido en dos commits distintos. Restar
# dos filas que no son el mismo experimento da un numero grande y sin significado, que es peor
# que no dar ninguno. Asi que la linea se construye por experimento y luego se agrega por commit.
#
# Y HAY TRES FORMAS DE MENTIR AQUI, las tres tapadas:
#   · el ARBOL SUCIO: con el arbol sucio el commit NO identifica el codigo. Ya paso —las dos
#     implementaciones del salto incremental, la de 148 s y la de 17,7 s, viven las dos bajo
#     6ed723e—. Una comparacion con una punta sucia se marca «no comparable» y no se cuenta.
#   · el SUELO DE RUIDO: un delta por debajo del ruido de la maquina no es una mejora, es la
#     maquina respirando. Se usa el suelo de ESTA herramienta, no uno global.
#   · la PRIMERA VEZ: un experimento que solo se ha medido una vez no dice si mejoro. Sale como
#     «nuevo», que es informacion, y no como mejora de cero.
# Las cuatro cosas que se pagan, con el nombre con el que se habla de ellas. En las cuatro,
# MENOS ES MEJOR — y por eso una sola regla de signo vale para las cuatro: delta negativo = mejora.
_METRICAS = (("bytes_out", "compresión"), ("seg", "tiempo"),
             ("ciclos", "CPU"), ("rss_kb", "RAM"))
_COMMITS = {}
for _l in sh("git log --pretty=%h\x01%s\x01%cI -n 400").splitlines():
    _p = _l.split("\x01")
    if len(_p) == 3: _COMMITS[_p[0]] = {"asunto": _p[1], "fecha": _p[2]}
D["commits"] = _COMMITS

# ── DE QUE COMMIT ES ESTE CODIGO — por el SELLO, no por lo que hubiera en HEAD ────────────────
#
# 🚨 Aqui se mide primero y se commitea despues con la cifra en el mensaje, que es lo correcto.
# Pero `mide.sh` sella `HEAD` en el momento de medir, asi que la fila cae bajo el commit ANTERIOR
# y el que CONTIENE el cambio sale vacio. El 02-08 salieron asi diez commits seguidos.
#
# `arbol_sha` -el digesto de los blobs de crates/ + Cargo.* que la bascula sella desde el 02-08-
# identifica el codigo EXACTO que corrio. Aqui se calcula el mismo digesto para cada commit, con
# `git ls-tree` y sin abrir un fichero, y se construye el mapa al reves. Es el MISMO calculo que
# hace bench/aplicado.py; si los dos divergieran, la tabla y la curva contarian cosas distintas.
# ⚠️ EL CALCULO YA NO VIVE AQUI. Estaba copiado en este fichero y en bench/aplicado.py, y el
# comentario de arriba avisaba de lo que pasaria si divergieran. El 03-08 iba a haber una TERCERA
# copia -bench/decide.py necesita el mismo digesto para saber si el commit que recoge una medida
# es el mismo codigo que se midio-, asi que se saco a bench/comun_banco.py y lo importan los tres.
# Tres copias de una formula no es redundancia, es tres oportunidades de contar cosas distintas.
_ARBOL_A_COMMIT = {}
for _c in list(_COMMITS)[:120]:
    _d = CB.arbol_de_commit(sh, _c)
    if not _d: continue
    # El MAS ANTIGUO se queda con el sello: si tres commits seguidos no tocan crates/ -papeleo,
    # banco, panel- comparten arbol, y la mejora es del primero, no del ultimo que pasaba por ahi.
    _ARBOL_A_COMMIT[_d] = _c

def _commit_de(fila):
    """El commit cuyo CODIGO se midio. El sello manda; si no lo hay, lo que hubiera en HEAD."""
    _a = fila.get("arbol_sha")
    if _a and _a in _ARBOL_A_COMMIT: return _ARBOL_A_COMMIT[_a]
    return fila.get("commit", "?")


def _linea_de(filas, suelo):
    """Commit a commit: que mejoro, que empeoro y que no se movio. En orden, del ultimo al primero."""
    fs = sorted([r for r in filas if r.get("t")], key=lambda r: r["t"])
    por_exp = {}
    for r in fs:
        por_exp.setdefault((_VUELTA.sub("", r.get("etiqueta", "")), r.get("muestra", ""),
                            r.get("config", "")), []).append(r)
    ev, primeras = {}, {}
    for (_et, _mu, _cf), rs in por_exp.items():
        # las vueltas del mismo commit son UNA medida: se promedian antes de comparar
        # 🚨 SE ORDENA POR LA FECHA DEL COMMIT, NO POR CUANDO SE MIDIO.
        #
        # Antes se recorrian las filas en orden de medida, dando por hecho que siempre se avanza:
        # mides, commiteas, mides otra vez. En cuanto se RECONSTRUYE un punto del pasado —compilar
        # un commit viejo para tener el «antes» que faltaba— esa premisa se rompe y el par se
        # calcula HACIA ATRAS. Paso el 03-08 al recuperar el punto previo al carril PDF: el panel
        # publico «-34,33% de RAM» presentando el codigo de por la mañana como una mejora del de
        # ahora. El signo entero al reves, y con pinta de titular.
        #
        # El orden de una linea de tiempo es el de la HISTORIA, no el del cuaderno de laboratorio.
        # Los commits que git no conoce van al final, que es donde no estorban.
        _g = {}
        for r in rs:
            # EL COMMIT DEL CODIGO, no el que hubiera en HEAD al medir. Ver `_commit_de`.
            _g.setdefault(_commit_de(r), []).append(r)
        seq = [(c, _g[c]) for c in sorted(_g, key=lambda c: (_COMMITS.get(c, {}).get("fecha") or "9999", c))]
        primeras.setdefault(seq[0][0], []).append({"etiqueta": _et, "muestra": _mu})
        for i in range(1, len(seq)):
            ca, ra = seq[i - 1]
            cb, rb = seq[i]
            for campo, nombre in _METRICAS:
                va = [x[campo] for x in ra if isinstance(x.get(campo), (int, float)) and x[campo] > 0]
                vb = [x[campo] for x in rb if isinstance(x.get(campo), (int, float)) and x[campo] > 0]
                if not va or not vb: continue
                ma, mb = sum(va) / len(va), sum(vb) / len(vb)
                if ma <= 0: continue
                d = (mb - ma) / ma
                # 🚨 ¿SE PUEDE ATRIBUIR ESTA DIFERENCIA? Y hasta el 02-08 esta linea decia que NO
                # casi siempre, por el motivo equivocado.
                #
                # Miraba `sucio`, que es un booleano sobre TODO el arbol: dejar caer un guion en
                # bench/ marcaba el par como no comparable aunque el binario fuera identico. El
                # propio `mide.sh` tiene escrito por que eso esta mal —«siete filas de una tanda
                # salieron sucias por seis ficheros .sh que el compresor no abre nunca; una alarma
                # que salta por lo que no importa enseña a ignorarla»— y por eso existe
                # `sucio_codigo`. La linea de tiempo nunca lo adopto: el 02-08 tiro CATORCE pares
                # de una tarde entera de trabajo y el grafico salio plano.
                #
                # Ahora la pregunta se hace bien y en dos escalones:
                #   1. si las dos filas traen `arbol_sha`, el codigo de cada lado esta
                #      IDENTIFICADO exactamente —lo hubiera commiteado alguien o no— y el par
                #      es comparable. Es lo que sella `mide.sh` desde el 02-08.
                #   2. si no lo traen (filas viejas), se cae a `sucio_codigo`, que al menos
                #      pregunta por crates/ + Cargo.* y no por el arbol entero.
                _ida = [x.get("arbol_sha") for x in ra]
                _idb = [x.get("arbol_sha") for x in rb]
                identificado = all(_ida) and all(_idb)
                sucio = (not identificado) and (any(x.get("sucio_codigo") for x in ra)
                                                or any(x.get("sucio_codigo") for x in rb))
                # ¿ES LA MISMA ENTRADA? Y no basta con que la etiqueta lo diga.
                #
                # Paso el 31-07: dos medidas de F18 con la MISMA etiqueta, la misma `muestra`
                # -arbol2.7g- y la misma `config`, una sobre 2.621 MB y otra sobre 711 MB. El
                # panel las resto y publico un empeoramiento gigante. No habia empeorado nada:
                # no era el mismo trabajo. La entrada habia cambiado y la etiqueta no.
                #
                # Cambiar la etiqueta arregla ese caso y deja abierto el siguiente, porque la
                # etiqueta la escribe una persona. `bytes_in` ya viaja en cada medida: se mira el
                # DATO. Mismo criterio que el arbol sucio —alli el commit no identifica el CODIGO,
                # aqui la etiqueta no identifica la ENTRADA—: dos formas de que un par no sea un par.
                _ia = [x.get("bytes_in") or 0 for x in ra if (x.get("bytes_in") or 0) > 0]
                _ib = [x.get("bytes_in") or 0 for x in rb if (x.get("bytes_in") or 0) > 0]
                _pa = sum(_ia) / len(_ia) if _ia else 0
                _pb = sum(_ib) / len(_ib) if _ib else 0
                # El 2% es holgura de que el corpus se toque un poco, no de que cambie. Un 3,7x
                # como el de F18 no se cuela ni de lejos.
                otra_entrada = _pa > 0 and _pb > 0 and abs(_pb - _pa) / _pa > 0.02
                # 🚨 EL SUELO DE RUIDO ES DEL RELOJ. A LOS BYTES NO SE LES APLICA.
                #
                # Estaba aplicado a las cuatro metricas por igual, y el suelo -2,96%- esta medido
                # cronometrando el respaldo diario. Pero los BYTES SON DETERMINISTAS: dos vueltas
                # de la misma configuracion sobre la misma entrada dan el MISMO archivo, y este
                # banco lo verifica (crates/vc-cli/src/main.rs:374; y la tanda B13 saco 950.245.382
                # exactos en sus dos «completo»). Un cambio de bytes no puede ser ruido.
                #
                # El destrozo era grande y silencioso: el trabajo de compresion mueve DECIMAS de
                # punto, asi que todo lo que este banco ha comprado en bytes caia por debajo de un
                # suelo prestado del reloj y se contaba como CERO. El 03-08, el carril PDF -que
                # baja el archivo real un 1,285%- salio con «0 mejoras».
                #
                # A cambio, a los bytes se les exige mas por el otro lado: la entrada tiene que ser
                # EXACTAMENTE la misma, no parecida. Para el reloj un 2% de holgura es sensato
                # porque el corpus se toca; para un numero determinista, comparar sobre entradas
                # distintas es comparar otra cosa.
                es_bytes = (campo == "bytes_out")
                if sucio or otra_entrada:
                    ver = "no_comparable"
                elif es_bytes and _pa != _pb:
                    ver = "no_comparable"
                elif es_bytes:
                    ver = "ruido" if d == 0 else ("mejora" if d < 0 else "empeora")
                elif suelo and abs(d) <= suelo:  ver = "ruido"
                elif d < 0:                      ver = "mejora"
                else:                            ver = "empeora"
                ev.setdefault(cb, []).append({
                    "etiqueta": _et, "muestra": _mu, "metrica": nombre,
                    "desde": ca, "antes": ma, "despues": mb, "delta": d, "veredicto": ver,
                    "n_antes": len(va), "n_despues": len(vb),
                    "porque": ("la ENTRADA no es la misma: %.0f MB antes contra %.0f MB despues, "
                               "con la misma etiqueta" % (_pa/1e6, _pb/1e6)) if otra_entrada
                              else ("el CODIGO estaba sucio y la fila no trae `arbol_sha`: "
                                    "no hay forma de saber que se midio"
                                    if sucio else "")})
    hitos = []
    for c in set(ev) | set(primeras):
        cs = ev.get(c, [])
        hitos.append({
            "commit": c,
            "asunto": _COMMITS.get(c, {}).get("asunto", ""),
            "fecha": _COMMITS.get(c, {}).get("fecha", ""),
            "mejora":   sum(1 for x in cs if x["veredicto"] == "mejora"),
            "empeora":  sum(1 for x in cs if x["veredicto"] == "empeora"),
            "ruido":    sum(1 for x in cs if x["veredicto"] == "ruido"),
            "sucias":   sum(1 for x in cs if x["veredicto"] == "no_comparable"),
            "nuevos":   len(primeras.get(c, [])),
            "cambios":  sorted([x for x in cs if x["veredicto"] in ("mejora", "empeora")],
                               key=lambda x: x["delta"])[:14],
        })
    # Por fecha del commit si git la conoce; si no, al final. Del mas nuevo al mas viejo.
    hitos.sort(key=lambda h: (h["fecha"] or "0000"), reverse=True)

    # ── EL GRAFICO: cuanto se ha movido el producto, ACUMULADO, en las cuatro cosas que se
    #    pagan — cuanto comprime, cuanto tarda, cuanta CPU quema y cuanta RAM ocupa.
    #
    # Acumulado y no commit-a-commit porque la pregunta es «cuanto ha mejorado ESTO», no «cuanto
    # movio aquel commit»: eso ya esta en la lista de abajo. Se compone MULTIPLICANDO (1+delta),
    # que es como se componen los cambios relativos — sumar porcentajes da un numero que no
    # existe: bajar 50% y subir 50% no vuelve al principio, deja un -25%.
    #
    # SOLO ENTRAN LOS CAMBIOS REALES. Lo que cae dentro del suelo de ruido cuenta como CERO, no
    # como un microcambio: acumular ruido durante cuarenta commits fabrica una tendencia entera
    # que no ha pasado. Y lo no comparable —arbol sucio— no entra de ninguna manera.
    _cron = [h for h in reversed(hitos) if h["fecha"]]
    series, acum, acum_det = [], {}, {}
    for _campo, _nom in _METRICAS:
        # ── EL TITULAR: POR EXPERIMENTO, no promediando experimentos ──────────────────────────
        #
        # Antes se promediaba el delta de TODOS los experimentos de un commit y se multiplicaba.
        # Eso cuenta la misma mejora una vez por cada ventana desde la que se mira: `F13 carril`
        # tiene ocho variantes, `F14 frame` tres y `CERO archivar el arbol` tres vueltas, y las
        # catorce son EL MISMO ARCHIVADOR. Cuando mejora, mejoran las catorce a la vez. Asi es
        # como salio un -43,33% de reloj que ninguna medida absoluta respalda: las de punta a
        # punta sobre el mismo arbol dan entre -11,5% y -19,9%.
        #
        # Ahora cada experimento compone SOLO SU PROPIA cadena, y el titular es la MEDIANA de
        # esos acumulados. Mediana y no media para que un experimento raro no arrastre el
        # titular; y con el RANGO al lado, porque enseñar un numero solo cuando hay catorce que
        # no coinciden es esconder justo lo interesante.
        _cad = {}
        for h in _cron:
            for x in ev.get(h["commit"], []):
                if x["metrica"] != _nom or x["veredicto"] not in ("mejora", "empeora"): continue
                _k = (x.get("etiqueta", ""), x.get("muestra", ""))
                _cad[_k] = _cad.get(_k, 1.0) * (1 + x["delta"])
        _tot = sorted((v - 1) * 100 for v in _cad.values())
        if _tot:
            _n = len(_tot)
            _med = _tot[_n // 2] if _n % 2 else (_tot[_n // 2 - 1] + _tot[_n // 2]) / 2
            acum[_nom] = round(_med, 2)
            acum_det[_nom] = {"n": _n, "min": round(_tot[0], 2), "max": round(_tot[-1], 2)}
        else:
            acum[_nom] = 0.0
            acum_det[_nom] = {"n": 0, "min": 0.0, "max": 0.0}
        # ── LA CURVA sigue siendo commit a commit, que es otra pregunta ───────────────────────
        # Aqui multiplicar el promedio del commit SI vale: la curva no dice «cuanto ha mejorado
        # el producto» sino «por donde se movio el recorrido». El titular es el que tenia que
        # dejar de mentir.
        pts, a = [], 1.0
        for k, h in enumerate(_cron):
            ds = [x["delta"] for x in ev.get(h["commit"], [])
                  if x["metrica"] == _nom and x["veredicto"] in ("mejora", "empeora")]
            if ds: a *= (1 + sum(ds) / len(ds))
            pts.append({"x": k, "y": round((a - 1) * 100, 3), "commit": h["commit"],
                        "n": len(ds)})
        series.append({"nombre": _nom, "puntos": pts})
    return {"hitos": hitos, "suelo": suelo,
            "medidos": len({r.get("commit") for r in fs if r.get("commit")}),
            "series": series, "acum": acum, "acum_det": acum_det,
            "etiquetas": [h["commit"] for h in _cron]}


def _suelo_de(m):
    s = (m or {}).get("suelo") or {}
    return s.get("peor")


D["linea"] = _linea_de(_HIST, _suelo_de(D["mayor"]))

# ── LA MISMA LINEA, PERO CONTRA LOS RIVALES ──────────────────────────────────────────────────
#
# La curva de arriba dibuja DELTAS: cuanto cambio un experimento respecto a SI MISMO. Contesta
# «¿mejoro?» y no contesta «¿mejoro hasta donde?». Un −11% acumulado puede dejarte todavia por
# encima de zpaq y un +2% puede sobrarte: sin la cifra absoluta al lado, el recorrido no dice en
# que punto del campo estamos. Esto es la otra mitad.
#
# EL RIVAL ES UNA RECTA A PROPOSITO. Su resultado sobre una entrada dada no cambia nunca —lo
# dice la cabecera de rivales.tsv y por eso se mide una sola vez—, asi que su serie es una
# constante y lo unico que se mueve es lo nuestro. Si la recta del rival subiera o bajara seria
# ruido de nuestra medida, no mejora suya.
#
# NINGUNA CIFRA DE AQUI SE CREE POR ESTAR ESCRITA. Cada punto de la tabla cita commit y fichero,
# y este bloque hace `git show` y busca el numero dentro. El que no aparezca sale marcado.
_RL_PTOS, _RL_MAL = [], 0
for _f in tsv("rivales-linea.tsv")[1:]:
    if len(_f) < 6: continue
    _fe, _mu, _cf, _by, _co, _fu = [_x.strip() for _x in _f[:6]]
    try: _n = int(_by)
    except ValueError: continue
    # Se quitan los puntos porque los documentos escriben 9.657.771 y las tablas 9657771: es la
    # misma cifra con dos trajes, y comparar el traje en vez del numero daria un falso rojo.
    _txt = sh(f"git show {_co}:{_fu}").replace(".", "").replace(" ", "")
    _ok = bool(_txt) and str(_n) in _txt
    if not _ok: _RL_MAL += 1
    _RL_PTOS.append({"fecha": _fe, "muestra": _mu, "config": _cf, "bytes": _n, "commit": _co,
                     "fuente": _fu, "ok": _ok, "de": "documento"})

# Y DESDE LA BASCULA, SOLA: toda fila de historia.jsonl etiquetada `RIVALES:` entra aqui sin
# tocar esta tabla ni este codigo. Es la regla R1 aplicada a la curva — una medida nueva llega a
# la pantalla el dia que se mide. La etiqueta es el contrato: dice «esta fila es el producto tal
# y como se ejecuta», que es lo unico comparable con la constante de un rival.
for _r in _HIST:
    if not (_r.get("etiqueta") or "").startswith("RIVALES:"): continue
    if not _r.get("bytes_out") or not _r.get("muestra"): continue
    _RL_PTOS.append({"fecha": (_r.get("t") or "")[:10], "muestra": _r["muestra"],
                     "config": (_r.get("config") or "defecto").strip() or "defecto",
                     "bytes": int(_r["bytes_out"]), "commit": _r.get("commit") or "?",
                     "fuente": "bench/historia.jsonl", "ok": True, "de": "báscula"})

# Un cuadro por muestra, y solo si hay recorrido que enseñar (dos fechas distintas). Con un solo
# punto no hay linea: hay una foto, y la foto ya esta en «Contra los rivales».
_RL_CUADROS = []
for _mu in sorted({_p["muestra"] for _p in _RL_PTOS}):
    # Dentro del MISMO dia se ordena de mas bytes a menos, que es el orden en que pasaron las
    # cosas: el 27-07 hubo tres medidas y la buena es la ultima. Ordenar al reves haria que la
    # tabla se leyera hacia atras y que «el ultimo punto» —el que se compara con el rival— fuera
    # el peor del dia en vez del que quedo.
    _ps = sorted((_p for _p in _RL_PTOS if _p["muestra"] == _mu),
                 key=lambda p: (p["fecha"], -p["bytes"]))
    _riv = {_k: _v for _k, _v in _todo.get(_mu, {}).items() if not _k.startswith("nuestro")}
    _RL_CUADROS.append({
        "muestra": _mu, "original": _orig.get(_mu), "puntos": _ps,
        "fechas": sorted({_p["fecha"] for _p in _ps}),
        "rivales": [{"nom": _k, "bytes": _v, "clase": D["quien_es"].get(_k, "sin clasificar")}
                    for _k, _v in sorted(_riv.items(), key=lambda kv: kv[1])],
        "dibuja": len({_p["fecha"] for _p in _ps}) > 1,
    })
D["rivlinea"] = {"cuadros": _RL_CUADROS, "sin_verificar": _RL_MAL,
                 "n": len(_RL_PTOS), "muestras": len(_RL_CUADROS)}

D["herramientas"] = tsv("herramientas.tsv")
D["pir_vccm"] = tsv("piramide-vccm.tsv")
D["piramide"] = tsv("piramide.tsv")
D["bucles"] = tsv("bucles.tsv")
D["velreal"] = tsv("velocidad-real.tsv")
D["ablacion"] = tsv("ablacion.tsv")

# NUESTRAS HERRAMIENTAS. El desplegable del menu sale de bench/herramientas.tsv:
# anadir una herramienta es anadir una linea, no tocar codigo. Cada panel sigue
# siendo un fichero independiente con SUS pestanas; esto solo es el salto de una
# a otra. Nada se mezcla: los datos de cada herramienta viven en su panel.
D["paneles"] = tsv("paneles.tsv")
D["yo"] = "vcompresor"
D["perfil"] = tsv("perfil.tsv")
D["censo"] = tsv("censo.tsv")
D["poda"] = tsv("poda.tsv")
# Todo lo que haya en bench/, no una lista escrita a mano. La lista a mano fue justo el fallo:
# 27 ficheros de medidas y el panel leia 10. Una medida que no se ve es una medida que se
# repite.
D["tablas"] = {}
# Y las que estan pero no tienen filas se DICEN, no se tiran. Una tabla vacia no es una tabla
# que no existe: alguien la creo, escribio la cabecera y nadie la ha llenado — eso es un hueco
# del banco, y es informacion. Tirarla en silencio la convertia en nada.
D["tablas_vacias"] = {}
for f in sorted(os.listdir(B)):
    if f.endswith(".tsv"):
        filas = tsv(f)
        if len(filas) > 1: D["tablas"][f[:-4]] = filas
        else: D["tablas_vacias"][f[:-4]] = len(filas)   # 0 = ni cabecera · 1 = solo cabecera
# R2 · EL REGISTRO DE METODOS. Una tabla que mide algo sin decir QUE mide, con que control y
# que la falsaria es, a los tres meses, indistinguible de un numero inventado. El cruce se hace
# aqui, contra las tablas que de verdad existen, para que el panel pueda cantar los tres huecos:
# metodos EN USO SIN DECLARAR, declaraciones A MEDIAS, y metodos declarados y NUNCA MEDIDOS.
D["metodos"] = tsv("metodos.tsv")
_EXIGIDAS = ("que_mide", "como_se_calcula", "control", "que_lo_falsaria")
_VACIO = ("", "-", "—", "?", "pendiente")
_mcab = D["metodos"][0] if D["metodos"] else []
_mfil = D["metodos"][1:] if D["metodos"] else []
_ix = {c: i for i, c in enumerate(_mcab)}
_declaradas = {f[_ix["tabla"]].strip() for f in _mfil if _ix.get("tabla") is not None}
D["metodos_cruce"] = {
    "declarados": len(_mfil),
    "sin_declarar": sorted(t for t in D["tablas"] if t not in _declaradas),
    "sin_medir": sorted(t for t in _declaradas
                        if t not in D["tablas"] and t not in D["tablas_vacias"]
                        and not os.path.exists(os.path.join(B, t + ".jsonl"))),
    "incompletos": [
        {"tabla": f[_ix["tabla"]].strip(),
         "faltan": [c for c in _EXIGIDAS
                    if _ix.get(c) is None or len(f) <= _ix[c]
                    or f[_ix[c]].strip().lower() in _VACIO]}
        for f in _mfil
        if f[_ix["origen"]].strip() != "registro"      # un registro no mide: no se le exige control
        and any(_ix.get(c) is None or len(f) <= _ix[c] or f[_ix[c]].strip().lower() in _VACIO
                for c in _EXIGIDAS)],
    "registros": sorted(f[_ix["tabla"]].strip() for f in _mfil
                        if f[_ix["origen"]].strip() == "registro"),
    "exigidas": list(_EXIGIDAS),
}
D["coste"] = tsv("coste.tsv")
D["talla"] = tsv("talla-poda.tsv")
# Los diez que la v2 poda. Marcarlos en la tabla de coste es lo que convierte una lista de
# numeros en una decision: se ve de un vistazo si lo que se quita es lo que cuesta.
D["podados"] = ["orden-1", "sparse-d2", "sparse-d1+3", "pareja-palabra", "sparse-paso4",
                "sparse-paso8", "sparse-paso2y4", "txt:trigrama", "txt:raiz",
                "txt:trigrama-raices"]
D["decay"] = tsv("decay.tsv")
D["nuestro"] = dict((f[0], int(f[1])) for f in tsv("nuestro.tsv") if len(f) > 1)

# ---- inventario --------------------------------------------------------------------------
# De Cargo.lock, no de `cargo tree`: el panel se genera desde un shell no interactivo donde
# cargo no esta en el PATH, y una dependencia que "no aparece" es peor que una lista fea.
paq = re.findall(r'^name = "(.+?)"\nversion = "(.+?)"',
                 open(os.path.join(RAIZ, "Cargo.lock"), encoding="utf-8").read(), re.M)
# QUE ES NUESTRO SE LEE DEL WORKSPACE, no del nombre. Estaba escrito `n.startswith("vc-")`, y el
# dia que nacio un crate nuestro que no se llama «vc-algo» —vlz— el panel lo conto como
# DEPENDENCIA DE TERCEROS, en la misma lista que zstd y preflate. La cabecera decia «empezamos
# en 60 y estamos en 72» y una de esas 72 era nuestra. Un criterio por prefijo del nombre es una
# convencion de bautizo haciendose pasar por un hecho: el hecho esta en Cargo.toml.
_miembros = set()
for _d in sorted(glob.glob(os.path.join(RAIZ, "crates/*/Cargo.toml"))):
    _m = re.search(r'^name\s*=\s*"(.+?)"', open(_d, encoding="utf-8").read(), re.M)
    if _m: _miembros.add(_m.group(1))
prop = sorted(n for n, _ in paq if n in _miembros)
ext = sorted(f"{n} {v}" for n, v in paq if n not in _miembros)
D["deps"] = {"propios": prop, "externos": ext}
D["loc"] = []
for f in sorted(glob.glob(os.path.join(RAIZ, "crates/*/src/*.rs"))):
    n = sum(1 for _ in open(f, encoding="utf-8", errors="replace"))
    D["loc"].append([os.path.relpath(f, RAIZ), n])
D["loc"].sort(key=lambda x: -x[1])
D["bin"] = []
for f in sorted(glob.glob(os.path.join(RAIZ, "target/release/*"))):
    if os.path.isfile(f) and os.access(f, os.X_OK) and "." not in os.path.basename(f):
        D["bin"].append([os.path.basename(f), os.path.getsize(f)])
D["bin"].sort(key=lambda x: -x[1])

# ---- mapa de bits ------------------------------------------------------------------------
D["mapa"] = []
for p in ("/tmp/mapa2.json", "/tmp/mapa.json", os.path.join(B, "mapa.json")):
    if os.path.exists(p):
        try:
            D["mapa"] = json.load(open(p, encoding="utf-8")); break
        except Exception: pass

# ---- en vuelo ----------------------------------------------------------------------------
D["vuelo"] = []
ps = sh("ps -eo etime=,pcpu=,rss=,args= | grep -E 'vcompresor|vc-fuzz|coste_modelo|json_vs' | grep -v grep")
for l in ps.splitlines()[:8]:
    c = l.split(None, 3)
    if len(c) == 4: D["vuelo"].append({"t": c[0], "cpu": c[1], "rss": int(c[2]) // 1024, "cmd": c[3][:90]})
D["carga"] = sh("cat /proc/loadavg").split(" ")[0] if os.path.exists("/proc/loadavg") else "?"

# ==========================================================================================
HTML = r"""<meta charset="utf-8">
<!-- El fichero SIEMPRE estuvo en UTF-8; lo que faltaba era decirlo. arranca.sh sirve el panel
     con python3 -m http.server, que manda Content-Type SIN charset, asi que el navegador
     adivinaba latin-1 y salian «cÃ³digo» y «29.6Ã—» en toda la pagina. Abriendolo como fichero
     local se veia bien, que es justo lo que hacia que el fallo no se notara: la forma PREVISTA
     de abrirlo —arranca.sh— era la unica rota. -->
<title>vcompresor · banco</title>
<meta name="viewport" content="width=device-width, initial-scale=1">
<style>
:root{
  /* La identidad de la pagina ES la escala del problema: cuantos bits cuesta un byte.
     Frio = predicho, gratis. Brasa = incompresible, va en crudo. Todo lo demas —acento,
     graficas, chips— sale de esa misma rampa para que la pagina y el dato hablen igual. */
  --b0:#141a2e; --b1:#3b2f6e; --b2:#7a3a78; --b3:#b8455c; --b4:#e07a3f; --b5:#f2cf95;
  --acc:#b8455c; --bien:#2f8f68; --ojo:#b8862a; --mal:#c8443c;
  /* LAS SERIES DE LAS GRAFICAS. Cuatro ranuras en orden FIJO — nunca se ciclan ni se reordenan
     por ranking: si un filtro cambia cuantas series hay, las que quedan NO se repintan.
     🚨 Antes la paleta era ["var(--b3)","var(--acc)",...] y --b3 y --acc son EL MISMO #b8455c:
     compresion y tiempo salian con el color identico. No "parecido": el validador da ΔE 0,0,
     indistinguibles incluso con vision normal. Lo vio el dueño en la grafica.
     Estos cuatro estan VALIDADOS con scripts/validate_palette.js, no elegidos a ojo: en oscuro
     pasan las cinco comprobaciones con TODOS los pares (el peor, teal↔carmin, ΔE 10,0 en deutan
     y 15,4 en vision normal). En claro pasan con un aviso de 6,4 en deutan, que solo es legal
     con codificacion secundaria — de ahi los patrones de trazo de `curva()`. */
  --s1:#b8455c; --s2:#5b8de8; --s3:#2aa198; --s4:#b08d20;
  --fondo:#0c0e15; --panel:#12151f; --panel2:#171b28; --linea:#232839;
  --tx:#e7e4ef; --tx2:#a49db6; --tx3:#6e6880;
  --mono:ui-monospace,SFMono-Regular,"SF Mono",Menlo,Consolas,"Liberation Mono",monospace;
  --sans:system-ui,-apple-system,"Segoe UI",Roboto,sans-serif;
  --r:3px;
}
/* Los cuatro tonos de serie se RE-ESCALONAN para el fondo claro, no se heredan: sobre blanco los
   mismos hex se agrisan y el validador lo canta (el teal cae por debajo del suelo de croma).
   Estos son su version para #fff, validada aparte. */
@media (prefers-color-scheme:light){
  :root{--fondo:#f7f5f9;--panel:#fff;--panel2:#faf8fb;--linea:#e3dee9;
        --tx:#191520;--tx2:#635b71;--tx3:#8d859b;--b0:#2a3560;
        --s1:#c04a63;--s2:#3a7ede;--s3:#009c8c;--s4:#a08205;}
}
:root[data-theme=light]{--fondo:#f7f5f9;--panel:#fff;--panel2:#faf8fb;--linea:#e3dee9;
        --tx:#191520;--tx2:#635b71;--tx3:#8d859b;--b0:#2a3560;
        --s1:#c04a63;--s2:#3a7ede;--s3:#009c8c;--s4:#a08205;}
:root[data-theme=dark]{--fondo:#0c0e15;--panel:#12151f;--panel2:#171b28;--linea:#232839;
        --tx:#e7e4ef;--tx2:#a49db6;--tx3:#6e6880;--b0:#141a2e;
        --s1:#b8455c;--s2:#5b8de8;--s3:#2aa198;--s4:#b08d20;}

*{box-sizing:border-box}
body{margin:0;background:var(--fondo);color:var(--tx);font-family:var(--sans);
     font-size:14px;line-height:1.55;-webkit-font-smoothing:antialiased}
.env{max-width:1240px;margin:0 auto;padding:0 20px 80px}

/* ---- cabecera: la barra de instrumentos ---- */
header{border-bottom:1px solid var(--linea);background:var(--panel);
       position:sticky;top:0;z-index:20}
.hb{max-width:1240px;margin:0 auto;padding:14px 20px;display:flex;flex-wrap:wrap;
    align-items:baseline;gap:6px 18px}
h1{font-family:var(--mono);font-size:15px;font-weight:700;letter-spacing:-.02em;margin:0}
h1 b{color:var(--acc)}
.meta{font-family:var(--mono);font-size:11px;color:var(--tx3);display:flex;gap:12px;
      flex-wrap:wrap;margin-left:auto}
.kpis{max-width:1240px;margin:0 auto;padding:0 20px 14px;display:grid;gap:1px;
      grid-template-columns:repeat(auto-fit,minmax(148px,1fr));background:var(--linea);
      border:1px solid var(--linea);border-radius:var(--r);overflow:hidden}
.kpi{background:var(--panel);padding:11px 13px}
/* MARCADOR: la fila de contadores grandes de cada seccion. Existe porque escribi siete
   secciones usandola antes de comprobar que estuviera, y salian todas apiladas en columna. */
.marc{display:grid;gap:1px;grid-template-columns:repeat(auto-fit,minmax(160px,1fr));
      background:var(--linea);border:1px solid var(--linea);border-radius:var(--r);
      overflow:hidden;margin:0 0 16px}
.tar{background:var(--panel2);padding:12px 14px}
.tar .k{font-family:var(--mono);font-size:26px;font-weight:700;letter-spacing:-.03em;
        line-height:1.1;color:var(--tx)}
.tar .e{font-size:11px;color:var(--tx2);margin-top:3px;line-height:1.35}
/* Hay un td{text-align:right} global del que solo se libra la primera columna. Cuando el texto
   va en la segunda (porque la primera lleva el estado), hace falta decirlo. */
td.izq,th.izq{text-align:left;white-space:normal}
.kpi .l{font-family:var(--mono);font-size:10px;text-transform:uppercase;letter-spacing:.09em;
        color:var(--tx3)}
.kpi .v{font-family:var(--mono);font-size:22px;font-weight:700;letter-spacing:-.03em;
        font-variant-numeric:tabular-nums;line-height:1.25}
.kpi .s{font-size:11px;color:var(--tx2)}

/* ---- modulos ---- */
section{margin-top:26px;border:1px solid var(--linea);border-radius:var(--r);
        background:var(--panel);overflow:hidden}
.cab{padding:11px 15px;border-bottom:1px solid var(--linea);display:flex;
     align-items:baseline;gap:12px;flex-wrap:wrap;background:var(--panel2)}
.cab h2{font-family:var(--mono);font-size:11px;font-weight:700;text-transform:uppercase;
        letter-spacing:.11em;margin:0;color:var(--tx)}
.cab .d{font-size:12px;color:var(--tx2);flex:1;min-width:200px}
.cpo{padding:15px}
.nota{font-size:12.5px;color:var(--tx2);max-width:74ch;margin:0 0 12px}
.nota:last-child{margin-bottom:0}
.nota b{color:var(--tx);font-weight:600}

/* ---- tablas ---- */
.env-t{overflow-x:auto;margin:0 -15px;padding:0 15px}
table{border-collapse:collapse;width:100%;font-size:12.5px;
      font-variant-numeric:tabular-nums}
th{font-family:var(--mono);font-size:10px;text-transform:uppercase;letter-spacing:.08em;
   color:var(--tx3);text-align:right;padding:5px 9px;border-bottom:1px solid var(--linea);
   font-weight:600;white-space:nowrap}
th:first-child,td:first-child{text-align:left}
td{padding:5px 9px;border-bottom:1px solid var(--linea);text-align:right;white-space:nowrap}
tbody tr:last-child td{border-bottom:none}
tbody tr:hover td{background:var(--panel2)}
td.n,th.n{font-family:var(--mono)}
td.t{white-space:normal}
.mut{color:var(--tx3)}
.bien{color:var(--bien)} .mal{color:var(--mal)} .ojo{color:var(--ojo)}
.chip{display:inline-block;font-family:var(--mono);font-size:10px;padding:1px 6px;
      border-radius:9px;border:1px solid currentColor;letter-spacing:.04em}
.barra{height:5px;border-radius:3px;background:var(--linea);overflow:hidden;min-width:54px}
.barra i{display:block;height:100%;background:var(--acc)}

/* ---- mapa de bits ---- */
.nav{display:flex;gap:2px;flex-wrap:wrap;border-bottom:1px solid var(--linea);
  margin:0 0 18px;position:sticky;top:0;background:var(--fondo);z-index:20;padding-top:6px}
.nav button{font-family:var(--mono);font-size:12px;letter-spacing:.02em;padding:9px 15px;
  border:0;border-bottom:2px solid transparent;background:none;color:var(--tx3);cursor:pointer}
.nav button:hover{color:var(--tx2)}
.nav button[aria-selected=true]{color:var(--tx);border-bottom-color:var(--acc)}
.nav button:focus-visible{outline:2px solid var(--acc);outline-offset:-2px}
section[data-t]{display:none}
section[data-t].on{display:block}
.tabs{display:flex;gap:5px;flex-wrap:wrap}
.tabs button{font-family:var(--mono);font-size:11px;padding:3px 11px;border-radius:11px;
  border:1px solid var(--linea);background:transparent;color:var(--tx2);cursor:pointer}
.tabs button[aria-selected=true]{background:var(--acc);border-color:var(--acc);color:#fff}
.tabs button:focus-visible{outline:2px solid var(--acc);outline-offset:2px}
canvas{display:block;width:100%;image-rendering:pixelated;border-radius:var(--r)}
#tira{height:132px;background:var(--b0)}
#hist{height:104px}
.leyenda{display:flex;align-items:center;gap:9px;margin-top:9px;font-family:var(--mono);
         font-size:10px;color:var(--tx3)}
.rampa{flex:1;height:8px;border-radius:4px;
  background:linear-gradient(90deg,var(--b0),var(--b1),var(--b2),var(--b3),var(--b4),var(--b5))}
.dos{display:grid;gap:15px;grid-template-columns:1fr 1fr}
@media(max-width:820px){.dos{grid-template-columns:1fr}}
pre.frag{font-family:var(--mono);font-size:11px;line-height:1.45;margin:0;padding:9px 11px;
  background:var(--panel2);border:1px solid var(--linea);border-left:3px solid var(--acc);
  border-radius:var(--r);white-space:pre-wrap;word-break:break-all;max-height:112px;
  overflow:auto;color:var(--tx2)}
pre.frag.frio{border-left-color:var(--b1)}
.frag-t{font-family:var(--mono);font-size:10px;text-transform:uppercase;letter-spacing:.08em;
        color:var(--tx3);margin:11px 0 5px;display:flex;justify-content:space-between}
.viv{display:flex;align-items:center;gap:7px;font-family:var(--mono);font-size:11px}
.pt{width:7px;height:7px;border-radius:50%;background:var(--bien);flex:none;display:inline-block;
    box-shadow:0 0 0 3px color-mix(in srgb,var(--bien) 22%,transparent)}
.pt.malo{background:var(--mal);box-shadow:0 0 0 3px color-mix(in srgb,var(--mal) 22%,transparent)}
.vivo-ind{display:inline-flex;align-items:center;gap:6px;color:var(--tx2)}
@media (prefers-reduced-motion:no-preference){
  .pt{animation:lat 2.4s ease-in-out infinite}
  .pt.malo{animation:none}
  @keyframes lat{0%,100%{opacity:1}50%{opacity:.35}}
}
code{font-family:var(--mono);font-size:11.5px;background:var(--panel2);padding:1px 5px;
     border-radius:2px;border:1px solid var(--linea)}
pre.uso{font-family:var(--mono);font-size:11.5px;line-height:1.6;margin:0;padding:12px 14px;
  background:var(--panel2);border:1px solid var(--linea);border-radius:var(--r);
  overflow-x:auto;color:var(--tx2)}
pre.uso b{color:var(--acc);font-weight:600}
.met{background:var(--panel2);border:1px solid var(--linea);border-radius:var(--r);
     padding:12px 14px;margin-top:10px}
.kv{display:grid;grid-template-columns:126px minmax(0,1fr);gap:5px 14px;margin-top:9px;font-size:12.5px}
.kv .k{color:var(--tx3);font-size:10.5px;text-transform:uppercase;letter-spacing:.6px;padding-top:3px}
@media(max-width:620px){.kv{grid-template-columns:1fr}}

/* El selector de herramienta. Va en el titulo porque el titulo ES la herramienta
   que estas mirando: si el salto estuviera escondido en un pie, nadie saltaria. */
.selp{position:relative;display:inline-block;cursor:pointer;color:var(--tx)}
.selp::after{content:"▾";font-size:.7em;margin-left:5px;color:var(--tx3);vertical-align:middle}
.selp:hover{color:var(--acc)}
.selp-menu{position:absolute;top:100%;left:0;z-index:50;min-width:330px;margin-top:6px;
  background:var(--panel);border:1px solid var(--linea);border-radius:var(--r);
  box-shadow:0 10px 30px rgba(0,0,0,.35);padding:5px;display:none;text-align:left}
.selp-menu.on{display:block}
.selp-menu a{display:block;padding:8px 10px;border-radius:var(--r);text-decoration:none;
  color:var(--tx);font-size:13px;font-weight:600;line-height:1.35}
.selp-menu a:hover{background:var(--panel2)}
.selp-menu a .q{display:block;font-size:11.5px;font-weight:400;color:var(--tx2);margin-top:2px}
.selp-menu a.yo{color:var(--acc)}
.selp-menu .aqui{font-size:10.5px;color:var(--tx3);font-weight:400}
.selp-menu .pie{padding:7px 10px 4px;font-size:10.5px;color:var(--tx3);border-top:1px solid var(--linea);
  margin-top:4px}

/* Graficas. SVG a pelo, sin libreria: el panel no pide nada a la red (R9). */
.gr{margin:10px 0 4px}
.gr svg{width:100%;height:auto;display:block}
.gr .rejilla{stroke:var(--linea);stroke-width:1}
.gr .eje{stroke:var(--linea2,var(--linea));stroke-width:1.5}
.gr .cero{stroke:var(--tx3);stroke-width:1.5;stroke-dasharray:4 3}
.gr .etq{fill:var(--tx3);font-size:10px;font-family:var(--mono)}
.gr .etq2{fill:var(--tx2);font-size:11px}

/* El salto entre el panel principal y el de cada herramienta. Va en la cabecera y no en las
   pestañas a proposito: las pestañas son secciones de ESTA pagina, esto son PAGINAS. */
.subs{display:flex;gap:2px;flex-wrap:wrap;margin-top:9px}
.subs button{font:500 11px/1 var(--mono);color:var(--tx3);cursor:pointer;
  padding:6px 11px;border:1px solid var(--linea);border-radius:5px;background:var(--fondo2)}
.subs button:hover{color:var(--tx);border-color:var(--tx3)}
.subs button.aqui{color:var(--fondo);background:var(--tx);border-color:var(--tx)}
</style>

<header>
  <div class="hb">
    <h1><span class="selp" id="selp"></span> · <b>banco de pruebas</b></h1>
    <nav class="subs" id="subs"></nav>
    <div class="meta">
      <span id="m-commit"></span><span id="m-host"></span><span id="m-datos"></span><span id="m-gen"></span>
    </div>
  </div>
  <div class="kpis" id="kpis"></div>
</header>

<div class="env">
  <nav class="nav" id="nav" role="tablist"></nav>

  <section>
    <div class="cab"><h2>Mapa de bits</h2>
      <div class="d">Dónde se va cada bit del fichero. Cada celda es un tramo; el color, lo que cuesta.</div>
      <div class="tabs" id="tabs"></div></div>
    <div class="cpo" id="mapa-cpo"></div>
  </section>

  <section>
    <div class="cab"><h2>Contra los rivales</h2>
      <div class="d">Se miden una vez y no se vuelven a medir: el resultado de un rival sobre una entrada es constante.</div></div>
    <div class="cpo" id="rivales"></div>
  </section>

  <section>
    <div class="cab"><h2>El registro de métodos</h2>
      <div class="d">Qué mide cada tabla del banco, con qué control y qué la falsaría. Una
        medida sin eso escrito es, a los tres meses, indistinguible de un número inventado.</div></div>
    <div class="cpo" id="metodos"></div>
  </section>

  <section>
    <div class="cab"><h2>Libro mayor de modelos</h2>
      <div class="d">Qué aporta y qué cuesta cada grupo. Se apaga y se mide: lo demás es opinión.</div></div>
    <div class="cpo" id="libro"></div>
  </section>

  <section>
    <div class="cab"><h2>Dónde se va el tiempo</h2>
      <div class="d">Perfil por función y censo de tablas — el reparto de memoria explica el reparto de tiempo.</div></div>
    <div class="cpo" id="tiempo"></div>
  </section>

  <section>
    <div class="cab"><h2>La aguja</h2>
      <div class="d">La misma batería en cada commit. Lo que importa no es una métrica: es si al mover una se movieron las otras.</div></div>
    <div class="cpo" id="aguja"></div>
    <a id="v-aguja"></a>
  </section>

  <section>
    <div class="cab"><h2>El carril ZIP</h2>
      <div class="d">Seis hipótesis, cinco falsas. El premio estaba en los miembros que el ZIP guarda sin comprimir.</div></div>
    <div class="cpo" id="zip"></div>
  </section>

  <section>
    <div class="cab"><h2>vdeflate — nuestro DEFLATE</h2>
      <div class="d"><b>No es un compresor: es un reproductor.</b> Coge un DEFLATE que ya existe
        —dentro de un PNG, un ZIP, un .docx—, lo desmonta hasta las decisiones que tomó su
        codificador y lo vuelve a montar <b>byte a byte</b>. Existe porque preflate es el
        <b>78% del reloj</b> de todo el archivador, es de terceros, y falla en el <b>8,7%</b> de
        los flujos reales entrando en pánico.</div></div>
    <div class="cpo" id="vdeflate"></div>
  </section>

  <section>
    <div class="cab"><h2>Las piezas: lo de fuera y lo nuestro</h2>
      <div class="d">Todo lo de terceros que usamos <b>o hemos usado</b>, con qué lo hemos
        sustituido y en qué estado está. Con acceso directo a la pirámide y la aguja de cada una.</div></div>
    <div class="cpo" id="piezas"></div>
  </section>

  <section>
    <div class="cab"><h2>Nuestras herramientas</h2>
      <div class="d">Cada pieza que hacemos nuestra es un proyecto aparte: con su competencia fuera,
        su banco propio y sus preguntas. Aquí una ficha por cada una, para mejorarlas
        <b>campo a campo</b> en vez de mirar la media.</div></div>
    <div class="cpo" id="herr"></div>
  </section>

  <section>
    <div class="cab"><h2>La pirámide de vc-cm — nuestra alternativa a zstd</h2>
      <div class="d">Su bucle caliente va <b>por bit</b>, así que aquí no se mide con cronómetros:
        el instrumento costaría más que lo medido. Se mide por <b>muestreo</b>, que no toca el
        código. Cada fila dice de dónde sale.</div></div>
    <div class="cpo" id="pirvccm"></div>
    <a id="v-vc-cm"></a>
  </section>

  <section>
    <div class="cab"><h2>La pirámide</h2>
      <div class="d">Todo el gasto de recursos, del primer dato al programa completo. Cada peldaño
        con su <b>reloj</b> y su <b>CPU</b> por separado, y si va <b>en serie</b> o <b>en paralelo</b>
        — porque un peldaño paralelo gasta CPU y uno de la serie gasta <b>espera</b>, que es lo
        que se nota.</div></div>
    <div class="cpo" id="piramide"></div>
    <a id="v-archivador"></a>
  </section>

  <section>
    <div class="cab"><h2>Los bucles, en orden</h2>
      <div class="d">En qué <code>for</code> empieza la lentitud. Se lee de arriba abajo y se para
        en el primero que cuesta de más.</div></div>
    <div class="cpo" id="bucles"></div>
  </section>

  <section>
    <div class="cab"><h2>Contra todos los archivadores conocidos</h2>
      <div class="d">Misma máquina, mismas entradas, sólo tamaños. Y marcado <b>quién es un producto
        de verdad</b> y quién no, que cambia cómo se lee la tabla entera.</div></div>
    <div class="cpo" id="comparativa"></div>
  </section>

  <section>
    <div class="cab"><h2>El dinero</h2>
      <div class="d">Cuánto se ahorra en euros por PB y año, y qué ajuste conviene a cada precio
        de almacenamiento.</div></div>
    <div class="cpo" id="dinero"></div>
  </section>

  <section>
    <div class="cab"><h2>Velocidad real contra la que decimos tener</h2>
      <div class="d">La tabla que usa <code>--price</code> para poner precio a la CPU se midió sobre
        <b>una</b> muestra de 421 MB. Esto es el archivador entero sobre un árbol de 2,7 GB y
        43.298 ficheros.</div></div>
    <div class="cpo" id="velreal"></div>
  </section>

  <section>
    <div class="cab"><h2>Qué se decidió, y con qué medida</h2>
      <div class="d">La contraparte que faltaba. Había registro de <b>lo aplicado</b> y ninguno de
        <b>lo rechazado</b>, así que los negativos vivían en comentarios del código y en cuerpos de
        commit — donde nadie los puede recomputar. <b>Ni una cifra de aquí está escrita a mano</b>:
        salen todas del libro mayor. Lo que se declara a mano es la <i>identidad</i> del
        experimento, y no lleva números a propósito: el 03-08 se publicó un negativo del
        <b>+3,1%</b> calculado sobre tres muestras cuando en el libro había cuatro pares y el
        descartado era el peor — el agregado real es <b>+4,3%</b>. Una cifra que se puede escribir
        en dos sitios acaba diciendo dos cosas.</div></div>
    <div class="cpo" id="decisiones"></div>
  </section>

  <section>
    <div class="cab"><h2>Qué está aplicado y en qué versión estamos</h2>
      <div class="d">Las versiones salen <b>leídas del código fuente</b>, no escritas a mano: si
        un ancla no aparece, el generador se para en vez de enseñar una versión inventada. Y la
        lista de commits lleva <b>cuántas medidas suyas hay en el banco</b>, porque un commit sin
        medidas no puede salir en la curva de abajo — y entonces la curva sale plana y parece que
        no se hizo nada, que es justo lo contrario de lo que pasó.</div></div>
    <div class="cpo" id="aplicado"></div>
  </section>

  <section>
    <div class="cab"><h2>Qué mejoró y qué empeoró, commit a commit</h2>
      <div class="d">El recorrido. Sólo se compara <b>el mismo experimento</b> —misma etiqueta,
        misma muestra, misma configuración— medido en dos commits distintos: restar dos medidas
        que no son lo mismo da un número grande y sin significado. Un delta por debajo del suelo
        de ruido <b>no es una mejora</b>, es la máquina respirando; y una punta con el árbol
        sucio no se compara, porque ahí el commit no identifica el código.</div></div>
    <div class="cpo" id="linea"></div>
  </section>

  <section>
    <div class="cab"><h2>Contra los rivales, en el tiempo</h2>
      <div class="d">La curva de arriba dice <b>cuánto cambió cada experimento respecto a sí
        mismo</b>; ésta dice <b>dónde nos deja eso</b>. Los rivales van en recta porque su
        resultado sobre una entrada dada <b>es una constante</b>: se miden una vez y lo único que
        se mueve es lo nuestro. Cada punto nuestro lleva el commit y el fichero de donde sale, y
        el generador <b>lo busca dentro con <code>git show</code></b> antes de dibujarlo.</div></div>
    <div class="cpo" id="rivlinea"></div>
  </section>

  <section>
    <div class="cab"><h2>La herramienta, de un vistazo</h2>
      <div class="d">Qué es, a quién sustituye, <b>qué se ha medido de ella</b> y qué preguntas
        del camino se le han hecho. Todo lo de esta página es suyo y sólo suyo: si un número no
        aparece aquí es que <b>no se ha medido</b>, no que esté en la página de al lado.</div></div>
    <div class="cpo" id="ficha"></div>
  </section>

  <section>
    <div class="cab"><h2>Nuestras herramientas, una por una</h2>
      <div class="d">Qué hemos construido nosotros, a quién sustituye, cómo va y <b>dónde están
        sus datos</b>. Cada una se puede mirar y trabajar por separado: sus preguntas son suyas,
        sus tablas son suyas y sus medidas son suyas. Mezclarlas ya salió caro una vez.</div></div>
    <div class="cpo" id="propias"></div>
  </section>

  <section>
    <div class="cab"><h2>Las junturas</h2>
      <div class="d"><b>La ley:</b> todas las herramientas conectadas y pasándose la información
        con ida y vuelta perfecta. Esta vista existe porque el fallo más caro que hemos
        encontrado <b>no vivía dentro de ninguna pieza</b>: el archivador daba frames de 64 MiB y
        le pasaba al códec una llamada sin parámetros, así que el códec miraba 4 MiB hacia atrás.
        Las dos piezas medían bien por separado. <b>Una báscula por herramienta es ciega a la
        juntura por construcción.</b></div></div>
    <div class="cpo" id="junturas"></div>
  </section>

  <section>
    <div class="cab"><h2>El techo conocido</h2>
      <div class="d">Qué es lo mejor que se sabe hacer en cada cosa que hacemos, quién lo tiene y
        si podemos pasarlo. <b>Dos reglas de admisión:</b> un techo sin fuente no es un techo, y
        las dos cifras tienen que ser la misma magnitud — comparar nuestra cota superior contra
        el resultado real de otro ya produjo hoy un «no se puede» que era falso.</div></div>
    <div class="cpo" id="techos"></div>
  </section>

  <section>
    <div class="cab"><h2>Agujeros</h2>
      <div class="d">Lo que sabemos, lo que sabemos a medias y <b>lo que no sabemos</b>. Una pregunta
        respondida señala el fichero del banco donde vive el número; una abierta señala la prueba
        que la cerraría. Una pregunta sin prueba que la cierre no es una pregunta, es una queja.</div></div>
    <div class="cpo" id="agujeros"></div>
  </section>

  <section>
    <div class="cab"><h2>Las preguntas del camino</h2>
      <div class="d">Lo que hay que preguntarle a algo <b>que ya funciona</b>. Los agujeros son
        datos que faltan; esto es otra cosa: la pieza funciona, las pruebas pasan y el archivo
        es correcto — lo que faltaba era haber preguntado. <b>La pregunta vale más que el
        hallazgo</b>, porque se le puede hacer a la siguiente pieza, y a un proyecto de cliente
        que no tiene nada que ver con éste.</div></div>
    <div class="cpo" id="camino"></div>
  </section>

  <section>
    <div class="cab"><h2>Dónde se han hecho ya</h2>
      <div class="d">Las preguntas de arriba, corridas sobre <b>cada pieza nuestra</b>. En gris,
        lo que todavía no se ha mirado: una auditoría que sólo enseña lo que encontró parece
        completa y no lo está.</div></div>
    <div class="cpo" id="matriz"></div>
  </section>

  <section>
    <div class="cab"><h2>El resultado es correcto, el camino no</h2>
      <div class="d">Lo que encontraron las preguntas de arriba. <b>Ninguna de estas líneas es
        un fallo</b>: por eso no las encuentra nada de lo que ya teníamos — los tests miran la
        salida, la aguja mira si algo empeoró respecto a ayer, y la pirámide mira dónde se va el
        tiempo. Un gasto que lleva ahí desde el primer día y nunca ha empeorado es invisible
        para las tres.</div></div>
    <div class="cpo" id="desperdicio"></div>
  </section>

  <section>
    <div class="cab"><h2>¿Qué carril se paga, y a qué precio?</h2>
      <div class="d">Cada carril se apaga y se mide. Lo que <b>compra</b> son bytes que no se
        guardan —y el disco se alquila todos los meses—; lo que <b>cuesta</b> es CPU, que se paga
        una vez. Por eso la respuesta no es «este carril es bueno», sino <b>«se paga por encima
        de tal precio y tal plazo»</b>.</div></div>
    <div class="cpo" id="carriles"></div>
  </section>

  <section>
    <div class="cab"><h2>Cinco formas de ir más rápido</h2>
      <div class="d">El censo de técnicas, ordenado por <b>lo que ataca</b> cada una y no por lo
        ingeniosa que es. Las dos primeras familias —no hacerlo, no repetirlo— tienen techos
        mucho más altos que las otras tres, y casi siempre se atacan al revés: se empieza
        optimizando el bucle y se acaba descubriendo que el bucle sobraba.</div></div>
    <div class="cpo" id="tecnicas"></div>
  </section>

  <section>
    <div class="cab"><h2>Precomputar: cuándo sí y cuándo empeora</h2>
      <div class="d">La norma general. Tres reglas deciden si <i>funciona</i> y una cuarta si
        <i>merece la pena</i>, que no es lo mismo — y aquí hay un caso propio donde cumplirlas a
        medias hizo la pieza más lenta, no más rápida.</div></div>
    <div class="cpo" id="precomputar"></div>
  </section>

  <section>
    <div class="cab"><h2>El libro mayor, agregado y auditado</h2>
      <div class="d">Las mismas medidas de siempre, pero por <b>experimento</b> en vez de por
        ejecución: cuántas vueltas se le dieron, cuánto varió y si se puede volver a correr. Una
        cifra sin su <i>n</i> no se puede discutir.</div></div>
    <div class="cpo" id="mayor"></div>
  </section>

  <section>
    <div class="cab"><h2>Cómo se resuelve</h2>
      <div class="d">El orden no es por lo fea que es cada cosa: es por <b>lo que se sabe de
        ella</b>. Y cada línea lleva el instrumento que puede verla — el suelo de ruido del banco
        es 2,05%, así que la mayoría <b>no se puede comprobar con el reloj</b>: hay que
        contarlas.</div></div>
    <div class="cpo" id="plan"></div>
  </section>

  <section>
    <div class="cab"><h2>Robustez y puntos ciegos</h2>
      <div class="d">Lo que el banco no medía: descomprimir, casos raros, archivos corruptos — y lo que sigue sin medirse.</div></div>
    <div class="cpo" id="robusto"></div>
  </section>

  <section>
    <div class="cab"><h2>Precisión del propio banco</h2>
      <div class="d">Cuánto ruido tiene una medida. Sin esto, cualquier diferencia pequeña parece un hallazgo.</div></div>
    <div class="cpo" id="ruido"></div>
  </section>

  <section>
    <div class="cab"><h2>Todas las medidas</h2>
      <div class="d">Cada tabla del banco, en crudo. Lo que no aparece arriba, aparece aquí.</div></div>
    <div class="cpo" id="todas"></div>
  </section>

  <section>
    <div class="cab"><h2>Ejecuciones</h2>
      <div class="d">Cada medida que pasó por la báscula: bytes, segundos, CPU, memoria pico y ciclos.</div></div>
    <div class="cpo" id="ejec"></div>
  </section>

  <section>
    <div class="cab"><h2>Cómo leer este panel</h2>
      <div class="d">Qué número manda, cuál es decorado y qué no se puede afirmar con lo que hay.</div></div>
    <div class="cpo" id="comoleer"></div>
  </section>

  <section>
    <div class="cab"><h2>Inventario</h2>
      <div class="d">Todo lo que entra en el binario, y de dónde viene.</div></div>
    <div class="cpo" id="inv"></div>
  </section>
</div>

<script>
/* Los datos van INCRUSTADOS para que la pagina funcione abierta como fichero suelto, sin
   servidor y sin red. Y si ademas se esta sirviendo por http, se re-piden cada pocos segundos
   y la pagina se REPINTA en sitio: sin recargar, sin perder el scroll ni la pestana elegida.
   Recargar entera seria una linea menos de codigo y una experiencia peor. */
let RAW = __DATOS__;

/* ── QUE HERRAMIENTA SE ESTA MIRANDO ────────────────────────────────────────────────────────
   El panel es UNO y ensena una herramienta cada vez. `D` no es el fichero de datos: es la vista
   armada = lo comun + lo de la herramienta activa. Las medidas de vdeflate y las del archivador
   NO conviven en `D` en ningun momento — viven en ramas distintas de RAW y solo se copia la que
   miras. Eso es lo que impide que una cifra cruce de herramienta: no esta ahi. */
const _VISTA_POR_DEFECTO = "principal";
let ACTIVA = _VISTA_POR_DEFECTO;
let D = {};
function arma(){
  const v = RAW.vistas[ACTIVA] || RAW.vistas[_VISTA_POR_DEFECTO] || {};
  D = Object.assign({}, RAW.comun, v);
  D.yo = ACTIVA;
}
/* La MISMA regla que _visible() en python. Escrita dos veces —una por lenguaje— y comprobada:
   la verificacion cuenta las secciones que python cree visibles y falla si alguna no sale. */
function visible(s){
  const d = s.dataset.h;
  if (d === "**") return true;
  if (d === "*")  return ACTIVA !== "principal";
  return d === ACTIVA;
}
/* La herramienta tambien va en la url: #vlz/camino abre vlz por su camino. Un enlace a una
   herramienta tiene que poder mandarse, igual que uno a una pestaña. */
(function(){
  const h = decodeURIComponent((location.hash || "").replace(/^#/, ""));
  const q = h.includes("/") ? h.slice(0, h.indexOf("/")) : h;
  if (q && RAW.vistas[q]) ACTIVA = q;
})();
arma();
/* EL HUECO SILENCIOSO, a proposito. Cada pagina lleva TODO el javascript pero solo
   ALGUNAS secciones, asi que un renderizador puede no encontrar su sitio. Devolviendo un
   elemento suelto escribe en el vacio y no pasa nada: sin esto, cada sub-panel saldria con un
   aviso de «once secciones no se han podido pintar» que seria falso y ademas asustaria. */
const _VACIO = document.createElement("div");
const $ = s => document.querySelector(s) || _VACIO;
const fm = n => n==null ? "—" : n.toLocaleString("es-ES");
const pc = (n,d=2) => (n>0?"+":"") + n.toFixed(d) + "%";

/* La rampa termica. 0 bits = frio (predicho, gratis); 8 bits = brasa (crudo).
   La referencia son 4 bits, no 8: por encima de 4 casi no hay nada y con la escala completa
   el mapa saldria todo negro y no diria nada. La gamma abre la parte baja, que es donde
   ocurre TODO. Va escrito en la leyenda para que nadie lo lea como una escala lineal. */
const RAMPA = [[20,26,46],[59,47,110],[122,58,120],[184,69,92],[224,122,63],[242,207,149]];
function color(bpb){
  let t = Math.pow(Math.min(1, Math.max(0,bpb)/4), 0.55) * (RAMPA.length-1);
  const i = Math.min(RAMPA.length-2, Math.floor(t)), f = t-i;
  const a = RAMPA[i], b = RAMPA[i+1];
  return `rgb(${a.map((v,k)=>Math.round(v+(b[k]-v)*f)).join(",")})`;
}

let sel = 0;

/* ---- blindaje de secciones ----------------------------------------------------------------
   Cada seccion se pinta DENTRO de `seguro`. Motivo, y es de hoy: la seccion de piezas propias
   leia una columna que no estaba, hacia f[-1].includes(...) y reventaba — y como todo iba
   seguido, se llevaba por delante el resto del panel Y el modo en vivo. La pagina parecia
   entera y estaba muerta a la mitad.
   Una seccion rota se cae SOLA. Y se DICE: tragarse el error en silencio dejaria un panel que
   parece bien y esconde lo que no pudo pintar, que es peor que el fallo. */
const fallosSeccion = [];
function seguro(fn){
  try { fn(); }
  catch (e) {
    const m = (e && e.message) || String(e);
    if (fallosSeccion.indexOf(m) < 0) fallosSeccion.push(m);
    console.error("panel · seccion rota:", e);
  }
}
/* La carga de la maquina, refrescada en sitio. Es lo unico del panel que cambia sin que haya
   pasado nada en el banco, asi que es lo unico que se actualiza sin repintar. */
function refrescaCarga(n){
  const c = document.getElementById("kpi-carga");
  if (!c || !n || !n.carga) return;
  const v = c.querySelector(".v"), s = c.querySelector(".s");
  if (v) { v.textContent = n.carga; v.className = "v " + (+n.carga > 4 ? "ojo" : "bien"); }
  if (s) s.textContent = (n.vuelo || []).length + " procesos en vuelo";
}
function avisaFallos(){
  const env = document.querySelector(".env");
  let c = document.getElementById("fallos");
  if (!fallosSeccion.length) { if (c) c.remove(); return; }
  if (!c) { c = document.createElement("div"); c.id = "fallos"; env.prepend(c); }
  c.innerHTML = `<b>${fallosSeccion.length} sección(es) no se han podido pintar.</b>
    El resto del panel sí: una sección rota ya no tumba la página. Lo que falló:
    ${fallosSeccion.map(m => "<code>" + esc(m) + "</code>").join(" · ")}`;
}

function esc(s){ return String(s).replace(/[&<>]/g, c=>({"&":"&amp;","<":"&lt;",">":"&gt;"}[c])); }

/* ---- el selector de herramienta ------------------------------------------------------------
   Cada herramienta tiene SU panel con SUS pestañas; esto es solo el salto de una a otra. No se
   mezclan datos: el panel de vcompresor no lleva dentro ni un byte de NERVIO, y al reves igual
   —tienen que poder abrirse y defenderse por separado, y NERVIO ademas esta antes de la
   solicitud de prioridad—.
   El enlace depende del protocolo con el que estas mirando: a doble clic (file://) se usa la
   ruta relativa entre carpetas hermanas; servido por http, la url con su puerto. Va escrito en
   el propio menu para que el criterio se pueda leer, no adivinar. */
function pintaHerramientas(){
  const cab = (D.paneles||[])[0] || [];
  const F = (D.paneles||[]).slice(1);
  const c = n => cab.indexOf(n);
  const yo = F.find(f => f[c("slug")] === D.yo) || null;
  const el = $("#selp");
  if (!el) return;
  el.textContent = yo ? yo[c("nombre")] : "vcompresor";
  if (!F.length) { el.style.cursor = "default"; return; }
  const esFichero = location.protocol === "file:";
  const menu = document.createElement("div");
  menu.className = "selp-menu";
  menu.innerHTML = F.map(f => {
    const mio = f[c("slug")] === D.yo;
    const destino = esFichero ? f[c("ruta_file")] : f[c("url_http")];
    return `<a href="${esc(destino)}" class="${mio?"yo":""}">${esc(f[c("nombre")])}${
      mio ? '<span class="aqui"> · estás aquí</span>' : ""}
      <span class="q">${esc(f[c("que_es")])}</span></a>`;
  }).join("") + `<div class="pie">Cada herramienta tiene su propio panel, con sus pestañas y sus
    datos. Viendo esto ${esFichero ? "como fichero: se usa la ruta entre carpetas hermanas"
    : "servido por http: se usa la url con su puerto"}.</div>`;
  el.appendChild(menu);
  el.addEventListener("click", e => {
    if (e.target.closest("a")) return;      // dejar pasar el enlace
    e.stopPropagation();
    menu.classList.toggle("on");
  });
  document.addEventListener("click", () => menu.classList.remove("on"));
}

/* ---- graficas ------------------------------------------------------------------------------
   SVG a pelo: el panel no pide NADA a la red, tampoco una libreria de graficas. Dos formas, que
   son las que hacen falta aqui: `curva` para una serie sobre un eje continuo —donde lo que
   importa es la FORMA y, sobre todo, si cruza el cero— y `barrasDiv` para comparar magnitudes
   con signo. Lo demas ya lo dicen mejor las tablas, y una tabla no miente de lejos. */
function curva(series, ejeX, ejeY, opt){
  opt = opt || {};
  const W=900, H=340, mL=64, mR=16, mT=14, mB=44;
  const pts = series.flatMap(s => s.puntos).filter(p => p.x!=null && p.y!=null);
  if (!pts.length) return '<p class="nota">Sin puntos que dibujar.</p>';
  const logX = !!opt.logX;
  const X0 = Math.min(...pts.map(p => logX?Math.log10(p.x):p.x));
  const X1 = Math.max(...pts.map(p => logX?Math.log10(p.x):p.x));
  let Y0 = Math.min(...pts.map(p => p.y)), Y1 = Math.max(...pts.map(p => p.y));
  if (opt.cero) { Y0 = Math.min(Y0, 0); Y1 = Math.max(Y1, 0); }
  const py = (Y1-Y0)*.12 || 1; Y0 -= py; Y1 += py;
  const dx = (X1-X0)||1;
  const X = v => mL + ((logX?Math.log10(v):v) - X0)/dx * (W-mL-mR);
  const Y = v => mT + (1-(v-Y0)/(Y1-Y0)) * (H-mT-mB);
  let o = `<svg viewBox="0 0 ${W} ${H}" role="img" aria-label="${esc(ejeY)} frente a ${esc(ejeX)}">`;
  for (let i=0;i<=4;i++){
    const yy = mT+i*(H-mT-mB)/4, vv = Y1-(Y1-Y0)*i/4;
    o += `<line class="rejilla" x1="${mL}" y1="${yy}" x2="${W-mR}" y2="${yy}"/>`;
    o += `<text class="etq" x="${mL-8}" y="${yy+3}" text-anchor="end">${vv.toFixed(2)}</text>`;
  }
  const xs = series[0].puntos.map(p => p.x);
  /* CON MUCHOS PUNTOS, LAS ETIQUETAS SE COMEN EL EJE. Cuarenta y seis hashes de commit uno al
     lado del otro no son cuarenta y seis etiquetas: son una mancha negra que ademas tapa la
     grafica. Se dibuja una de cada N —siempre la primera y la ultima, que son las que sitúan— y
     la rejilla se adelgaza igual. Un eje ilegible es un eje que no esta. */
  const paso = xs.length > 12 ? Math.ceil(xs.length / 8) : 1;
  xs.forEach((v,i) => {
    const ultima = i === xs.length - 1;
    const pone = paso === 1 || i % paso === 0 || ultima;
    if (!pone) return;
    const etq = opt.etiquetasX ? opt.etiquetasX[i] : String(v);
    o += `<line class="rejilla" x1="${X(v)}" y1="${mT}" x2="${X(v)}" y2="${H-mB}"/>`;
    o += `<text class="etq" x="${X(v)}" y="${H-mB+15}" text-anchor="${
      i===0?"start":ultima?"end":"middle"}">${esc(String(etq))}</text>`;
  });
  /* La linea del cero solo se anuncia como frontera si ALGUNA serie la cruza de verdad.
     Ponerlo siempre seria una etiqueta que miente en cuanto los datos no cruzan. */
  if (opt.cero && Y0 < 0 && Y1 > 0) {
    const cruza = series.some(s => s.puntos.some(p => p.y > 0) && s.puntos.some(p => p.y < 0));
    o += `<line class="cero" x1="${mL}" y1="${Y(0)}" x2="${W-mR}" y2="${Y(0)}"/>` +
         `<text class="etq" x="${W-mR}" y="${Y(0)-6}" text-anchor="end">${
           cruza ? "0 — aquí cambia de signo" : "0 — la línea base"}</text>`;
  }
  o += `<line class="eje" x1="${mL}" y1="${H-mB}" x2="${W-mR}" y2="${H-mB}"/>`;
  o += `<line class="eje" x1="${mL}" y1="${mT}" x2="${mL}" y2="${H-mB}"/>`;
  o += `<text class="etq2" x="${(W+mL)/2}" y="${H-6}" text-anchor="middle">${esc(ejeX)}</text>`;
  o += `<text class="etq2" x="14" y="${H/2}" text-anchor="middle" transform="rotate(-90 14 ${H/2})">${esc(ejeY)}</text>`;
  // ORDEN FIJO, NUNCA CICLADO POR RANKING. La ranura la da la POSICION de la serie, asi que si
  // un filtro quita una, las que quedan conservan su color.
  const PAL = ["var(--s1)","var(--s2)","var(--s3)","var(--s4)","var(--b2)","var(--ojo)","var(--b1)"];
  // CODIFICACION SECUNDARIA: cada serie lleva ademas su PATRON DE TRAZO. No es adorno — en claro
  // el par teal/carmin se queda en ΔE 6,4 bajo deuteranopia, y la guia solo admite esa banda si
  // la identidad NO depende del color. Con el trazo, tambien funciona impreso en blanco y negro
  // y en modo de contraste forzado.
  const TRAZO = ["none", "7 3", "2 3", "9 3 2 3"];
  series.forEach((s,si) => {
    const col = s.color || PAL[si % PAL.length];
    const dash = TRAZO[si % TRAZO.length];
    let d = "", k = 0;
    s.puntos.forEach(p => { if(p.x!=null&&p.y!=null) d += (k++?"L":"M")+X(p.x).toFixed(1)+" "+Y(p.y).toFixed(1)+" "; });
    if (k>1) o += `<path d="${d}" fill="none" stroke="${col}" stroke-width="2.2"` +
                  (dash!=="none" ? ` stroke-dasharray="${dash}"` : "") + `/>`;
    s.puntos.forEach(p => {
      if (p.x==null||p.y==null) return;
      // 🚨 EL PUNTO LLEVA EL COLOR DE SU SERIE, NO EL DEL SIGNO.
      //
      // Antes hacia `opt.cero ? (p.y<0 ? bien : mal) : col`, o sea que en la linea de tiempo
      // -que es la que usa `cero`- TODOS los puntos de TODAS las series se pintaban verde o rojo
      // segun subieran o bajaran. El tono hacia dos trabajos a la vez: identidad en la linea y
      // polaridad en los puntos, y ganaba la polaridad. Por eso la grafica se veia como una masa
      // roja en la que no se distinguia una serie de otra.
      //
      // Y el signo YA se ve sin gastar color: la linea del cero esta dibujada y rotulada, y estar
      // por encima o por debajo lo dice. Gastar el tono en repetirlo costaba la identidad.
      o += `<circle cx="${X(p.x).toFixed(1)}" cy="${Y(p.y).toFixed(1)}" r="4" fill="${col}"` +
           ` stroke="var(--panel)" stroke-width="1.5">` +
           // El titulo dice el punto por su NOMBRE si lo trae (una fecha, un commit). Antes
           // decia siempre `p.x`, que es la posicion en el eje: «3 → 9.42» no situa nada.
           `<title>${esc(s.nombre+"  "+(p.etq!=null?p.etq:p.x)+"  →  "+p.y+
                        (opt.unidad?" "+opt.unidad:""))}</title></circle>`;
    });
  });
  let lg = "";
  if (opt.leyenda !== false && series.length > 1) {
    // La leyenda enseña el TRAZO ademas del color: si solo enseñara el cuadradito, la codificacion
    // secundaria no serviria de nada para leer la grafica.
    lg = '<div class="leyenda">' + series.map((s,si) => {
      const c = s.color||PAL[si%PAL.length], t = TRAZO[si%TRAZO.length];
      return `<span><svg width="22" height="10" style="vertical-align:-1px;margin-right:4px">` +
             `<line x1="0" y1="5" x2="22" y2="5" stroke="${c}" stroke-width="2.2"` +
             (t!=="none" ? ` stroke-dasharray="${t}"` : "") + `/></svg>${esc(s.nombre)}</span>`;
    }).join("") + "</div>";
  }
  return `<div class="gr">${lg}${o}</svg></div>`;
}

function barrasDiv(filas, opt){
  opt = opt || {};
  if (!filas.length) return '<p class="nota">Sin datos que dibujar.</p>';
  const W=900, fh=26, mL=Math.min(300, opt.mL||230), mR=70, H=filas.length*fh+30;
  const mx = Math.max(...filas.map(f => Math.abs(f.valor))) || 1;
  const hayNeg = filas.some(f => f.valor < 0);
  const ancho = W-mL-mR, cx = hayNeg ? mL+ancho/2 : mL;
  const esc0 = hayNeg ? (ancho/2)/mx : ancho/mx;
  let o = `<svg viewBox="0 0 ${W} ${H}" role="img">`;
  o += `<line class="eje" x1="${cx}" y1="6" x2="${cx}" y2="${H-24}"/>`;
  filas.forEach((f,i) => {
    const y = 8+i*fh, w = Math.abs(f.valor)*esc0;
    const x = f.valor<0 ? cx-w : cx;
    const col = f.color || (f.valor>0 ? "var(--bien)" : "var(--mal)");
    o += `<text class="etq" x="${mL-10}" y="${y+14}" text-anchor="end">${esc(f.etq)}</text>`;
    o += `<rect x="${x}" y="${y+3}" width="${Math.max(1,w)}" height="${fh-11}" fill="${col}" rx="2">` +
         `<title>${esc(f.etq+": "+f.valor+(opt.unidad?" "+opt.unidad:""))}</title></rect>`;
    o += `<text class="etq" x="${W-mR+8}" y="${y+14}">${esc(f.texto!=null?f.texto:f.valor)}</text>`;
  });
  return `<div class="gr">${o}</svg></div>`;
}

function pinta(){
/* ---- cabecera ---- */
$("#m-commit").textContent = D.commit + (D.sucio ? " · árbol sucio" : "");
$("#m-host").textContent = D.host;
/* Sin reloj dentro del html, la cabecera ensena el SELLO del contenido — dos paneles con el
   mismo sello son el mismo dato, y eso es mas util que una hora. La hora aparece solo cuando
   la trae datos.json, o sea en el modo en vivo, que es el unico caso en que significa algo. */
$("#m-gen").textContent = D.gen ? "generado " + D.gen : "sello " + (D.sello || "?");
/* HASTA DONDE LLEGAN LOS DATOS, que no es lo mismo que hace cuánto se refrescó la
   página. El 03-08 la cabecera ponía «en vivo · 2 s» mientras enseñaba tablas generadas
   trece horas y 94 medidas antes: ese contador mide el bucle de fetch, y el bucle estaba
   vivo — lo viejo era el DATO. Dos cosas distintas compartiendo sitio en la cabecera son
   una cosa que miente. Y sale también en file://, que es donde no hay bucle que mirar. */
$("#m-datos").textContent = D.libro_hasta
  ? "datos hasta " + D.libro_hasta + " · " + D.libro_filas + " medidas"
  : "";

seguro(function(){
  const k = [];
  /* LA CABECERA HABLA DE LA HERRAMIENTA DE ESTA PAGINA, no del proyecto. Ensenaba el ratio del
     corpus completo —que es del archivador— encima de la pagina de vdeflate y de la de vc-cm,
     que es la mezcla que este panel existe para no hacer. En el principal si toca el resumen del
     proyecto entero, porque el principal es del proyecto. */
  const F = D.ficha;
  if (F) {
    k.push({l:"medidas suyas", v:String(F.medidas),
            s:F.medidas ? F.declaradas+" declaradas al medir" : "nunca ha pasado por la báscula",
            c:F.medidas ? "" : "mal"});
    k.push({l:"camino", v:F.camino.hechas+"/"+F.camino.total,
            s:"preguntas hechas"+(F.camino.mas?" · "+F.camino.mas+" en sus matrices":""),
            c:F.camino.hechas ? "" : "mal"});
    k.push({l:"hallazgos", v:String(F.camino.hallazgos), s:"abiertos en su camino",
            c:F.camino.hallazgos ? "mal" : "mut"});
    k.push({l:"código", v:fm((F.loc||[]).reduce((a,b)=>a+b[1],0)),
            s:"líneas · "+F.codigo});
    k.push({l:"sustituye a", v:F.sustituye_a.startsWith("—") ? "—" : "de fuera",
            s:F.sustituye_a, c:"mut"});
  } else {
  const tot = D.multi.find(r => r[0]==="TOTAL");
  if (tot) k.push({l:"corpus completo", v:(+tot[1]/+tot[2]).toFixed(1)+"×",
                   s:fm(+tot[1])+" B → "+fm(+tot[2])+" B"});
  if (tot) k.push({l:"vs zpaq -m5", v:pc(+tot[4],2), s:"en 673 MB reales", c:+tot[4]<0?"bien":"mal"});
  const m = D.mapa[0];
  if (m) k.push({l:"bits por byte", v:m.bpb.toFixed(3), s:"de 8 posibles · "+m.nombre});
  k.push({l:"modelos", v:"43", s:"34 tablas + 9 derivados"});
  k.push({l:"código", v:fm(D.loc.reduce((a,b)=>a+b[1],0)), s:"líneas · "+((D.deps||{}).externos||[]).length+" dependencias"});
  }
  /* La carga solo existe en el modo en vivo: es del servidor AHORA, no del banco. En el
     fichero suelto se dice que no la hay en vez de ensenar una vieja como si fuera de ahora. */
  k.push(D.carga
    ? {id:"kpi-carga", l:"carga de la máquina", v:D.carga,
       s:(D.vuelo||[]).length+" procesos en vuelo", c:+D.carga>4?"ojo":"bien"}
    : {id:"kpi-carga", l:"carga de la máquina", v:"—",
       s:"sólo en vivo (sirve el directorio)", c:"mut"});
  $("#kpis").innerHTML = k.map(x=>`<div class="kpi"${x.id?` id="${x.id}"`:""}><div class="l">${x.l}</div>
    <div class="v${x.c?" "+x.c:""}">${x.v}</div><div class="s">${x.s}</div></div>`).join("");
});

/* ---- mapa de bits ---- */
function pintaMapa(){
  /* SI ESTA PAGINA NO TIENE MAPA, no hay nada que pintar. Desde que `$` devuelve un elemento
     suelto en vez de null —para que los renderizadores sin seccion escriban en el vacio— aqui
     llegaba un <div>, y un <div> no tiene getContext: reventaba y, al no estar blindada, se
     llevaba la pagina entera.
     SE PREGUNTA POR EL CONTENEDOR, no por el canvas: `#tira` lo crea esta misma funcion al
     escribir en `#mapa-cpo`, asi que en la primera pasada no existe todavia. Preguntar por el
     hijo que uno mismo va a fabricar no es una guarda: es plantarse siempre. */
  if ($("#mapa-cpo") === _VACIO) return;
  const m = D.mapa[sel];
  if(!m){ $("#mapa-cpo").innerHTML = '<p class="nota">Sin volcado. Genera uno con <code>VC_COSTEPOS=/tmp/x.f32 vcompresor comprimir …</code> y pásalo por <code>bench/extrae_mapa.py</code>.</p>'; return; }
  $("#mapa-cpo").innerHTML = `
    <p class="nota">Se comprimieron <b>${fm(m.bytes)} bytes</b> de <b>${m.nombre}</b> y salieron
      <b>${fm(m.salida)}</b> — ${m.ratio}× a ${m.bpb.toFixed(3)} bits por byte. Cada celda de abajo
      es un tramo de ${Math.round(m.bytes/m.tira.length)} bytes, en orden de lectura.</p>
    <canvas id="tira" width="1000" height="132"></canvas>
    <div class="leyenda"><span>0 bits · predicho, gratis</span><span class="rampa"></span>
      <span>4+ bits · en crudo</span></div>
    <div class="leyenda"><span style="color:var(--tx3)">escala no lineal (γ 0,55, tope 4 bits):
      por encima de 4 bits casi no hay nada y con escala plana el mapa saldría todo negro</span></div>
    <div class="dos" style="margin-top:18px">
      <div>
        <div class="frag-t"><span>Lo que sale casi gratis</span><span>${m.baratos[0].bpb.toFixed(3)} bits/byte</span></div>
        <pre class="frag frio">${esc(m.baratos[0].txt)}</pre>
        <div class="frag-t"><span>Lo que se lleva el fichero</span><span>${m.caros[0].bpb.toFixed(3)} bits/byte</span></div>
        <pre class="frag">${esc(m.caros[0].txt)}</pre>
        <p class="nota" style="margin-top:11px">Esa es la respuesta a si una letra ocupa su byte
          entero: <b>no lo ocupa</b>. Lo predecible baja de 0,01 bits —una milésima de byte— y lo
          impredecible se acerca a 8. La media de este fichero es ${m.bpb.toFixed(3)}.</p>
      </div>
      <div>
        <div class="frag-t"><span>Reparto: cuántos bytes salen a cada precio</span></div>
        <canvas id="hist" width="620" height="104"></canvas>
        <div class="frag-t" style="margin-top:14px"><span>Los caracteres que más pesan</span></div>
        <div class="env-t"><table><thead><tr><th>car</th><th class="n">veces</th>
          <th class="n">bits/uno</th><th class="n">% del total</th><th></th></tr></thead><tbody>
          ${m.chars.slice(0,10).map(c=>`<tr><td class="n">${esc(c.ch)}</td><td class="n">${fm(c.n)}</td>
            <td class="n">${c.bpb.toFixed(2)}</td><td class="n">${c.pct.toFixed(1)}</td>
            <td><div class="barra"><i style="width:${Math.min(100,c.pct*5)}%;background:${color(c.bpb)}"></i></div></td></tr>`).join("")}
        </tbody></table></div>
      </div>
    </div>`;
  const cv = $("#tira"), ct = cv.getContext("2d");
  const N = m.tira.length, COL = 100, FIL = Math.ceil(N/COL), cw = 1000/COL, ch = 132/FIL;
  m.tira.forEach((v,i)=>{ ct.fillStyle = color(v);
    ct.fillRect(Math.floor((i%COL)*cw), Math.floor(Math.floor(i/COL)*ch), Math.ceil(cw), Math.ceil(ch)); });

  const hv = $("#hist"), h = hv.getContext("2d");
  const mx = Math.max(...m.reparto);
  h.clearRect(0,0,620,104);
  m.reparto.forEach((v,i)=>{ const bw = 620/m.reparto.length;
    const alto = mx ? Math.round(v/mx*94) : 0;
    h.fillStyle = color(i/4);
    h.fillRect(i*bw, 104-alto, Math.max(1,bw-1.5), alto); });
  h.fillStyle = getComputedStyle(document.body).getPropertyValue("--tx3");
  h.font = "10px " + getComputedStyle(document.body).fontFamily;
  [0,2,4,6,8].forEach(b=>h.fillText(b+"b", b*4*(620/33)+2, 100));
}
/* BLINDADA COMO TODAS LAS DEMAS. Esta llamada llevaba semanas FUERA de `seguro()`, y con una
   sola que se quede fuera la promesa de «una seccion rota no tumba la pagina» es mentira. Hoy lo
   fue: `pintaMapa` pedia un canvas que en los sub-paneles no existe, reventaba, y se llevaba la
   pagina entera por delante. */
seguro(function(){
  $("#tabs").innerHTML = D.mapa.map((m,i)=>
    `<button role="tab" aria-selected="${i===sel}" data-i="${i}">${m.nombre}</button>`).join("");
  pintaMapa();
});

/* ---- rivales ---- */
seguro(function(){
  const ent = Object.keys(D.rivales);
  const filas = [];
  for (const e of ent){
    const nos = D.nuestro[e];
    const rs = D.rivales[e].slice().sort((a,b)=>a.bytes-b.bytes);
    const mejor = rs[0];
    filas.push({e, nos, rs, mejor});
  }
  $("#rivales").innerHTML = `
    <p class="nota">La columna que decide es <b>vs zpaq</b>: es el rival de nuestra liga —
      mismo orden de velocidad. paq8px comprime más pero va entre <b>20 y 60 veces más lento</b>,
      así que es la referencia del techo, no un competidor.</p>
    <div class="env-t"><table><thead><tr><th>entrada</th><th class="n">nosotros</th>
      ${["zpaq -m5","paq8px -8","xz -9e","zstd -19","brotli -11"].map(n=>`<th class="n">${n}</th>`).join("")}
      <th class="n">vs zpaq</th></tr></thead><tbody>
      ${filas.map(f=>{
        const g = n => (f.rs.find(r=>r.nom===n)||{}).bytes;
        const z = g("zpaq -m5");
        const d = (f.nos && z) ? (f.nos-z)*100/z : null;
        return `<tr><td>${f.e}</td><td class="n">${f.nos?fm(f.nos):'<span class="mut">—</span>'}</td>
          ${["zpaq -m5","paq8px -8","xz -9e","zstd -19","brotli -11"].map(n=>{
            const v=g(n); return `<td class="n${v&&f.nos&&v<f.nos?" mut":""}">${v?fm(v):'<span class="mut">—</span>'}</td>`;}).join("")}
          <td class="n ${d==null?"mut":d<0?"bien":"mal"}">${d==null?"—":pc(d)}</td></tr>`;
      }).join("")}
    </tbody></table></div>`;
});

/* ---- el registro de metodos ---- */
seguro(function(){
  const M = D.metodos || [], C = D.metodos_cruce || {};
  const cab = M[0] || [], F = M.slice(1), j = n => cab.indexOf(n);
  const I = {t:j("tabla"), n:j("nombre"), o:j("origen"), e:j("estado"), q:j("que_mide"),
             u:j("unidad_principal"), col:j("columna_principal"), c:j("como_se_calcula"),
             h:j("hueco_que_tapa"), ctl:j("control"), fal:j("que_lo_falsaria"), fu:j("fuente")};
  const inc = {};
  (C.incompletos||[]).forEach(x => inc[x.tabla] = x.faltan);

  let h = `<div class="marc">
    <div class="tar"><div class="k">${F.length}</div><div class="e">métodos declarados</div></div>
    <div class="tar"><div class="k ${(C.sin_declarar||[]).length?"mal":"bien"}">${(C.sin_declarar||[]).length}</div>
      <div class="e">midiendo sin declarar</div></div>
    <div class="tar"><div class="k ${(C.incompletos||[]).length?"ojo":"bien"}">${(C.incompletos||[]).length}</div>
      <div class="e">declaración a medias</div></div>
    <div class="tar"><div class="k">${(C.sin_medir||[]).length}</div>
      <div class="e">declarados y sin medir</div></div>
    <div class="tar"><div class="k mut">${(C.registros||[]).length}</div>
      <div class="e">registros (no miden)</div></div>
  </div>`;

  h += `<p class="nota">Este banco mide cosas para las que no hay protocolo publicado. Por eso
    un método se declara aquí con lo que lo hace defendible —<b>el control que lo sostiene y qué
    lo falsaría</b>— y el panel canta cuando falta. Inventar método es legítimo; no escribirlo,
    no.</p>`;

  if ((C.sin_declarar||[]).length) {
    h += `<div class="frag-t" style="margin-top:16px"><span class="mal">midiendo sin declarar</span>
      <span>${C.sin_declarar.length} tablas</span></div>
      <p class="nota">Están medidas y salen en el panel, pero <b>nadie ha escrito qué significan</b>.
        Dentro de tres meses ninguna de estas cifras se podrá defender. No es urgente: es lo que
        hay, y esta lista es la cola de trabajo.</p>
      <p class="nota" style="font-family:var(--mono);font-size:11px;line-height:1.9">${
        C.sin_declarar.map(t => `<span class="chip">${esc(t)}</span>`).join(" ")}</p>`;
  }

  h += `<div class="frag-t" style="margin-top:18px"><span>declarados</span><span>${F.length}</span></div>`;
  F.forEach(f => {
    const falta = inc[f[I.t]] || [];
    const propio = (f[I.o]||"").indexOf("propio") === 0;
    h += `<div class="met">
      <div><b>${esc(f[I.n]||f[I.t])}</b>
        <span class="chip ${propio?"":"mut"}">${esc(f[I.o]||"origen sin declarar")}</span>
        <span class="chip mut">${esc(f[I.e]||"")}</span>
        <span class="mut" style="font-family:var(--mono);font-size:11px">${esc(f[I.t])}${
          f[I.col] ? " · " + esc(f[I.col]) : ""}${f[I.u] ? " (" + esc(f[I.u]) + ")" : ""}</span></div>`;
    if (falta.length)
      h += `<div class="nota ojo" style="margin-top:5px">declaración a medias: falta
        <b>${esc(falta.join(", "))}</b></div>`;
    h += `<div class="kv">
      <div class="k">qué mide</div><div>${esc(f[I.q]||"—")}</div>
      <div class="k">cómo se calcula</div><div>${esc(f[I.c]||"—")}</div>
      <div class="k">hueco que tapa</div><div>${esc(f[I.h]||"—")}</div>
      <div class="k">control</div><div>${esc(f[I.ctl]||"—")}</div>
      <div class="k">qué lo falsaría</div><div>${esc(f[I.fal]||"—")}</div>
      <div class="k">fuente</div><div class="mut" style="font-family:var(--mono)">${esc(f[I.fu]||"—")}</div>
    </div></div>`;
  });

  if ((C.sin_medir||[]).length)
    h += `<div class="frag-t" style="margin-top:18px"><span>declarados y nunca medidos</span>
      <span>${C.sin_medir.length}</span></div>
      <p class="nota">Hipótesis paradas: el método está escrito y no ha producido ni una fila.
        No es un fallo — dice por dónde seguir. ${C.sin_medir.map(t=>`<span class="chip">${esc(t)}</span>`).join(" ")}</p>`;

  $("#metodos").innerHTML = h;
});

/* ---- libro mayor ---- */
seguro(function(){
  const a = D.ablacion, cab = a[0], filas = a.slice(1);
  const mx = Math.max(...filas.map(f=>Math.abs(+f[1])));
  $("#libro").innerHTML = `
    <p class="nota">Se apaga un grupo entero y se mide. <b>ratio</b> = cuánto EMPEORA el fichero
      al quitarlo (más alto = más falta hace). <b>tiempo</b> = cuánto BAJA. Un grupo que empeora
      poco y ahorra mucho tiempo es candidato a desaparecer.</p>
    ${barrasDiv(filas.map(f => ({etq: f[0], valor: +(+f[1]).toFixed(3),
                                 texto: pc(+f[1]) + "  ·  " + f[2] + "% tiempo"})),
                {unidad: "% de archivo"})}
    <p class="nota">Lo que aporta cada grupo, de un vistazo. A la derecha de cada barra va
      también lo que <b>cuesta en tiempo</b>, porque un grupo sólo se puede juzgar con las dos
      cifras juntas: la barra sola diría que todo lo que aporta se queda.</p>
    <div class="env-t"><table><thead><tr><th>grupo</th><th class="n">aporta</th>
      <th></th><th class="n">cuesta</th><th>veredicto</th></tr></thead><tbody>
      ${filas.map(f=>{
        const r=+f[1], t=+f[2];
        const v = r<0 ? ["ESTORBA","mal"] : r>3 ? ["imprescindible","bien"]
                : (r < -t/4) ? ["mal negocio","ojo"] : ["se queda",""];
        return `<tr><td>${f[0]}</td><td class="n ${r>0?"bien":"mal"}">${pc(r)}</td>
          <td><div class="barra"><i style="width:${Math.abs(r)/mx*100}%;background:${r>0?"var(--bien)":"var(--mal)"}"></i></div></td>
          <td class="n mut">${t}%</td>
          <td><span class="chip ${v[1]}">${v[0]}</span></td></tr>`;
      }).join("")}
    </tbody></table></div>
    ${D.poda.length ? `<p class="nota" style="margin-top:15px">Y la <b>poda validada en las cinco
      muestras</b>, no en las dos con las que se decidió — que es donde se cayeron dos hipótesis del día:</p>
    <div class="env-t"><table><thead><tr>${D.poda[0].map((c,i)=>`<th class="${i?"n":""}">${c}</th>`).join("")}</tr></thead>
      <tbody>${D.poda.slice(1).map(f=>`<tr>${f.map((c,i)=>{
        const num = i && !isNaN(+c);
        const pct = c.includes("%");
        return `<td class="${i?"n":""} ${pct?(c.startsWith("-")?"bien":"mal"):""}">${num?fm(+c):c}</td>`;
      }).join("")}</tr>`).join("")}</tbody></table></div>` : ""}
    ${(() => {
      const t = D.talla.slice(1).map(f => ({t: f[0], v1: +f[1], v2: +f[2], d: +f[3]}));
      if (!t.length) return "";
      /* La curva que casi no se mide. La poda estaba validada en 10 MB y con eso bastaba para
         darla por buena; barriendo el tamano cambia de SIGNO. Se deja a la vista porque el
         numero de una sola talla es lo que estuvo a punto de meter un +0,5% en todo lo
         pequeno. */
      const mx = Math.max(...t.map(x => Math.abs(x.d)));
      return `<p class="nota" style="margin-top:18px"><b>Y por qué la poda no es incondicional.</b>
        Validada en 10 MB daba −0,146% e iba a entrar tal cual. Barriendo el <b>tamaño del
        bloque</b> cambia de signo: los podados son dispersos y de texto, y sus tablas crecen
        con el bloque — en uno pequeño caben enteras y aciertan, en uno grande se desbordan y
        estorban. El cruce está sobre los 6 MiB y el umbral se puso en <b>8 MiB</b>, para podar
        sólo donde el dato es claro. Con <code>--max</code> los tramos son de 32 MiB, así que el
        archivado real siempre poda.</p>
      ${curva([{nombre: "efecto de podar", puntos: t.map((x,i) => ({x: i, y: +x.d.toFixed(3)}))}],
              "tamaño de bloque", "efecto de podar (%)",
              {cero: true, unidad: "%", etiquetasX: t.map(x => x.t)})}
      <p class="nota">Esta curva es el argumento entero: un solo punto —el de 10 MB— decía
        <b>podar siempre</b>. La línea de puntos es el cero, y ahí es donde la decisión se da la
        vuelta. Verde por debajo (podar ayuda), rojo por encima (estorba).</p>
      <div class="env-t"><table><thead><tr><th>bloque</th><th class="n">sin poda</th>
        <th class="n">con poda</th><th class="n">efecto</th><th></th><th>veredicto</th></tr></thead><tbody>
        ${t.map(x => `<tr><td class="n">${esc(x.t)}</td><td class="n">${fm(x.v1)}</td>
          <td class="n">${fm(x.v2)}</td>
          <td class="n ${x.d < 0 ? "bien" : "mal"}">${pc(x.d, 3)}</td>
          <td><div class="barra"><i style="width:${Math.abs(x.d)/mx*100}%;background:${
             x.d < 0 ? "var(--bien)" : "var(--mal)"}"></i></div></td>
          <td><span class="chip ${x.d<0?"bien":"mal"}">${x.d<0?"poda":"sin podar"}</span></td></tr>`).join("")}
      </tbody></table></div>`;
    })()}
    ${(() => {
      const c = D.coste.slice(1).map(f => ({nom: f[0], cic: +f[1], fal: +f[2]}));
      if (!c.length) return "";
      /* El suelo de ruido no se estima a ojo: se LEE del propio experimento. Quitar un modelo
         no puede hacer el compresor mas rapido, asi que todo lo que salga en positivo es ruido
         de medida, y el mayor positivo marca el suelo. Sin esa banda dibujada, un -0,4% parece
         un hallazgo y no lo es. */
      const suelo = Math.max(0, ...c.map(x => x.cic));
      c.sort((a,b) => a.cic - b.cic);
      const mx = Math.max(...c.map(x => Math.abs(x.cic)));
      return `<p class="nota" style="margin-top:18px"><b>Y lo que cuesta cada modelo en ciclos</b>,
        dejándolo fuera y volviendo a medir. Lo importante no es el orden: es que la lista sale
        <b>plana</b>. Ninguno pasa de ${mx.toFixed(2)}% y ${c.filter(x=>x.cic>0).length} salen en
        positivo — y eso es imposible, porque no se va más rápido haciendo más trabajo. Así que
        <b>el suelo de ruido es ±${suelo.toFixed(2)}%</b> y no hay ningún modelo caliente: no se
        puede acelerar esto quitando uno o dos. La poda funciona porque son diez.</p>
      <div class="env-t"><table><thead><tr><th>modelo</th><th class="n">ciclos</th><th></th>
        <th class="n">fallos de caché</th><th>v2</th></tr></thead><tbody>
        ${c.map(x => `<tr><td class="n">${esc(x.nom)}</td>
          <td class="n ${Math.abs(x.cic)<=suelo?"mut":x.cic<0?"bien":"mal"}">${pc(x.cic)}</td>
          <td><div class="barra"><i style="width:${Math.abs(x.cic)/mx*100}%;background:${
             Math.abs(x.cic)<=suelo?"var(--tx3)":x.cic<0?"var(--bien)":"var(--mal)"}"></i></div></td>
          <td class="n ${Math.abs(x.fal)<=suelo*4?"mut":x.fal<0?"bien":"mal"}">${pc(x.fal)}</td>
          <td>${D.podados.includes(x.nom)?'<span class="chip mal">podado</span>':""}</td></tr>`).join("")}
      </tbody></table></div>`;
    })()}`;
});

/* ---- tiempo ---- */
seguro(function(){
  const p = D.perfil.slice(1), c = D.censo.slice(1);
  const mxp = p.length ? Math.max(...p.map(f=>+f[1])) : 1;
  $("#tiempo").innerHTML = `
    <div class="dos">
      <div>
        <div class="frag-t"><span>Perfil por función</span><span>${p.length?"perf record, muestra real":"pendiente"}</span></div>
        ${p.length ? `<div class="env-t"><table><thead><tr><th>función</th><th class="n">% del tiempo</th><th></th></tr></thead>
        <tbody>${p.map(f=>`<tr><td class="n">${f[0]}</td><td class="n">${(+f[1]).toFixed(1)}</td>
          <td><div class="barra"><i style="width:${+f[1]/mxp*100}%"></i></div></td></tr>`).join("")}
        </tbody></table></div>` : '<p class="nota">Sin perfil guardado.</p>'}
        <p class="nota" style="margin-top:12px">El mezclador —lo que más se ha tocado— es una
          fracción pequeña. El grueso es <b>buscar en tablas que no caben en caché</b>. Optimizar
          aritmética ahí no sirve de nada; lo que sirve es que las tablas quepan.</p>
      </div>
      <div>
        <div class="frag-t"><span>Censo de tablas</span><span>${c.length?"ocupación real":"pendiente"}</span></div>
        ${c.length ? `<div class="env-t"><table><thead><tr><th>modelo</th><th class="n">MiB</th>
          <th class="n">ranuras</th><th class="n">ocupadas</th><th class="n">%</th></tr></thead>
          <tbody>${c.map(f=>{const o=+f[4];
            return `<tr><td class="n">${f[0]}</td><td class="n">${f[1]}</td><td class="n">${fm(+f[2])}</td>
            <td class="n">${fm(+f[3])}</td><td class="n ${o<5?"mal":o<25?"ojo":"bien"}">${o.toFixed(1)}</td></tr>`;
          }).join("")}</tbody></table></div>` : '<p class="nota">Sin censo guardado. <code>VC_CENSO=1</code> lo imprime.</p>'}
      </div>
    </div>`;
});

/* ---- ejecuciones ---- */
seguro(function(){
  const e = D.ejec.slice().reverse();
  const viv = (D.vuelo || []).length ? `<div style="margin-bottom:13px">${D.vuelo.map(v=>
    `<div class="viv"><span class="pt"></span><span>${v.t}</span>
     <span class="mut">${v.cpu}% cpu · ${fm(v.rss)} MB</span>
     <span class="mut" style="overflow:hidden;text-overflow:ellipsis">${esc(v.cmd)}</span></div>`).join("")}</div>` : "";
  $("#ejec").innerHTML = viv + (e.length ? `
    <div class="env-t"><table><thead><tr><th>cuándo</th><th>qué</th><th>config</th>
      <th class="n">salida</th><th class="n">ratio</th><th class="n">seg</th>
      <th class="n">cpu</th><th class="n">RAM</th><th class="n">Gciclos</th><th class="n">IPC</th>
      <th class="n">carga</th></tr></thead><tbody>
      ${e.map(r=>`<tr>
        <td class="n mut">${r.t.slice(5,16).replace("T"," ")}</td>
        <td class="t">${esc(r.etiqueta||r.cmd.slice(0,40))}${r.sucio?' <span class="chip ojo">sucio</span>':""}</td>
        <td class="n mut">${esc(r.config||"—")}</td>
        <td class="n">${r.bytes_out?fm(r.bytes_out):"—"}</td>
        <td class="n">${r.ratio?r.ratio.toFixed(2)+"×":"—"}</td>
        <td class="n">${r.seg?r.seg.toFixed(1):"—"}</td>
        <td class="n mut">${r.cpu_pct?Math.round(r.cpu_pct)+"%":"—"}</td>
        <td class="n">${r.rss_kb?fm(Math.round(r.rss_kb/1024)):"—"}</td>
        <td class="n">${r.ciclos?(r.ciclos/1e9).toFixed(1):"—"}</td>
        <td class="n">${r.ipc?r.ipc.toFixed(2):"—"}</td>
        <td class="n ${r.carga>4?"ojo":"mut"}">${r.carga?r.carga.toFixed(1):"—"}</td></tr>`).join("")}
    </tbody></table></div>` : `
    <p class="nota">Todavía no hay ninguna ejecución en el registro. Se llena solo: cualquier
      medida que pase por la báscula deja su línea.</p>
    <pre class="uso">bench/mide.sh <b>-e</b> "F2.48 decaimiento w&gt;&gt;16" <b>-m</b> sql.10m <b>-c</b> "VC_DECAY=16" <b>--</b> \\
    ./target/release/vcompresor comprimir /tmp/s/sql.10m -o /tmp/x.vc --max</pre>
    <p class="nota" style="margin-top:12px">Anota el commit, si el árbol estaba sucio, la carga
      de la máquina en ese momento, los bytes, los segundos, la CPU, la memoria pico y los
      ciclos. <b>Los ciclos mandan sobre el reloj</b>: la misma prueba tarda 51,6 s o 97,1 s
      según la carga, y esa diferencia no es del compresor.</p>`);
});

/* ---- la aguja ---- */
seguro(function(){
  const A = D.aguja;
  if (!A.length) { $("#aguja").innerHTML = `
    <p class="nota">Todavía no hay ninguna instantánea. Se toma con
      <code>bench/aguja.sh "de qué cambio"</code> — unos seis minutos, la misma batería
      siempre.</p>
    <p class="nota">Las medidas de <b>ratio</b> corren en paralelo porque son deterministas;
      sólo las de tiempo y memoria van en serie. Una batería de veinte minutos no se ejecuta
      nunca, y una que no se ejecuta no mide nada.</p>`; return; }

  const MET = [
    {k:"total5",    n:"ratio · 5 muestras", tipo:"bytes", clave:true},
    {k:"sql",       n:"sql",                tipo:"bytes"},
    {k:"logs",      n:"logs",               tipo:"bytes"},
    {k:"csvjson",   n:"csvjson",            tipo:"bytes"},
    {k:"codigo",    n:"código",             tipo:"bytes"},
    {k:"enwik8",    n:"enwik8",             tipo:"bytes"},
    {k:"ofimatica", n:"ofimática (20 docx)",tipo:"bytes", clave:true},
    {k:"comp_seg",  n:"comprimir · seg",    tipo:"coste"},
    {k:"comp_mb",   n:"comprimir · MB",     tipo:"coste"},
    {k:"desc_seg",  n:"descomprimir · seg", tipo:"coste"},
    {k:"desc_mb",   n:"descomprimir · MB",  tipo:"coste"},
  ];
  const ult = A[A.length-1];
  const roto = !(ult.ida_vuelta && ult.determinista && ult.fuzz);

  /* LA AGUJA, DIBUJADA. La tabla dice cada numero; la grafica dice la FORMA, que es lo que
     esta pestana promete: no importa una metrica, importa si al mover una se movieron las
     otras. Por eso todo va normalizado a la PRIMERA instantanea (% de cambio contra la linea
     base) — con los bytes en crudo, enwik8 (2,1 MB) aplastaria a logs (51 KB) y no se veria
     nada. Y por eso ratio y coste van en dos graficas: mezclarlos deja las lineas de ratio
     planas contra un coste que se mueve diez veces mas. */
  const linea = (tipo) => {
    const ms = MET.filter(m => m.tipo === tipo && A.some(a => a[m.k] != null));
    const base = A[0];
    const ss = ms.map(m => ({
      nombre: m.n,
      puntos: A.map((a,i) => ({
        x: i,
        y: (base[m.k] && a[m.k] != null) ? +(((a[m.k]-base[m.k])*100)/base[m.k]).toFixed(3) : null
      })).filter(p => p.y != null)
    })).filter(s => s.puntos.length > 1);
    if (!ss.length) return "";
    return curva(ss, "instantáneas de la aguja (en orden)", "% contra la línea base",
                 {cero: true, unidad: "%", etiquetasX: A.map(a => a.commit + (a.sucio?"*":""))});
  };

  $("#aguja").innerHTML = `
    ${roto ? `<p class="nota" style="border-left:3px solid var(--mal);padding-left:11px">
      <b>Hay una garantía rota en la última instantánea.</b> Ninguna cifra de ratio cuenta
      hasta que vuelva a estar en verde: un archivador que comprime muy bien y no devuelve lo
      que le metiste no vale nada.</p>` : ""}
    ${A.length > 1 ? `
      <p class="nota">Todo contra la <b>primera instantánea</b>, en porcentaje: es la única
        forma de ver las siete muestras en el mismo eje —en bytes crudos, enwik8 aplastaría a
        logs— y de responder a lo que esta pestaña pregunta de verdad, que no es cuánto bajó
        una, sino <b>si al mover una se movieron las otras</b>. Bajo el cero, mejor que la base;
        por encima, peor.</p>
      ${linea("bytes")}
      <p class="nota" style="margin-top:14px">Y el coste aparte, porque se mueve mucho más que
        el ratio y en la misma gráfica dejaría las líneas de arriba planas. Aquí subir no es
        automáticamente malo: un coste que sube a cambio de ratio puede ser un trato aceptable.</p>
      ${linea("coste")}` : ""}
    <div class="env-t"><table><thead><tr><th>métrica</th>
      ${A.map(a=>`<th class="n" title="${esc(a.etiqueta||"")}">${esc(a.commit)}${a.sucio?"*":""}</th>`).join("")}
      <th class="n">último salto</th></tr></thead><tbody>
      ${MET.map(m=>{
        const v = A.map(a=>a[m.k]);
        const a1 = v[v.length-1], a0 = v.length>1 ? v[v.length-2] : null;
        const d = (a0 && a1!=null) ? (a1-a0)*100/a0 : null;
        // En bytes, menos es mejor. En coste tambien, pero un coste que sube a cambio de
        // ratio puede ser un trato aceptable: se pinta distinto, no se juzga igual.
        const cl = d==null||Math.abs(d)<0.001 ? "mut" : (d<0 ? "bien" : (m.tipo==="bytes"?"mal":"ojo"));
        return `<tr><td${m.clave?' style="font-weight:600"':''}>${m.n}</td>
          ${v.map((x,i)=>`<td class="n ${i===v.length-1?"":"mut"}">${x==null?"—":fm(x)}</td>`).join("")}
          <td class="n ${cl}">${d==null?"—":pc(d,3)}</td></tr>`;
      }).join("")}
      <tr><td>garantías</td>
        ${A.map(a=>{
          const ok = a.ida_vuelta && a.determinista && a.fuzz;
          return `<td class="n">${ok?'<span class="chip bien">OK</span>':'<span class="chip mal">MAL</span>'}</td>`;
        }).join("")}<td></td></tr>
    </tbody></table></div>
    <p class="nota" style="margin-top:13px">Cada columna es un commit; la última es la de
      ahora. <b>Las tres garantías van en la misma tabla que el ratio a propósito</b>: son la
      condición para que las cifras de arriba signifiquen algo, no una comprobación aparte que
      se mira cuando uno se acuerda.</p>`;
});

/* ---- carril ZIP ---- */
seguro(function(){
  const t = n => (D.tablas[n]||[]);
  const tab = (filas, res) => !filas.length ? "" : `<div class="env-t"><table><thead><tr>${
    filas[0].map((c,i)=>`<th class="${i?"n":""}">${esc(c)}</th>`).join("")}</tr></thead><tbody>${
    filas.slice(1).map(f=>`<tr>${f.map((c,i)=>{
      const num = i && !isNaN(+c) && c!=="";
      const cl = res && res(c,i) || "";
      return `<td class="${i?"n":""} ${cl}">${num?fm(+c):esc(c)}</td>`;
    }).join("")}</tr>`).join("")}</tbody></table></div>`;
  const veredicto = (c,i) => c==="ES EL PREMIO" ? "bien" : c==="no" ? "mut" : "";
  $("#zip").innerHTML = `
    <p class="nota">Un documento de ofimática es <b>29% imágenes</b>, y el ZIP las guarda
      <b>sin comprimir</b> —no gana nada re-comprimiendo un PNG—. El compresor sólo miraba los
      miembros deflatados, así que 23 MB de 81 pasaban de largo como bytes literales.</p>
    ${tab(t("zip-progreso"))}
    <p class="nota" style="margin-top:16px"><b>Y las cinco hipótesis que no eran.</b> La del
      dedup era la más convincente: 630 de 1.074 miembros son duplicados byte a byte, el 9,8%
      de los bytes. Vale 0,18%. Un hecho llamativo y verdadero no es una explicación.</p>
    ${tab(t("zip-hipotesis"), veredicto)}
    <div class="dos" style="margin-top:16px">
      <div><div class="frag-t"><span>De qué está hecho un .docx</span></div>${tab(t("zip-composicion"))}</div>
      <div><div class="frag-t"><span>Y cómo lo guarda el ZIP</span></div>${tab(t("zip-metodos"))}</div>
    </div>`;
});

/* ---- EL MENU DE ARRIBA ---------------------------------------------------------------
   Veintiuna secciones seguidas no son un panel: son un scroll de dos metros donde lo de
   abajo es tan invisible como si no estuviera. Con pestañas cada grupo cabe en una pantalla.
   La pestaña elegida va en el hash de la URL para que se pueda enlazar y para que al
   recargar no se pierda — el panel se refresca solo y perder la pestaña en cada refresco
   seria insufrible. */
seguro(function(){
  /* P = [[indice_global, NOMBRE], ...] de la vista activa. El indice global es el que llevan
     las secciones en data-t; el numero que se ensena se cuenta por POSICION. Son dos numeros
     distintos y mezclarlos dejo el panel en blanco una vez, asi que van con nombre distinto. */
  const P = D.pestanas || [];
  const nav = $("#nav");
  if (!nav || !P.length) return;
  nav.innerHTML = P.map((t,i) =>
    `<button role="tab" aria-selected="${i===0}" data-i="${i}">${i+1} · ${esc(t[1])}</button>`).join("");
  /* La url lleva un NOMBRE, no un numero. `#p1` era CAMINO... hasta que CAMINO se metio
     delante de PIEZAS y paso a ser otra cosa: un enlace guardado ayer abre hoy una pestaña
     distinta y no avisa. Con nombre, o abre la que pediste o no abre ninguna. Los `#pN`
     viejos se siguen aceptando al entrar, pero ya no se escriben. */
  const SLUG = P.map(t => t[1].trim().toLowerCase()
    .normalize("NFD").replace(/[̀-ͯ]/g, "").replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, ""));
  function ve(i){
    [...nav.children].forEach((b,k)=>b.setAttribute("aria-selected", k===i));
    const g = P[i] ? P[i][0] : -1;                 // el indice GLOBAL de esa pestaña
    // Las secciones de OTRA herramienta se apagan siempre: estan en el mismo html, y sin esto
    // se quedarian encendidas de la vista anterior — que es justo la mezcla que esto evita.
    [...document.querySelectorAll("section[data-t]")]
      .forEach(s => s.classList.toggle("on", visible(s) && +s.dataset.t === g));
    history.replaceState(null, "", "#" + (ACTIVA === _VISTA_POR_DEFECTO ? "" : ACTIVA + "/")
                                       + (SLUG[i] || "p" + i));
  }
  nav.addEventListener("click", e => {
    const b = e.target.closest("button[data-i]");
    if (b) ve(+b.dataset.i);
  });
  // Un ancla dentro de una pestaña oculta: se abre su pestaña y luego se salta.
  document.addEventListener("click", e => {
    const a = e.target.closest('a[href^="#"]');
    if (!a) return;
    const dest = document.querySelector(a.getAttribute("href"));
    const sec = dest && dest.closest("section[data-t]");
    if (sec && visible(sec)) {
      const i = P.findIndex(p => p[0] === +sec.dataset.t);
      if (i >= 0) { ve(i); setTimeout(()=>dest.scrollIntoView({block:"start"}), 0); }
    }
  });
  const h = decodeURIComponent((location.hash || "").replace(/^#/, ""));
  const pest = h.includes("/") ? h.slice(h.indexOf("/") + 1) : h;
  const porNombre = SLUG.indexOf(pest);
  const m = /^p(\d+)$/.exec(pest);
  ve(porNombre >= 0 ? porNombre : (m && +m[1] < P.length ? +m[1] : 0));
});

/* ---- vdeflate ----
   BLINDADO como el resto. Era el UNICO renderizador de la pagina que corria sin `seguro()`, asi
   que un fallo suyo no se llevaba su seccion: se llevaba la pagina entera. Es exactamente como
   salio el panel en blanco dos veces, y estaba a un caracter de volver a pasar. */
seguro(function(){
  const T = D.vdeflate || [];
  if (T.length < 2) { $("#vdeflate").innerHTML = '<p class="nota">Sin datos.</p>'; return; }
  const cab = T[0], F = T.slice(1), j = n => cab.indexOf(n);
  const I = {s:j("seccion"), k:j("clave"), v:j("valor"), d:j("detalle"), e:j("extra")};
  const de = s => F.filter(f => f[I.s] === s);
  const CL = {"HECHO":"bien", "SIGUIENTE":"ojo", "PLANIFICADO":"mut"};
  const fases = de("fase"), cob = de("cobertura"), suelo = de("suelo"), her = de("herramienta");
  const hechas = fases.filter(f => f[I.v] === "HECHO").length;

  $("#vdeflate").innerHTML = `
    <div class="marc">
      <div class="tar"><div class="k">${hechas}/${fases.length}</div><div class="e">fases hechas</div></div>
      <div class="tar"><div class="k">33,5%</div><div class="e">del 78% ya es nuestro</div></div>
      <div class="tar"><div class="k">1.973</div><div class="e">de 1.973, en las dos direcciones</div></div>
      <div class="tar"><div class="k">24,7×</div><div class="e">por encima del suelo físico</div></div>
    </div>

    <div class="frag-t" style="margin-top:16px"><span>LAS FASES</span>
      <span class="mut">el peso es dentro del 78% que se lleva preflate</span></div>
    ${fases.map(f => `<div class="env-t" style="padding:10px 12px;margin-bottom:4px">
      <div style="display:flex;justify-content:space-between;gap:10px">
        <b style="font-size:12px">${esc(f[I.k])}</b>
        <span class="${CL[f[I.v]]||'mut'}" style="font-size:11px;white-space:nowrap">${esc(f[I.v])}</span></div>
      <div class="mut" style="font-size:11px;margin-top:5px">${esc(f[I.d])}</div>
      <div style="font-size:11px;margin-top:5px"><b>Se demuestra con:</b> ${esc(f[I.e])}</div>
    </div>`).join("")}

    <div class="frag-t" style="margin-top:18px"><span>LA COBERTURA DEL BANCO</span>
      <span class="mut">qué del RFC 1951 ejercitan de verdad los 1.973 flujos</span></div>
    <div class="env-t"><table><tbody>${cob.map(f => `<tr>
      <td style="width:280px">${esc(f[I.k])}</td>
      <td style="font-variant-numeric:tabular-nums">${esc(f[I.v])}</td>
      <td style="width:110px" class="${f[I.d]==='si'?'bien':'ojo'}">${esc(f[I.d])}</td></tr>`).join("")}
    </tbody></table></div>
    <p class="nota"><b>«Pasa el banco» y «es correcto» son dos afirmaciones distintas</b>, y la
      segunda sólo vale hasta donde llegue la primera. Por eso se cuenta qué toca el banco: lo
      que saliera a cero sería lo que no sabemos. Salió a cero <b>una sola vía</b> —la longitud
      258 codificada como 284+31, que zlib nunca genera— y se cerró fabricando el caso a mano y
      contrastándolo <b>contra zlib</b>, no contra nosotros mismos.</p>

    <div class="frag-t" style="margin-top:18px"><span>EL SUELO</span>
      <span class="mut">medido en esta máquina, no estimado</span></div>
    <div class="env-t"><table><tbody>${suelo.map(f => `<tr>
      <td style="width:300px">${esc(f[I.k])}</td>
      <td style="font-variant-numeric:tabular-nums"><b>${esc(f[I.v])}</b></td>
      <td class="mut">${esc(f[I.d])}</td></tr>`).join("")}
    </tbody></table></div>
    <p class="nota">El método bit a bit <b>fue una decisión</b>, no un descuido: está escrito en
      la cabecera del módulo —«el más lento y el más fácil de demostrar correcto»—. Ahora que la
      corrección está cerrada contra 1.973 flujos, ese margen es exactamente lo que queda por
      cobrar: acumulador de 64 bits, tabla de 9 bits, y copias anchas cuando no se solapan.</p>

    <div class="frag-t" style="margin-top:18px"><span>LAS HERRAMIENTAS</span>
      <span class="mut">${her.length}</span></div>
    <div class="env-t"><table><tbody>${her.map(f => `<tr>
      <td style="width:290px"><code style="font-size:11px">${esc(f[I.k])}</code></td>
      <td style="width:80px" class="mut">${esc(f[I.v])}</td>
      <td>${esc(f[I.d])}</td></tr>`).join("")}
    </tbody></table></div>`;
});

/* ---- las preguntas del camino ---- */
seguro(function(){
  const T = D.camino || [];
  if (T.length < 2) { $("#camino").innerHTML = '<p class="nota">Sin datos.</p>'; return; }
  const cab = T[0], F = T.slice(1), j = n => cab.indexOf(n);
  const I = {id:j("id"), q:j("la_pregunta"), p:j("por_que_encuentra_desperdicio"),
             e:j("que_encontro_aqui"), s:j("estado")};
  const arreglada = f => /arreglada/i.test(f[I.s]);
  $("#camino").innerHTML = `
    <div class="marc">
      <div class="tar"><div class="k">${F.length}</div><div class="e">preguntas en la lista</div></div>
      <div class="tar"><div class="k">${F.filter(arreglada).length}</div><div class="e">ya arregladas</div></div>
      <div class="tar"><div class="k">${F.filter(f=>!arreglada(f)).length}</div><div class="e">con arreglo pendiente</div></div>
    </div>
    <p class="nota">Se le hacen a <b>cualquier pieza que ya funcione</b>, de arriba abajo.
      Pasarlas no es estar terminado —siempre queda medir— pero ya no queda desperdicio
      de los baratos. Ninguna se inventó en abstracto: cada una es la generalización de algo
      que apareció mirando este código, y se anota qué la disparó.</p>
    ${F.map(f => `<div class="frag-t" style="margin-top:14px">
        <span><b>${esc(f[I.q])}</b></span><span class="${arreglada(f)?'bien':'mut'}">${esc(f[I.id])}</span></div>
      <div class="env-t" style="padding:10px 12px">
        <div class="mut" style="font-size:12px;margin-bottom:6px">${esc(f[I.p])}</div>
        <div style="font-size:12px"><b>Aquí encontró:</b> ${esc(f[I.e])}</div>
        <div style="font-size:11px;margin-top:5px" class="${arreglada(f)?'bien':'ojo'}">${esc(f[I.s])}</div>
      </div>`).join("")}`;
});

/* ---- la matriz de la auditoria ---- */
seguro(function(){
  const T = D.auditcam || [];
  if (T.length < 2) { $("#matriz").innerHTML = '<p class="nota">Sin datos.</p>'; return; }
  const cab = T[0], F = T.slice(1), j = n => cab.indexOf(n);
  const I = {pz:j("pieza"), dn:j("donde"), ln:j("lineas"), qe:j("que_es"),
             pid:j("pregunta_id"), pq:j("la_pregunta"), v:j("veredicto"), r:j("la_respuesta")};
  const piezas = [...new Set(F.map(f => f[I.pz]))];
  const preg = [...new Set(F.map(f => f[I.pid]))].sort((a,b)=>+a.slice(1)-+b.slice(1));
  const cel = {};
  F.forEach(f => { cel[f[I.pz]+"|"+f[I.pid]] = f; });
  const COL = {"HALLAZGO":"#d9534f", "SOSPECHA":"#e8a33d", "LIMPIO":"#4b9e5f",
               "SIN":"var(--linea)", "NO":"transparent"};
  const cls = v => COL[v.split(" ")[0]] || "var(--linea)";
  const meta = p => { const f = F.find(x => x[I.pz] === p); return f ? f : null; };

  $("#matriz").innerHTML = `
    <div class="marc">
      <div class="tar"><div class="k">${F.filter(f=>f[I.v].startsWith("HALLAZGO")).length}</div><div class="e">hallazgos</div></div>
      <div class="tar"><div class="k">${F.filter(f=>f[I.v]==="LIMPIO").length}</div><div class="e">limpios, y se dice por qué</div></div>
      <div class="tar"><div class="k">${F.filter(f=>f[I.v]==="SIN MIRAR").length}</div><div class="e">sin mirar todavía</div></div>
      <div class="tar"><div class="k">${piezas.length}×${preg.length}</div><div class="e">celdas en total</div></div>
    </div>
    <div style="overflow-x:auto"><table style="font-size:11px;border-collapse:collapse">
      <thead><tr><th style="text-align:left;padding:4px 8px">pieza</th>
        <th style="text-align:right;padding:4px 6px">líneas</th>
        ${preg.map(p => `<th style="padding:4px 3px;font-family:var(--mono);color:var(--tx3)" title="${esc((cel[piezas[0]+"|"+p]||{})[I.pq]||p)}">${p.slice(1)}</th>`).join("")}</tr></thead>
      <tbody>${piezas.map(pz => `<tr>
        <td style="padding:4px 8px;white-space:nowrap"><b>${esc(pz)}</b></td>
        <td style="padding:4px 6px;text-align:right;color:var(--tx3);font-variant-numeric:tabular-nums">${esc((meta(pz)||{})[I.ln]||"")}</td>
        ${preg.map(p => { const f = cel[pz+"|"+p];
          const v = f ? f[I.v] : "SIN MIRAR";
          return `<td style="padding:3px" title="${esc(pz)} · ${esc(p)}: ${esc(v)}">
            <div style="width:15px;height:15px;border-radius:3px;background:${cls(v)};
              border:1px solid var(--linea);margin:auto"></div></td>`; }).join("")}
        </tr>`).join("")}</tbody></table></div>
    <p class="nota" style="margin-top:10px">
      <span style="display:inline-block;width:11px;height:11px;border-radius:2px;background:#d9534f;vertical-align:-1px"></span> hallazgo &nbsp;
      <span style="display:inline-block;width:11px;height:11px;border-radius:2px;background:#e8a33d;vertical-align:-1px"></span> sospecha &nbsp;
      <span style="display:inline-block;width:11px;height:11px;border-radius:2px;background:#4b9e5f;vertical-align:-1px"></span> limpio &nbsp;
      <span style="display:inline-block;width:11px;height:11px;border-radius:2px;background:var(--linea);vertical-align:-1px"></span> sin mirar &nbsp;
      <span style="display:inline-block;width:11px;height:11px;border-radius:2px;border:1px solid var(--linea);vertical-align:-1px"></span> no aplica
    </p>
    ${piezas.map(pz => {
      const h = F.filter(f => f[I.pz] === pz && (f[I.v].startsWith("HALLAZGO") || f[I.v]==="SOSPECHA"));
      if (!h.length) return "";
      const m = meta(pz);
      return `<div class="frag-t" style="margin-top:14px"><span><b>${esc(pz)}</b> · ${esc(m[I.qe])}</span>
        <span class="mut"><code style="font-size:10px">${esc(m[I.dn])}</code></span></div>
        <div class="env-t" style="padding:10px 12px">${h.map(f => `
          <div style="margin-bottom:8px;font-size:12px">
            <span class="${f[I.v]==="SOSPECHA"?"ojo":"mal"}">${esc(f[I.pid])}</span>
            <b>${esc(f[I.pq])}</b><br><span style="color:var(--tx2)">${esc(f[I.r])}</span></div>`).join("")}</div>`;
    }).join("")}`;
});

/* ---- el plan ---- */
seguro(function(){
  const T = D.plan || [];
  if (T.length < 2) { $("#plan").innerHTML = '<p class="nota">Sin datos.</p>'; return; }
  const cab = T[0], F = T.slice(1), j = n => cab.indexOf(n);
  const I = {p:j("paso"), q:j("que"), pq:j("por_que_ahora"), c:j("con_que_se_comprueba"),
             e:j("esfuerzo"), s:j("estado")};
  const CL = {"HOY":"bien", "SIGUIENTE":"ojo"};
  const pasos = [...new Set(F.map(f => f[I.p]))];
  $("#plan").innerHTML = pasos.map(n => `
    <div class="frag-t" style="margin-top:16px"><span>paso ${esc(n)}</span>
      <span class="mut">${F.filter(f=>f[I.p]===n).length} cosa(s)</span></div>
    ${F.filter(f=>f[I.p]===n).map(f => `<div class="env-t" style="padding:10px 12px;margin-bottom:4px">
      <div style="display:flex;justify-content:space-between;gap:10px">
        <b style="font-size:12px">${esc(f[I.q])}</b>
        <span class="${CL[f[I.s]]||'mut'}" style="font-size:11px;white-space:nowrap">${esc(f[I.s])}</span></div>
      <div class="mut" style="font-size:11px;margin-top:5px">${esc(f[I.pq])}</div>
      <div style="font-size:11px;margin-top:5px"><b>Se comprueba con:</b> ${esc(f[I.c])}
        <span class="mut">· ${esc(f[I.e])}</span></div>
    </div>`).join("")}`).join("");
});

/* ---- el censo del desperdicio ---- */
seguro(function(){
  const T = D.desperdicio || [];
  if (T.length < 2) { $("#desperdicio").innerHTML = '<p class="nota">Sin datos.</p>'; return; }
  const cab = T[0], F = T.slice(1), j = n => cab.indexOf(n);
  const I = {id:j("id"), d:j("donde"), b:j("que_hace_bien"), g:j("que_gasta_de_mas"),
             c:j("cuanto"), k:j("camino_corto"), e:j("estado")};
  const ok = f => f[I.e].startsWith("ARREGLADO");
  $("#desperdicio").innerHTML = `
    <div class="marc">
      <div class="tar"><div class="k">${F.filter(f=>!ok(f)).length}</div><div class="e">abiertas</div></div>
      <div class="tar"><div class="k">${F.filter(ok).length}</div><div class="e">arregladas</div></div>
      <div class="tar"><div class="k">${F.length}</div><div class="e">en el censo</div></div>
    </div>
    <p class="nota"><b>Regla de admisión:</b> una línea entra sólo si se puede decir qué hace
      bien, qué gasta de más —con medida, o con un «sin medir» dicho en voz alta— y cuál sería
      el camino corto. Sin las tres es una opinión sobre el código de otro.</p>
    ${F.map(f => `<div class="frag-t" style="margin-top:14px">
        <span>${esc(f[I.id])} · <code style="font-size:11px">${esc(f[I.d])}</code></span>
        <span class="${ok(f)?'bien':'ojo'}">${esc(f[I.e])}</span></div>
      <div class="env-t" style="padding:10px 12px">
        <div style="font-size:12px" class="bien">Hace bien: <span style="color:var(--tx2)">${esc(f[I.b])}</span></div>
        <div style="font-size:12px;margin-top:6px" class="mal">Gasta de más: <span style="color:var(--tx2)">${esc(f[I.g])}</span></div>
        <div style="font-size:11px;margin-top:6px" class="mut">Cuánto: ${esc(f[I.c])}</div>
        <div style="font-size:12px;margin-top:6px"><b>El camino corto:</b> ${esc(f[I.k])}</div>
      </div>`).join("")}`;
});

/* ---- la linea de tiempo: el recorrido, commit a commit -------------------------------------
   La misma vista en las once paginas, cada una con SUS medidas: la global en el principal, la de
   cada herramienta en la suya. Es la tercera pregunta, la que no contestaba nadie: los tests
   miran la salida, la aguja mira si algo empeoro respecto a AYER, la piramide mira donde se va
   el tiempo — y ninguna de las tres enseña el RECORRIDO. */
seguro(function(){
  const L = D.linea || {};
  const H = L.hitos || [];
  const suelo = L.suelo;
  const el = $("#linea");
  if (!H.length) {
    el.innerHTML = `<p class="nota">Ningún experimento de los suyos se ha medido en dos commits
      distintos, así que no hay recorrido que enseñar. <b>No es que no haya cambiado nada</b>: es
      que no se ha medido dos veces lo mismo, que es la única forma de saberlo.</p>`;
    return;
  }
  const mej = H.reduce((a,h)=>a+h.mejora, 0), emp = H.reduce((a,h)=>a+h.empeora, 0);
  const rui = H.reduce((a,h)=>a+h.ruido, 0), suc = H.reduce((a,h)=>a+h.sucias, 0);
  const dp = d => (d>0?"+":"") + (d*100).toFixed(2) + "%";
  const pc2 = v => (v>0?"+":"") + v.toFixed(2) + "%";
  /* EL GRAFICO PRIMERO: cuarenta y seis commits en una tabla son cuarenta y seis filas que nadie
     lee. La forma se ve de un golpe — y lo que hay que ver de un golpe es si la linea baja
     (mejor) o sube (peor), y si alguna de las cuatro va al reves que las otras, que es el caso
     interesante: comprimir mas casi siempre se paga en tiempo. */
  const S = (L.series || []).filter(x => x.puntos && x.puntos.length > 1);
  const A = L.acum || {};

  el.innerHTML = `
    ${S.length ? `
      <div class="marc" style="margin-bottom:4px">
        ${S.map(x => `<div class="tar">
          <div class="k ${A[x.nombre]<0?'bien':A[x.nombre]>0?'mal':'mut'}">${pc2(A[x.nombre]||0)}</div>
          <div class="e">${esc(x.nombre)} · <b>mediana</b> de ${(L.acum_det&&L.acum_det[x.nombre]||{}).n||0}
            experimentos${(L.acum_det&&L.acum_det[x.nombre]&&L.acum_det[x.nombre].n>1)
            ? ` · de ${L.acum_det[x.nombre].min}% a ${L.acum_det[x.nombre].max}%` : ""}</div></div>`).join("")}
      </div>
      ${curva(S, "commits, del más viejo al más nuevo", "cambio acumulado (%)",
              {cero:true, etiquetasX:(L.etiquetas||[]), unidad:"%"})}
      <p class="nota"><b>Los números de arriba son la MEDIANA de lo que ha acumulado cada
        experimento por separado</b>, no un total del producto. Antes se promediaban todos los
        experimentos de cada commit y se multiplicaba, y eso contaba la misma mejora una vez por
        cada ventana desde la que se mira: <code>F13 carril</code> tiene ocho variantes,
        <code>F14 frame</code> tres y <code>CERO archivar el árbol</code> tres vueltas, y las
        catorce son <b>el mismo archivador</b>. Salía un −43% de reloj que ninguna medida absoluta
        respalda: las de punta a punta sobre el mismo árbol dan entre −11,5% y −19,9%.
        <b>Y esto sigue sin ser «cuánto ha mejorado el producto»</b>: para decir eso hace falta un
        experimento que sea el producto entero medido en las dos puntas.</p>
      <p class="nota"><b>Ese experimento ya existe y se llama <code>REFERENCIA</code></b>
        (<code>bench/referencia.sh</code>, desde el 03-08): tres filas — el árbol real con los
        defectos, la fase 3, y <code>--max</code>, que es la única configuración en la que corre el
        CM (<code>set_cm(maximo)</code>). Se corren <b>iguales en cada cierre</b>, y por eso dan
        pares. Antes de que existieran, un día entero de trabajo podía no mover esta curva: no
        porque no hubiera mejoras, sino porque una <b>ablación A/B son dos configuraciones en el
        MISMO commit</b> y aquí se dibuja el mismo experimento en <b>dos commits distintos</b>.</p>
      <p class="nota">⚠️ <b>Un par sólo cuenta si los dos lados identifican su código.</b> Desde el
        02-08 la báscula sella <code>arbol_sha</code> —el digesto de <code>crates/</code> tal y como
        está en el disco— y esta curva lo usa para saber de qué commit es cada medida, en vez de
        creerse el que hubiera en HEAD al medir (aquí se mide primero y se commitea después, así
        que eso ponía la fila en el commit anterior). Las filas anteriores a ese sello tomadas con
        el código sucio <b>no se pueden atribuir</b> y se cuentan aparte, en la columna
        «no comparables». No es que se escondan: es que no se sabe qué midieron.</p>
      <p class="nota">Cada línea es <b>el recorrido</b> de esa métrica: se compone multiplicando,
        no sumando porcentajes. <b>Bajar es mejorar</b> en las cuatro — menos bytes, menos
        segundos, menos ciclos y menos memoria. Un tramo plano no es que no se tocara nada: es
        que <b>nadie volvió a medir lo mismo</b> en ese commit, o que el cambio cayó dentro del
        ruido y no se cuenta como movimiento.</p>`
     : `<p class="nota">Todavía no hay dos medidas del mismo experimento en commits distintos,
        así que no hay curva que dibujar. Con una sola medida no se sabe si algo mejoró.</p>`}
    <div class="marc">
      <div class="tar"><div class="k">${L.medidos}</div><div class="e">commits con medidas</div></div>
      <div class="tar"><div class="k bien">${mej}</div><div class="e">mejoras reales</div></div>
      <div class="tar"><div class="k ${emp?'mal':'mut'}">${emp}</div><div class="e">empeoramientos</div></div>
      <div class="tar"><div class="k mut">${rui}</div><div class="e">dentro del ruido</div></div>
      <div class="tar"><div class="k ${suc?'ojo':'mut'}">${suc}</div>
        <div class="e">no comparables · árbol sucio</div></div>
    </div>
    <p class="nota">${suelo != null
      ? `Suelo de ruido de esta báscula: <b>${(suelo*100).toFixed(2)}%</b>. Por debajo de ahí no
         se llama mejora ni empeoramiento.`
      : `<b>Sin suelo de ruido</b>: no hay ningún experimento repetido 3 veces con el árbol limpio
         y carga comparable, así que no se puede decir qué delta es real y cuál es la máquina.
         Todo lo de abajo se enseña, pero sin ese umbral no hay veredicto que sostener.`}</p>
    ${H.map(h => `
      <div class="frag-t" style="margin-top:14px">
        <span><code>${esc(h.commit)}</code> ${esc(h.asunto || "(commit que no está en el log)")}</span>
        <span class="mut">${h.fecha ? esc(h.fecha.slice(0,16).replace("T"," ")) : ""}</span></div>
      <div class="env-t" style="padding:10px 13px">
        <div style="display:flex;gap:16px;flex-wrap:wrap;font-size:11px">
          <span class="${h.mejora?'bien':'mut'}"><b>${h.mejora}</b> mejor</span>
          <span class="${h.empeora?'mal':'mut'}"><b>${h.empeora}</b> peor</span>
          <span class="mut"><b>${h.ruido}</b> en el ruido</span>
          ${h.sucias?`<span class="ojo"><b>${h.sucias}</b> no comparables</span>`:""}
          ${h.nuevos?`<span class="mut"><b>${h.nuevos}</b> medidos por primera vez</span>`:""}
        </div>
        ${h.cambios.length ? `<table style="margin-top:8px"><tbody>
          ${h.cambios.map(c=>`<tr>
            <td class="${c.veredicto==='mejora'?'bien':'mal'}" style="white-space:nowrap">
              ${dp(c.delta)}</td>
            <td class="mut" style="white-space:nowrap">${esc(c.metrica)}</td>
            <td>${esc(c.etiqueta)}${c.muestra?` <span class="mut">· ${esc(c.muestra)}</span>`:""}</td>
            <td class="mut" style="white-space:nowrap">desde <code>${esc(c.desde)}</code>
              · n=${c.n_antes}→${c.n_despues}</td></tr>`).join("")}
        </tbody></table>` : `<div class="mut" style="font-size:11px;margin-top:6px">Nada que
          comparar con el commit anterior.</div>`}
      </div>`).join("")}`;
});

/* ---- contra los rivales, en el tiempo -------------------------------------------------------
   La curva de deltas de arriba y esta contestan preguntas distintas y hacen falta las dos: una
   dice cuanto nos hemos movido, esta dice donde nos deja. El 27-07 el producto mejoro un 33% y
   SEGUIA por detras de zpaq; el 28-07 mejoro un 0,3% y ya iba por delante. Con una sola de las
   dos curvas, las dos frases suenan igual. */
seguro(function(){
  const el = $("#rivlinea");
  const R = D.rivlinea || {};
  const C = (R.cuadros || []).filter(c => c.dibuja && c.puntos.length);
  if (!C.length) {
    el.innerHTML = `<p class="nota">Ninguna entrada tiene <b>dos fechas</b> con el producto
      medido igual, así que no hay recorrido contra nadie que dibujar. Una sola medida es una
      foto, y la foto ya está en «Contra los rivales». Para que aparezca aquí basta con medir con
      la báscula poniendo la etiqueta <code>RIVALES: …</code> y la muestra con el mismo nombre que
      usa la tabla de rivales.</p>`;
    return;
  }
  const MB = n => n/1048576;
  const NOM = {max:"vcompresor --max", defecto:"vcompresor por defecto"};
  el.innerHTML = C.map(c => {
    const F = c.fechas, ix = f => F.indexOf(f);
    /* Las configuraciones son SERIES DISTINTAS y no se mezclan: `--max` y el modo por defecto son
       dos productos con dos precios. Juntarlos daria una curva que baja cada vez que alguien
       midio con la bandera puesta. */
    const cfgs = [...new Set(c.puntos.map(p => p.config))].sort();
    const mias = cfgs.map(cf => ({
      nombre: NOM[cf] || ("vcompresor " + cf),
      puntos: F.map(f => {
        // Si el mismo dia hay dos medidas de la misma configuracion, manda la MEJOR: son el
        // mismo producto medido dos veces, no dos productos.
        const ps = c.puntos.filter(p => p.config === cf && p.fecha === f);
        if (!ps.length) return {x: ix(f), y: null};
        const p = ps.reduce((a,b) => b.bytes < a.bytes ? b : a);
        return {x: ix(f), y: +MB(p.bytes).toFixed(3), etq: f + " · " + p.commit};
      })}));
    const mejor = Math.min(...c.puntos.map(p => p.bytes));
    /* QUE RIVALES SE DIBUJAN. Los que dejan mas del DOBLE que nuestro mejor punto aplastan la
       parte de la grafica donde pasa todo —zip -9 deja 55 MB donde nosotros dejamos 9,4— y
       ademas nunca estuvieron en juego. Se quedan fuera de la linea y se listan debajo CON SU
       CIFRA: el criterio recorta el dibujo, no el dato. */
    const dentro = c.rivales.filter(r => r.bytes <= 2*mejor);
    const fuera  = c.rivales.filter(r => r.bytes >  2*mejor);
    const series = mias.concat(dentro.map(r => ({
      nombre: r.nom + (r.clase === "no es producto" ? " (no es un producto)" : ""),
      puntos: F.map(f => ({x: ix(f), y: +MB(r.bytes).toFixed(3), etq: "constante"}))})));
    // La foto de hoy: el ultimo punto de cada configuracion contra cada rival dibujado.
    const ult = {};
    cfgs.forEach(cf => {
      const ps = c.puntos.filter(p => p.config === cf);
      if (ps.length) ult[cf] = ps[ps.length-1];
    });
    const pct = (a,b) => ((a-b)/b*100);
    const tarj = [];
    Object.keys(ult).forEach(cf => dentro.forEach(r => {
      const d = pct(ult[cf].bytes, r.bytes);
      tarj.push(`<div class="tar"><div class="k ${d<0?'bien':'mal'}">${
        (d>0?"+":"")+d.toFixed(1)}%</div><div class="e">${esc(NOM[cf]||cf)} contra
        <b>${esc(r.nom)}</b> · ${esc(ult[cf].fecha)}</div></div>`);
    }));
    const noVer = c.puntos.filter(p => !p.ok);
    return `
      <div class="frag-t" style="margin-top:14px"><span><b>${esc(c.muestra)}</b>${
        c.original ? ` · entrada de ${c.original.toLocaleString("es-ES")} B` : ""}</span>
        <span class="mut">${c.puntos.length} medidas · ${F.length} fechas</span></div>
      ${tarj.length ? `<div class="marc" style="margin:8px 0 4px">${tarj.join("")}</div>` : ""}
      ${curva(series, "fecha de la medida", "MB del archivo · menos es mejor",
              {etiquetasX: F, unidad: "MB"})}
      <p class="nota"><b>Bajar es ganar.</b> Cada recta es un rival medido en esta misma máquina
        sobre esta misma entrada; el punto donde nuestra línea la cruza es el día que se le pasó.
        ${fuera.length ? `Fuera del dibujo por dejar más del doble que nuestro mejor punto —no
        por esconderlos—: ${fuera.map(r => `<b>${esc(r.nom)}</b> ${
          (r.bytes/1048576).toFixed(1)} MB`).join(" · ")}.` : ""}</p>
      <table style="margin-top:8px"><thead><tr><th>fecha</th><th>config</th><th>bytes</th>
        <th>commit</th><th>de dónde sale</th><th>¿está el número dentro?</th></tr></thead><tbody>
        ${c.puntos.map(p => `<tr>
          <td>${esc(p.fecha)}</td><td class="mut">${esc(p.config)}</td>
          <td><b>${p.bytes.toLocaleString("es-ES")}</b></td>
          <td class="mut"><code>${esc(p.commit)}</code></td>
          <td class="mut" style="text-align:left">${esc(p.fuente)}</td>
          <td class="${p.ok?'bien':'mal'}">${p.ok ? "sí · "+esc(p.de) : "NO VERIFICADO"}</td>
        </tr>`).join("")}</tbody></table>
      ${noVer.length ? `<p class="nota mal">⚠️ <b>${noVer.length} punto(s) sin verificar</b>: el
        número no aparece en el fichero que citan. Mientras esté así, ese punto <b>no vale</b>
        aunque esté dibujado — o el número se copió mal, o el fichero cambió debajo.</p>` : ""}`;
  }).join("");
});

/* ---- la herramienta de esta pagina, de un vistazo -------------------------------------------
   Solo existe en las paginas de herramienta: en el principal `D.ficha` no viene y esto se calla.
   Es lo que hace que una herramienta RECIEN NACIDA tenga pagina propia el mismo dia que nace, con
   sus ceros delante. vlz nacio el 31-07 y no salia en ningun sitio del panel porque la regla del
   censo viejo era «una herramienta sin banco propio NO tiene ficha» — o sea, lo que todavia no se
   ha medido no existe. Justo al reves: un cero aqui no es un hueco del panel, ES EL DATO. Dice
   que nunca ha pasado por la bascula, y eso es lo que hay que ver para arreglarlo. */
seguro(function(){
  const F = D.ficha;
  if (!F) return;
  const C = F.camino || {hechas:0, hallazgos:0, sin:0, total:0};
  const med = F.medidas || 0, tabs = F.tablas || [], loc = F.loc || [];
  const nloc = loc.reduce((a,b)=>a+b[1], 0);
  const propio = F.sustituye_a && !F.sustituye_a.startsWith("—");

  /* Su camino: las MISMAS preguntas que se le hacen a todo —viajan intactas, ese es el
     instrumento— cruzadas con SU veredicto. Lo que no se ha mirado sale en gris y contado: una
     auditoria que solo enseña lo que encontro parece completa y no lo esta. */
  const AC = (D.auditcam || []).slice(1);
  const jc = (D.auditcam || [[]])[0];
  const ci = n => jc.indexOf(n);
  const porId = {};
  AC.forEach(r => { if (r.length > ci("veredicto")) porId[r[ci("pregunta_id")]] = r; });
  const P = (D.camino || []).slice(1);
  const jp = (D.camino || [[]])[0];
  const pi = n => jp.indexOf(n);
  const VCL = v => v.startsWith("HALLAZGO") ? "mal" : v.startsWith("SOSPECHA") ? "ojo"
                 : v.startsWith("SIN") ? "mut" : v.startsWith("NO") ? "mut" : "bien";

  $("#ficha").innerHTML = `
    <div class="frag-t"><span><b>${esc(F.nombre)}</b>${propio
      ? ` <span class="mut">· nació para sustituir a ${esc(F.sustituye_a)}</span>` : ""}</span>
      <span class="mut">${esc(F.codigo)}</span></div>
    <div class="env-t" style="padding:11px 13px"><div style="font-size:12px">${esc(F.que_es)}</div></div>
    <div class="marc" style="margin-top:14px">
      <div class="tar"><div class="k ${med?'':'mal'}">${med}</div>
        <div class="e">medidas suyas en la báscula${F.declaradas!=null&&med
          ? ` · ${F.declaradas} declaradas` : ""}</div></div>
      <div class="tar"><div class="k ${C.hechas?'':'mal'}">${C.hechas}<span class="mut" style="font-size:13px">/${C.total}</span></div>
        <div class="e">preguntas del camino hechas</div></div>
      <div class="tar"><div class="k ${C.hallazgos?'mal':'mut'}">${C.hallazgos}</div>
        <div class="e">hallazgos abiertos</div></div>
      <div class="tar"><div class="k">${tabs.length}</div>
        <div class="e">tablas del banco son suyas</div></div>
      <div class="tar"><div class="k">${fm(nloc)}</div>
        <div class="e">líneas de código${loc.length?` · ${loc.length} ficheros`:""}</div></div>
    </div>
    ${med === 0 ? `<p class="nota mal" style="margin-top:12px"><b>Ni una medida suya ha pasado
      por <code>bench/mide.sh</code>.</b> No es que sea barata ni que vaya bien: es que <b>no se
      sabe</b>. Sus cifras, si las hay, viven en logs sueltos y no en el libro mayor, así que
      nadie puede repetirlas ni compararlas con las de ayer. Se mide con
      <code>bench/mide.sh -H ${esc(F.clave)} ...</code>.</p>` : ""}
    ${C.hechas === 0 ? `<p class="nota" style="margin-top:10px">A esta herramienta <b>no se le ha
      hecho ninguna</b> de las ${C.total} preguntas del camino. No significa que esté limpia:
      significa que no se ha mirado. Se corre con <code>bench/auditoria-camino.py</code>.</p>` : ""}

    <div class="frag-t" style="margin-top:18px"><span>Su camino — las mismas ${C.total} preguntas,
      sus respuestas</span><span class="mut">${C.sin} sin mirar</span></div>
    <div class="env-t"><table><thead><tr><th>#</th><th>la pregunta</th><th>veredicto</th>
      <th>qué encontró <b>aquí</b></th></tr></thead><tbody>
      ${P.map(q => {
        const id = q[pi("id")], r = porId[id];
        const v = r ? r[ci("veredicto")] : "SIN MIRAR";
        const resp = r ? (r[ci("la_respuesta")] || "") : "";
        return `<tr><td class="n">${esc(id)}</td>
          <td>${esc(q[pi("la_pregunta")])}</td>
          <td class="${VCL(v)}">${esc(v)}</td>
          <td class="${resp?'':'mut'}">${esc(resp || "—")}</td></tr>`;}).join("")}
    </tbody></table></div>

    <div class="frag-t" style="margin-top:18px"><span>Sus tablas del banco</span>
      <span class="mut">${tabs.length}</span></div>
    <div class="env-t" style="padding:11px 13px">
      ${tabs.length ? `<div class="mut" style="font-size:10px;line-height:1.9">
        ${tabs.map(t=>`<code>${esc(t)}</code>`).join(" ")}</div>`
       : `<span class="mal" style="font-size:11px">Ninguna tabla del banco es suya.</span>`}
      ${loc.length ? `<div style="font-size:11px;margin-top:10px">
        ${loc.map(l=>`<div style="display:flex;justify-content:space-between">
          <code class="mut">${esc(l[0])}</code><span>${fm(l[1])}</span></div>`).join("")}</div>` : ""}
    </div>`;
});

/* ---- nuestras herramientas, una por una ---- */
seguro(function(){
  const T = D.propias || [];
  if (T.length < 2) { $("#propias").innerHTML = '<p class="nota">Sin datos. Corre bench/propias.py.</p>'; return; }
  const cab = T[0], F = T.slice(1), j = n => cab.indexOf(n);
  const I = {k:j("clave"), n:j("nombre"), q:j("que_es"), s:j("sustituye_a"), c:j("codigo"),
             p:j("pestana"), e:j("estado"), ph:j("preguntas_hechas"), hh:j("hallazgos"),
             ps:j("preguntas_sin_hacer"), m:j("medidas"), nt:j("tablas_propias"), t:j("sus_tablas")};
  const num = v => parseInt(v) || 0;
  const propio = f => !f[I.s].startsWith("—");
  const CL = {"REEMPLAZADA":"bien", "EN OBRA":"ojo", "CONVIVEN":"", "SE QUEDA":"mut", "A AISLAR":"mal"};

  $("#propias").innerHTML = `
    <div class="marc">
      <div class="tar"><div class="k">${F.length}</div><div class="e">herramientas nuestras</div></div>
      <div class="tar"><div class="k">${F.filter(propio).length}</div>
        <div class="e">nacieron para sustituir a una de fuera</div></div>
      <div class="tar"><div class="k">${F.reduce((a,f)=>a+num(f[I.hh]),0)}</div>
        <div class="e">hallazgos abiertos entre todas</div></div>
      <div class="tar"><div class="k ${F.filter(f=>num(f[I.m])===0).length?'mal':''}">${F.filter(f=>num(f[I.m])===0).length}</div>
        <div class="e">sin una sola medida en la báscula</div></div>
    </div>
    <p class="nota"><b>Por qué cada una lleva lo suyo aparte.</b> Una cifra de una herramienta
      que decide por otra parece un dato y es un error. Pasó el 31-07: <code>--price</code>
      estimaba la CPU con una tabla que mide <b>el códec solo</b> para decidir sobre <b>el
      archivador entero</b> — 34,8× de diferencia entre niveles en una, 1,79× en la otra, las dos
      correctas, ninguna válida para lo del otro.</p>
    ${F.map(f => {
      const med = num(f[I.m]), hal = num(f[I.hh]), sin = num(f[I.ps]);
      const tabs = (f[I.t]||"").split(" ").filter(Boolean);
      return `<div class="frag-t" style="margin-top:16px" id="h-${esc(f[I.k])}">
        <span><b>${esc(f[I.n])}</b>${propio(f)
          ? ` <span class="mut">· sustituye a ${esc(f[I.s])}</span>` : ""}</span>
        <span class="${CL[f[I.e]]!==undefined?CL[f[I.e]]:'mut'}">${esc(f[I.e])}</span></div>
      <div class="env-t" style="padding:11px 13px">
        <div style="font-size:12px">${esc(f[I.q])}</div>
        <div class="mut" style="font-size:11px;margin-top:6px">
          <code>${esc(f[I.c])}</code>${f[I.p]!=="—" ? ` · su pestaña: <b>${esc(f[I.p])}</b>` : ""}</div>
        <div style="display:flex;gap:18px;flex-wrap:wrap;margin-top:9px;font-size:11px">
          <span><b>${f[I.ph]}</b> preguntas hechas${sin?` <span class="ojo">· ${sin} sin hacer</span>`:""}</span>
          <span class="${hal?'mal':'mut'}"><b>${hal}</b> hallazgos</span>
          <span class="${med?'':'mal'}"><b>${med}</b> medidas en la báscula</span>
          <span class="mut"><b>${f[I.nt]}</b> tablas propias</span>
        </div>
        ${tabs.length ? `<div class="mut" style="font-size:10px;margin-top:7px;line-height:1.7">
          ${tabs.map(t=>`<code>${esc(t)}</code>`).join(" ")}</div>` : ""}
        ${med===0 ? `<div class="mal" style="font-size:11px;margin-top:7px">⚠ Ni una medida suya
          ha pasado por <code>bench/mide.sh</code>. Eso no es que sea barata: es que sus cifras
          viven en logs sueltos y no en el libro mayor.</div>` : ""}
      </div>`;}).join("")}`;
});

/* ---- que se decidio, y con que medida ---- */
seguro(function(){
  const T = D.decisiones || [];
  if (T.length < 2) {
    $("#decisiones").innerHTML =
      '<p class="nota">Sin datos. Corre <code>bench/decide.py</code>.</p>'; return; }
  const c = T[0], F = T.slice(1), j = n => c.indexOf(n);
  const I = {id:j("id"), que:j("que"), ver:j("veredicto"), jue:j("juego"), est:j("estado"),
             by:j("bytes_pct"), ci:j("ciclos_pct"), med:j("medidas"), fal:j("faltan"),
             eti:j("etiqueta"), det:j("detalle"), cua:j("cuando"), anc:j("ancla"),
             toc:j("tocado_desde"), cad:j("caduca_si")};
  /* En las dos columnas MENOS ES MEJOR, así que el signo ya dice si es bueno. Aquí no se
     recalcula nada: se pinta lo que escribió decide.py. Si esta vista hiciera su propia cuenta
     habría dos cifras del mismo experimento, que es justo el fallo que esto viene a cerrar. */
  const col = v => (!v || v === "-") ? "mut" : (v[0] === "-" ? "bien" : "mal");
  const VER = {"APLICADO":"bien", "RECHAZADO":"mut", "AL MENU":"ojo", "PENDIENTE":"ojo"};
  /* Dos niveles de alarma, y la diferencia importa: GRAVE es «esta decisión no está sostenida»
     —falta media muestra, o el código no dice lo que la decisión afirma—. AVISO es «mírala»:
     se puede comprobar, pero el terreno se movió. Pintarlas del mismo color las hace ignorables
     a las dos. */
  const grave = e => /SIN MEDIDA|INCOMPLETA|DESANCLADA|ANCLA /.test(e);
  const cls = e => e === "OK" ? "bien" : (grave(e) ? "mal" : "ojo");
  const nOK = F.filter(f => f[I.est] === "OK").length;
  const nGrave = F.filter(f => grave(f[I.est])).length;
  const nApl = F.filter(f => f[I.ver] === "APLICADO").length;

  $("#decisiones").innerHTML = `
    <div class="marc">
      <div class="tar"><div class="k">${F.length}</div>
        <div class="e">decisiones con medida detrás</div></div>
      <div class="tar"><div class="k bien">${nApl}</div>
        <div class="e">aplicadas · el resto rechazadas o al menú</div></div>
      <div class="tar"><div class="k ${nGrave?'mal':'bien'}">${nGrave}</div>
        <div class="e"><b>sin sostener</b>: falta muestra o no casa el ancla</div></div>
      <div class="tar"><div class="k ${nOK===F.length?'bien':'ojo'}">${nOK}/${F.length}</div>
        <div class="e">limpias: completas, ancladas y frescas</div></div>
    </div>
    ${nGrave ? `<p class="nota"><b>Qué significa «sin sostener».</b> No es que el resultado sea
      falso: es que <b>la medida no cubre lo que la decisión afirma</b>. El juego canónico son
      cinco muestras de texto (<code>bench/muestras.tsv</code>) y hay decisiones cerradas sobre
      cuatro — siempre falta la misma, <code>enwik8</code>, que es la que castiga los órdenes
      altos. Una es la que se <b>aplicó por defecto</b>. Con las cuatro cómodas delante se
      concluye lo que apetece; la que falta suele ser justo la que no gusta.</p>` : ""}
    <div class="env-t" style="padding:11px 13px;margin-top:14px">
      ${F.map(f=>`<div style="font-size:12px;margin:12px 0;padding-bottom:11px;
          border-bottom:1px solid var(--linea)">
        <div style="display:flex;gap:8px;align-items:baseline;flex-wrap:wrap">
          <span class="chip ${VER[f[I.ver]]||'mut'}">${esc(f[I.ver])}</span>
          <b>${esc(f[I.que])}</b>
          <span class="mut" style="font-size:10px">${esc(f[I.jue])} · ${esc(f[I.med])}</span>
          <span class="chip ${cls(f[I.est])}" style="margin-left:auto">${esc(f[I.est])}</span>
        </div>
        <div style="margin-top:6px;display:flex;gap:18px;flex-wrap:wrap">
          <span>bytes <b class="${col(f[I.by])}">${esc(f[I.by])}</b></span>
          <span>ciclos <b class="${col(f[I.ci])}">${esc(f[I.ci])}</b></span>
          <span class="mut" style="font-size:10px">${esc(f[I.cua])} · ancla: ${esc(f[I.anc])}</span>
        </div>
        <div class="mut" style="font-size:10.5px;margin-top:4px">${esc(f[I.det])}</div>
        ${f[I.fal] && f[I.fal] !== "-" ? `<div class="mal" style="font-size:11px;margin-top:4px">
          falta medir: <b>${esc(f[I.fal])}</b></div>` : ""}
        ${f[I.toc] && f[I.toc] !== "-" ? `<div class="ojo" style="font-size:10.5px;margin-top:3px">
          el fichero se tocó después de medir: ${esc(f[I.toc])}</div>` : ""}
        ${f[I.cad] ? `<div class="mut" style="font-size:10.5px;margin-top:4px">
          <b>caduca si</b> ${esc(f[I.cad])}</div>` : ""}
      </div>`).join("")}
    </div>
    <p class="nota" style="margin-top:12px">Lo declarado a mano vive en
      <code>bench/decisiones.tsv</code> y <b>no lleva ni un número</b>: sólo la identidad del
      experimento —qué etiqueta del libro mayor lo sostiene, qué configuración es el control y
      cuál el tratamiento— más el porqué y la premisa que lo caduca. Los números los pone
      <code>bench/decide.py</code> leyendo <code>historia.jsonl</code>. Una cifra, un sitio.</p>`;
});

/* ---- que esta aplicado y en que version estamos ---- */
seguro(function(){
  const V = D.aplicado_version || [], A = D.aplicado || [];
  if (V.length < 2 || A.length < 2) {
    $("#aplicado").innerHTML = '<p class="nota">Sin datos. Corre bench/aplicado.py.</p>'; return; }
  const vc = V[0], VF = V.slice(1), vj = n => vc.indexOf(n);
  const IV = {q:vj("que"), v:vj("valor"), d:vj("de_donde"), p:vj("por_que_importa")};
  const ac = A[0], AF = A.slice(1), aj = n => ac.indexOf(n);
  const IA = {c:aj("commit"), t:aj("cuando"), q:aj("que_hizo"), m:aj("medidas"), d:aj("de_que")};
  const val = n => { const f = VF.find(x => x[IV.q] === n); return f ? f[IV.v] : "?"; };
  const sin = AF.filter(f => f[IA.m] === "0");

  $("#aplicado").innerHTML = `
    <div class="marc">
      <div class="tar"><div class="k">v${esc(val("modelo de vc-cm"))}</div>
        <div class="e">modelo de vc-cm</div></div>
      <div class="tar"><div class="k">${esc(val("frame por defecto (MiB)"))} MiB</div>
        <div class="e">frame por defecto</div></div>
      <div class="tar"><div class="k bien">${esc(val("modelos que sabemos abrir"))}</div>
        <div class="e">modelos que sabemos abrir</div></div>
      <div class="tar"><div class="k ${sin.length?'ojo':'bien'}">${sin.length}</div>
        <div class="e">de ${AF.length} commits <b>sin medidas en el banco</b></div></div>
    </div>
    ${sin.length ? `<p class="nota"><b>Por qué la curva de abajo puede salir plana.</b> Esos
      ${sin.length} commits no aparecen en ella, y no porque no hicieran nada: sus medidas
      <b>no pasaron por <code>bench/mide.sh</code></b>, así que no están en
      <code>historia.jsonl</code>. Casi siempre es que se midieron en el Mac — que no tiene
      báscula capaz de escribir en el banco ni es repo git, así que no puede sellar el commit.
      Mientras eso siga así, <b>cuanto más se trabaje en el Mac más ciega se queda esta vista</b>.
      El número está aquí para que se vea, no para que se olvide.</p>` : ""}
    <div class="frag-t" style="margin-top:14px"><span><b>Las versiones</b></span>
      <span class="mut">leídas del fuente</span></div>
    <div class="env-t" style="padding:11px 13px">
      ${VF.map(f=>`<div style="font-size:12px;margin:7px 0">
        <b>${esc(f[IV.q])}</b> <span class="bien">${esc(f[IV.v])}</span>
        <span class="mut" style="font-size:10px"> · ${esc(f[IV.d])}</span>
        <div class="mut" style="font-size:11px;margin-top:2px">${esc(f[IV.p])}</div></div>`).join("")}
    </div>
    <div class="frag-t" style="margin-top:14px"><span><b>Qué ha entrado</b></span>
      <span class="mut">${AF.length} commits · ${AF.length - sin.length} con medidas</span></div>
    <div class="env-t" style="padding:11px 13px">
      ${AF.map(f=>`<div style="font-size:11px;margin:6px 0;display:flex;gap:9px">
        <code style="flex:0 0 62px">${esc(f[IA.c])}</code>
        <span class="mut" style="flex:0 0 108px">${esc(f[IA.t])}</span>
        <span class="${f[IA.m]==='0'?'ojo':'bien'}" style="flex:0 0 58px">${
          f[IA.m]==='0' ? 'sin medir' : esc(f[IA.m]) + ' med.'}</span>
        <span style="flex:1">${esc(f[IA.q])}${f[IA.d] ? `<span class="mut"> · ${esc(f[IA.d])}</span>` : ""}</span>
        </div>`).join("")}
    </div>`;
});

/* ---- las junturas: la ley R11 ---- */
seguro(function(){
  const T = D.junturas || [];
  if (T.length < 2) { $("#junturas").innerHTML = '<p class="nota">Sin datos. Corre bench/junturas.py.</p>'; return; }
  const cab = T[0], F = T.slice(1), j = n => cab.indexOf(n);
  const I = {de:j("de"), a:j("a"), q:j("que_deberia_cruzar"), c:j("cruza"),
             iv:j("ida_y_vuelta"), pr:j("como_se_demuestra"), co:j("cuanto_cuesta"),
             md:j("con_que_se_midio")};
  const U = D.undisparo || [], UF = U.length > 1 ? U.slice(1) : [];
  const sitios = UF.reduce((a,f)=>a+(parseInt(f[3])||0), 0);
  const noCruza = F.filter(f => f[I.c].startsWith("NO"));
  const sinMedir = F.filter(f => f[I.co].startsWith("SIN MEDIR"));
  const sinIV = F.filter(f => f[I.iv] === "NO");
  const cl = f => f[I.c].startsWith("NO") ? "mal" : (f[I.c] === "A MEDIAS" ? "ojo" : "bien");

  $("#junturas").innerHTML = `
    <div class="marc">
      <div class="tar"><div class="k">${F.length}</div><div class="e">junturas censadas</div></div>
      <div class="tar"><div class="k ${noCruza.length?'mal':''}">${noCruza.length}</div>
        <div class="e">por las que hoy no cruza nada</div></div>
      <div class="tar"><div class="k ${sinMedir.length?'ojo':''}">${sinMedir.length}</div>
        <div class="e">sin medir lo que cuestan</div></div>
      <div class="tar"><div class="k ${sinIV.length?'mal':''}">${sinIV.length}</div>
        <div class="e">sin ida y vuelta demostrada</div></div>
    </div>
    ${sitios ? `<p class="nota"><b>El detector, y es automático.</b> Casi toda librería de fuera
      tiene dos caras: una función de un disparo que se lo traga todo por defecto, y una versión
      configurable. Llamar a la de un disparo es, literalmente, no pasarle lo que sabemos.
      <code>bench/junturas.py</code> recorre el código y encuentra
      <b class="${sitios?'ojo':''}">${sitios} sitios</b> en <b>${UF.length}</b> formas distintas.
      <b>Esto habría cazado el fallo de la ventana.</b> Una lista escrita a mano, no.</p>
      <div class="env-t" style="padding:10px 13px;margin-bottom:14px">
      ${UF.map(f=>`<div style="font-size:11px;margin:5px 0">
        <code>${esc(f[0])}</code> <span class="ojo">${esc(f[3])} sitios</span>
        <div class="mut" style="font-size:10px;margin-top:2px">se traga: ${esc(f[1])}</div>
        <div class="mut" style="font-size:10px">acepta lo que sabemos: <code>${esc(f[2])}</code></div>
      </div>`).join("")}</div>` : ""}
    ${F.map(f=>`<div class="frag-t" style="margin-top:14px">
        <span><b>${esc(f[I.de])}</b> <span class="mut">→</span> <b>${esc(f[I.a])}</b></span>
        <span class="${cl(f)}">${esc(f[I.c])}</span></div>
      <div class="env-t" style="padding:11px 13px">
        <div style="font-size:12px">${esc(f[I.q])}</div>
        <div style="font-size:11px;margin-top:8px">
          <span class="${f[I.iv]==='NO'?'mal':(f[I.iv]==='SI'?'bien':'mut')}">ida y vuelta:
            ${esc(f[I.iv])}</span>
          <span class="mut"> · ${esc(f[I.pr])}</span></div>
        <div style="font-size:11px;margin-top:6px" class="${f[I.co].startsWith('SIN MEDIR')?'ojo':''}">
          <b>cuesta:</b> ${esc(f[I.co])}</div>
        ${f[I.md] !== "—" ? `<div class="mut" style="font-size:10px;margin-top:4px">
          medido con: ${esc(f[I.md])}</div>` : ""}
      </div>`).join("")}`;
});

/* ---- el techo conocido, y si podemos pasarlo ---- */
seguro(function(){
  const T = D.techos || [];
  if (T.length < 2) { $("#techos").innerHTML = '<p class="nota">Sin datos. Corre bench/techos.py.</p>'; return; }
  const cab = T[0], F = T.slice(1), j = n => cab.indexOf(n);
  const I = {p:j("pieza"), q:j("que_hace"), t:j("el_techo_conocido"), f:j("quien_lo_tiene_y_de_donde_sale"),
             d:j("donde_estamos"), m:j("misma_magnitud"), v:j("podemos_pasarlo")};
  const pasado = F.filter(f => f[I.v].startsWith("YA PASADO"));
  const podemos = F.filter(f => f[I.v].startsWith("SI"));
  const incomp = F.filter(f => f[I.m].startsWith("NO"));
  const cl = f => f[I.v].startsWith("YA PASADO") ? "bien"
                : (f[I.v].startsWith("SI") ? "ojo" : "mut");
  const et = f => f[I.v].startsWith("YA PASADO") ? "ya pasado"
                : (f[I.v].startsWith("SI") ? "se puede" : "sin respuesta");

  $("#techos").innerHTML = `
    <div class="marc">
      <div class="tar"><div class="k">${F.length}</div><div class="e">techos censados</div></div>
      <div class="tar"><div class="k bien">${pasado.length}</div><div class="e">ya pasados</div></div>
      <div class="tar"><div class="k ojo">${podemos.length}</div>
        <div class="e">con camino conocido para pasarlos</div></div>
      <div class="tar"><div class="k ${incomp.length?'mal':''}">${incomp.length}</div>
        <div class="e">donde las dos cifras <b>no son la misma magnitud</b></div></div>
    </div>
    <p class="nota"><b>La segunda regla nace de un error de hoy.</b> La fase 3 de vdeflate se
      comparó como «13,14% nuestro contra 1,70% de preflate» y de ahí salió un «no se
      construye». Pero el 13,14% era una <b>cota superior</b> de un modelo que cobra las
      correcciones a tarifa plana —8 bits por fallo, falle por 1 byte o por 200— y el 1,70% es el
      <b>resultado real</b> de preflate con esas correcciones codificadas por entropía
      adaptativa. Dos números correctos, ninguno comparable, y una decisión de no construir
      tomada encima. <b>Una fila que no puede jurar que compara lo mismo no da veredicto.</b></p>
    ${F.map(f=>`<div class="frag-t" style="margin-top:14px">
        <span><b>${esc(f[I.p])}</b> <span class="mut">· ${esc(f[I.q])}</span></span>
        <span class="${cl(f)}">${et(f)}</span></div>
      <div class="env-t" style="padding:11px 13px">
        <div style="font-size:12px"><b>el techo:</b> ${esc(f[I.t])}</div>
        <div class="mut" style="font-size:11px;margin-top:4px">${esc(f[I.f])}</div>
        <div style="font-size:11px;margin-top:8px"><b>dónde estamos:</b> ${esc(f[I.d])}</div>
        <div style="font-size:11px;margin-top:8px"
             class="${f[I.m].startsWith('NO')?'mal':'mut'}">
          <b>¿la misma magnitud?</b> ${esc(f[I.m])}</div>
        <div style="font-size:12px;margin-top:8px"><b>¿podemos pasarlo?</b> ${esc(f[I.v])}</div>
      </div>`).join("")}`;
});

/* ---- que carril se paga y a que precio ---- */
seguro(function(){
  const T = D.carriles || [];
  if (T.length < 2) { $("#carriles").innerHTML = '<p class="nota">Sin datos. Corre bench/carriles.sh.</p>'; return; }
  const cab = T[0], F = T.slice(1), j = n => cab.indexOf(n);
  const I = {c:j("carril"), cb:j("compra_bytes"), cp:j("compra_pct"),
             kc:j("cuesta_cpu_s"), kp:j("cuesta_pct"), m:j("medible")};
  // Las columnas de decision son las que llevan «_Nm»: precio y retencion.
  const DEC = cab.map((n,i)=>({n,i})).filter(x => /_\d+m$/.test(x.n));
  const num = v => parseFloat(v) || 0;
  const es = v => (Math.round(num(v))).toLocaleString("es-ES");
  const paga = f => DEC.filter(d => num(f[d.i]) > 0).length;
  $("#carriles").innerHTML = `
    <div class="marc">
      <div class="tar"><div class="k">${F.length}</div><div class="e">carriles medidos</div></div>
      <div class="tar"><div class="k">${F.filter(f=>f[I.m]==="si").length}</div>
        <div class="e">con coste por encima del ruido</div></div>
      <div class="tar"><div class="k">${F.filter(f=>f[I.m]!=="si").length}</div>
        <div class="e">sin medir: su coste no se distingue del ruido</div></div>
    </div>
    <p class="nota"><b>Un carril que no se paga no es un carril malo</b>: es un carril encendido
      para el dinero equivocado. Hoy <code>--price</code> no apaga ninguno — sondea para elegir
      el <i>nivel</i> de zstd, que entre el 17 y el 19 mueve un 0,8% del tamaño. Eso es D8.</p>
    <div style="overflow-x:auto"><table><thead><tr>
      <th>carril</th><th>compra</th><th>cuesta CPU</th><th>medible</th>
      ${DEC.map(d=>`<th style="text-align:right">${esc(d.n.replace("_"," "))}</th>`).join("")}
    </tr></thead><tbody>
    ${F.map(f => `<tr>
      <td><b>${esc(f[I.c])}</b></td>
      <td style="text-align:right">${esc(f[I.cp])}%</td>
      <td style="text-align:right">${esc(f[I.kc])}s <span class="mut">${esc(f[I.kp])}%</span></td>
      <td>${f[I.m]==="si" ? '<span class="chip bien">sí</span>'
                          : '<span class="chip ojo">no (ruido)</span>'}</td>
      ${DEC.map(d=>`<td style="text-align:right" class="${num(f[d.i])>0?'bien':'mal'}">${es(f[d.i])}</td>`).join("")}
    </tr>`).join("")}
    </tbody></table></div>
    <p class="nota">Cada columna es un <b>precio de almacenamiento</b> y una <b>retención</b>, en
      € por PB de entrada: lo que el carril ahorra de disco menos lo que cuesta de CPU. En rojo,
      los casos en que ese carril <b>cuesta más de lo que ahorra</b> y habría que apagarlo.
      Los carriles marcados «no (ruido)» no se deciden: su coste no se distingue del suelo de
      ruido de la máquina, y eso no es ser barato, es estar <b>sin medir</b>.</p>`;
});

/* ---- el censo de tecnicas: cinco formas de ir mas rapido ---- */
seguro(function(){
  const T = D.tecnicas || [];
  if (T.length < 2) { $("#tecnicas").innerHTML = '<p class="nota">Sin datos.</p>'; return; }
  const cab = T[0], F = T.slice(1), j = n => cab.indexOf(n);
  const I = {f:j("familia"), t:j("tecnica"), a:j("que_ataca"), e:j("estado"),
             m:j("lo_medido"), q:j("que_falta")};
  const cls = e => e.startsWith("HECHO") ? "bien"
                 : e.startsWith("SIGUIENTE") ? "ojo"
                 : e.startsWith("NO SE HACE") ? "mal" : "mut";
  const fams = [...new Set(F.map(f => f[I.f]))];
  const hechas = F.filter(f => f[I.e].startsWith("HECHO")).length;
  $("#tecnicas").innerHTML = `
    <div class="marc">
      <div class="tar"><div class="k">${F.length}</div><div class="e">técnicas en el censo</div></div>
      <div class="tar"><div class="k">${hechas}</div><div class="e">aplicadas y medidas</div></div>
      <div class="tar"><div class="k">${fams.length}</div><div class="e">familias, por lo que atacan</div></div>
    </div>
    <p class="nota"><b>El orden importa y es contraintuitivo.</b> «No hacerlo» y «no repetirlo»
      tienen techo casi infinito; «con menos instrucciones» y «a la vez» tienen el techo del
      hardware. Aquí se atacaron al revés —horas en el bucle de copia— y el hueco de la familia 1
      valía más que todo lo demás junto.</p>
    ${fams.map(fa => `
      <div class="frag-t" style="margin-top:16px"><span>${esc(fa)}</span>
        <span class="mut">${F.filter(f=>f[I.f]===fa).length}</span></div>
      ${F.filter(f=>f[I.f]===fa).map(f => `<div class="env-t" style="padding:10px 12px;margin-bottom:4px">
        <div style="display:flex;justify-content:space-between;gap:10px;align-items:baseline">
          <b style="font-size:12px">${esc(f[I.t])}</b>
          <span class="${cls(f[I.e])}" style="font-size:11px;white-space:nowrap">${esc(f[I.e])}</span></div>
        <div class="mut" style="font-size:11px;margin-top:5px">Ataca: ${esc(f[I.a])}</div>
        <div style="font-size:11px;margin-top:5px">${esc(f[I.m])}</div>
        ${f[I.q] && f[I.q] !== "—" ? `<div style="font-size:11px;margin-top:5px" class="ojo">Falta:
          <span style="color:var(--tx2)">${esc(f[I.q])}</span></div>` : ""}
      </div>`).join("")}`).join("")}`;
});

/* ---- la norma de precomputar ---- */
seguro(function(){
  const T = D.precomputar || [];
  if (T.length < 2) { $("#precomputar").innerHTML = '<p class="nota">Sin datos.</p>'; return; }
  const cab = T[0], F = T.slice(1), j = n => cab.indexOf(n);
  const I = {c:j("candidato"), d:j("donde"), r1:j("r1_dominio_pequeño"), r2:j("r2_se_repite"),
             r3:j("r3_cuesta_mas_que_un_acceso"), r4:j("r4_cuanto_mueve"), e:j("estado"),
             n:j("nota")};
  // El veredicto de una regla se lee de como EMPIEZA su celda, que es donde va el SI/NO.
  const veredicto = v => (v||"").startsWith("SI") ? "bien"
                       : (v||"").startsWith("NO") ? "mal"
                       : (v||"").startsWith("SIN") ? "ojo" : "mut";
  const marca = v => (v||"").split("·")[0].trim() || "—";
  const cls = e => e.startsWith("HECHO") ? "bien" : e === "EL LIMITE" ? "mal"
                 : e.startsWith("SIN") ? "ojo" : "mut";
  $("#precomputar").innerHTML = `
    <div class="marc">
      <div class="tar"><div class="k">3+1</div><div class="e">reglas: tres dicen si funciona, la cuarta si merece la pena</div></div>
      <div class="tar"><div class="k">${F.filter(f=>f[I.e].startsWith("HECHO")).length}</div><div class="e">aplicadas</div></div>
      <div class="tar"><div class="k">${F.filter(f=>f[I.e].includes("SIN MEDIR")).length}</div><div class="e">sin medir</div></div>
    </div>
    <p class="nota"><b>La norma.</b> Cambiar un cálculo por una consulta gana <b>sólo</b> si se
      cumplen las tres: <b>1</b> el dominio es pequeño y cabe en caché · <b>2</b> las entradas se
      repiten · <b>3</b> el cálculo cuesta <i>más</i> que un acceso a caché —ésta es la que
      decide—. Y una <b>cuarta</b> que las tres no contestan: <b>¿cuánto mueve?</b> Sin ella la
      norma se convierte en una excusa para tocar código que no molesta.</p>
    ${F.map(f => `<div class="frag-t" style="margin-top:14px">
        <span>${esc(f[I.c])} · <code style="font-size:11px">${esc(f[I.d])}</code></span>
        <span class="${cls(f[I.e])}">${esc(f[I.e])}</span></div>
      <div class="env-t" style="padding:10px 12px">
        <div style="display:flex;gap:6px;flex-wrap:wrap;margin-bottom:7px">
          ${[["1 dominio",I.r1],["2 se repite",I.r2],["3 cuesta más",I.r3]].map(([et,ix]) =>
            `<span class="chip ${veredicto(f[ix])}" title="${esc(f[ix])}">${et}: ${esc(marca(f[ix]))}</span>`).join("")}
        </div>
        <div style="font-size:11px"><b>¿Cuánto mueve?</b> ${esc(f[I.r4])}</div>
        <div class="mut" style="font-size:11px;margin-top:6px">${esc(f[I.n])}</div>
      </div>`).join("")}`;
});

/* ---- R3 + R4 + R10: el libro mayor agregado y auditado ---- */
seguro(function(){
  const M = D.mayor;
  if (!M || !M.grupos) { $("#mayor").innerHTML = '<p class="nota">Sin datos.</p>'; return; }
  const G = M.grupos, su = M.suelo || {};
  const pct = x => (100*x).toFixed(1).replace(".", ",") + "%";
  const n1 = (x, d=1) => x.toFixed(d).replace(".", ",");
  // La incertidumbre se pinta como lo que ES, no como una sigma para todo: con una vuelta se
  // dice «una vuelta» y no se dibuja dispersion ninguna.
  const disp = a => !a ? "—"
    : a.n === 1 ? '<span class="mut">n=1 · una vuelta, sin dispersión</span>'
    : `n=${a.n} · ±${n1(a.disp)} <span class="mut">(${a.clase})</span>`;
  // Un delta por debajo del suelo de ruido se pinta APAGADO, nunca en negrita.
  const ruidoso = a => a && a.n >= 2 && su.mediana != null && a.media > 0
                    && (a.disp / a.media) <= su.mediana;
  const audit = [
    ["sin comando escrito", M.sin_cmd, true],
    ["sin commit", M.sin_commit, true],
    ["medidas con el árbol sucio", M.sucias, false],
    ["medidas con la máquina a carga > 8", M.carga_alta, false],
    ["ejecuciones que fallaron", M.rc_malo, true],
  ];
  $("#mayor").innerHTML = `
    <div class="marc">
      <div class="tar"><div class="k">${M.filas}</div><div class="e">medidas en el libro mayor</div></div>
      <div class="tar"><div class="k">${G.length}</div><div class="e">experimentos distintos</div></div>
      <div class="tar"><div class="k">${M.n1}</div><div class="e">con una sola vuelta</div></div>
      <div class="tar"><div class="k ${su.mediana==null?'mut':''}">${su.mediana==null?'—':pct(su.mediana)}</div>
        <div class="e">suelo de ruido, leído de los datos</div></div>
    </div>
    <p class="nota"><b>De dónde sale el suelo de ruido (R10).</b> No se estima. Es la dispersión
      relativa mediana de los experimentos que cumplen las tres: repetidos <b>3 veces o más</b>,
      con <b>cargas que no difieren en más de 4</b>, y medidos con el <b>árbol limpio</b>.
      ${su.mediana==null ? `<b class="mal">Y ahora mismo no hay ninguno que las cumpla.</b>
        Ésa es la respuesta honesta y es incómoda: de ${M.filas} medidas, <b>${M.sucias}</b> se
        tomaron con el árbol sin commitear, así que el commit no identifica el código. No es
        teórico — las dos tandas del salto incremental, la rota de 148 s y la buena de 17,7 s,
        figuran las dos bajo <code>6ed723e</code>, y promediarlas daba un «suelo de ruido» del
        90,9% que no era ruido sino un cambio de código disfrazado de repetición. Para tener
        esta cifra hay que repetir una configuración tres veces <i>después</i> de commitear.`
       : `Eso es lo que mete la máquina de su parte cuando no cambia nada, así que un delta por
        debajo no es una mejora: es la máquina respirando. El peor caso fue ${pct(su.peor)}, de
        ${su.n} experimentos.`}
      Estimarlo a ojo ya salió mal una vez: ±0,62% publicado contra ±1,3% real, con seis
      controles de cero delante sin reconocerlos.</p>
    <p class="nota"><b>Reproducibilidad (R4).</b> No se comprueba que el número vuelva a salir
      —eso sería volver a medirlo—: se comprueba que esté <b>escrito</b> cómo repetirlo, que es
      justo lo que se pierde a los tres meses.</p>
    <p class="nota"><b>Cada herramienta con sus datos aparte.</b> No es burocracia: una cifra que
      cruza de herramienta decide sobre lo que no midió. Pasó — <code>--price</code> estimaba la
      CPU con una tabla que mide <b>el códec solo</b> para decidir sobre <b>el archivador
      entero</b>; los dos números eran correctos (34,8× de diferencia entre niveles para uno,
      1,79× para el otro) y ninguno valía para lo del otro.</p>
    <div class="marc">${(M.herramientas||[]).map(h => `
      <div class="tar"><div class="k ${h.nombre==="sin clasificar"?"mal":""}">${h.filas}</div>
      <div class="e">${esc(h.nombre)}${h.declaradas < h.filas
        ? ` · <span class="mut">${h.filas-h.declaradas} deducidas</span>` : ""}</div></div>`).join("")}</div>
    <p class="nota"><b>Deducidas</b> quiere decir que la fila es anterior a la norma y su
      herramienta se saca del comando al leer. El libro mayor es <b>append-only</b>: no se
      reescribe el pasado para que cuadre, se dice qué parte está deducida.</p>
    <div class="marc">${audit.map(([et,v,malo]) => `
      <div class="tar"><div class="k ${v===0?'bien':(malo?'mal':'ojo')}">${v}</div>
      <div class="e">${et}</div></div>`).join("")}</div>
    <table><thead><tr><th>experimento</th><th>muestra</th><th>segundos</th>
      <th>incertidumbre</th><th>carga</th><th>árbol</th></tr></thead><tbody>
      ${G.slice(0, 60).map(g => `<tr>
        <td>${esc(g.etiqueta)}${g.config?` <span class="mut">${esc(g.config)}</span>`:""}</td>
        <td class="mut">${esc(g.muestra)}</td>
        <td style="text-align:right" class="${ruidoso(g.seg)?'mut':''}">${g.seg?n1(g.seg.media):"—"}</td>
        <td style="font-size:11px">${disp(g.seg)}</td>
        <td style="text-align:right" class="${g.carga_max-g.carga_min>4?'mal':'mut'}">${n1(g.carga_min)}–${n1(g.carga_max)}</td>
        <td>${g.sucio?'<span class="chip mal">sucio</span>':'<span class="mut">limpio</span>'}</td>
      </tr>`).join("")}</tbody></table>
    ${G.length>60?`<p class="nota">Se enseñan los 60 experimentos con más vueltas, de ${G.length}.
      Los demás están en <i>Todas las medidas</i>: no se esconde ninguno, se ordenan.</p>`:""}
    <p class="nota"><b>La columna de carga es la que más engaña.</b> Dos vueltas del mismo
      experimento con cargas que difieren en más de 4 no son comparables, y van en rojo: la misma
      prueba tardó 562,8 s a carga 6,7 y 651,7 s a carga 37,8 sin que cambiara una línea.</p>`;
});

/* ---- las piezas: lo de fuera y lo nuestro ---- */
seguro(function(){
  const T = D.terceros || [];
  if (T.length < 2) { $("#piezas").innerHTML = '<p class="nota">Sin datos.</p>'; return; }
  const cab = T[0], F = T.slice(1);
  const j = n => cab.indexOf(n);
  const I = {f:j("de_fuera"), q:j("que_hacia"), n:j("la_nuestra"), e:j("estado"),
             r:j("resultado"), b:j("banco")};
  const CLS = {"REEMPLAZADA":"bien","EN OBRA":"ojo","CONVIVEN":"","SE QUEDA":"mut","A AISLAR":"mal"};
  const ORDEN = ["REEMPLAZADA","EN OBRA","CONVIVEN","A AISLAR","SE QUEDA"];
  const cta = e => F.filter(f => f[I.e] === e).length;

  // A que vista salta cada pieza nuestra. Las que NO tienen vista propia todavia lo dicen: un
  // enlace que no lleva a ningun sitio es peor que no tenerlo.
  const VISTAS = {
    "vc-cm":       [["su pirámide", "#pirvccm"]],
    "vc-cdc":      [],
    "vc-formato":  [],
    "vc-jpeg":     [],
    "vc-inflate":  [],
  };
  const enlaces = nom => {
    const k = Object.keys(VISTAS).find(x => nom.includes(x));
    if (!k) return "";
    const v = VISTAS[k];
    if (!v.length) return '<span class="mut" style="font-size:11px">todavía no tiene pirámide propia</span>';
    return v.map(([t,h]) => `<a href="${h}" style="font-size:11px">${t} →</a>`).join(" · ")
         + ' · <a href="#v-aguja" style="font-size:11px">la aguja →</a>';
  };

  const grupo = e => {
    const g = F.filter(f => f[I.e] === e);
    if (!g.length) return "";
    return `<div class="frag-t" style="margin-top:18px"><span>${esc(e)}</span>
      <span>${g.length} pieza${g.length===1?"":"s"}</span></div>
      <div class="env-t"><table><tbody>${g.map(f => `<tr>
        <td style="vertical-align:top;width:210px"><b>${esc(f[I.f])}</b>
          <div class="mut" style="font-size:11px;margin-top:2px">${esc(f[I.q])}</div></td>
        <td style="vertical-align:top;width:160px">${f[I.n] === "—" ? '<span class="mut">—</span>'
          : `<code>${esc(f[I.n])}</code><div style="margin-top:3px">${enlaces(f[I.n])}</div>`}</td>
        <td style="vertical-align:top">${esc(f[I.r])}
          <div class="mut" style="font-size:11px;margin-top:3px">${esc(f[I.b])}</div></td></tr>`).join("")}
      </tbody></table></div>`;
  };

  $("#piezas").innerHTML = `
    <div class="marc">
      <div class="tar"><div class="k">${cta("REEMPLAZADA")}</div><div class="e">ya reemplazadas por lo nuestro</div></div>
      <div class="tar"><div class="k">${cta("EN OBRA")}</div><div class="e">en obra</div></div>
      <div class="tar"><div class="k">${cta("SE QUEDA")+cta("CONVIVEN")+cta("A AISLAR")}</div><div class="e">siguen siendo de fuera</div></div>
      <div class="tar"><div class="k">${F.length}</div><div class="e">piezas de terceros en total</div></div>
    </div>
    <p class="nota"><b>Las que ya no están son la parte importante.</b> La tabla de dependencias
      dice de qué dependemos <em>hoy</em>; ésta dice además de qué dependíamos y dejamos de
      depender, y con qué resultado. Sin esa columna, «preguntarse siempre si podemos hacerlo
      nosotros» parece una intención. Con ella es un historial —y en dos de las seis sustituidas
      lo nuestro <b>gana con números</b>, no empata.</p>
    <p class="nota"><b>Y las que se quedan, se quedan por un motivo dicho.</b> blake3 y el cifrado
      son las únicas donde hacerlo nosotros sería <em>medible­mente peor</em>: en criptografía el
      fallo no se ve en las pruebas —nuestra ida y vuelta pasaría igual con un cifrado roto—.
      Eso no es dogma, es la razón concreta.</p>
    ${ORDEN.map(grupo).join("")}`;
});

/* ---- nuestras herramientas ---- */
seguro(function(){
  const H = D.herramientas || [];
  if (H.length < 2) { $("#herr").innerHTML = '<p class="nota">Sin fichas.</p>'; return; }
  const cab = H[0], F = H.slice(1);
  const j = n => cab.indexOf(n);
  const I = {id:j("id"), nom:j("nombre"), que:j("que_hace"), con:j("contra_quien"),
             est:j("estado"), tam:j("tamano"), med:j("lo_medido"), ban:j("banco"),
             fal:j("lo_que_falta")};
  const cls = e => e.includes("gana") ? "bien" : e.includes("REHACER") ? "mal"
             : e.includes("OBRA") || e.includes("NACIDA") ? "ojo" : "";
  $("#herr").innerHTML = `
    <div class="marc">
      <div class="tar"><div class="k">${F.length}</div><div class="e">piezas nuestras con banco propio</div></div>
      <div class="tar"><div class="k">${F.filter(f=>f[I.est].includes("USAMOS")).length}</div>
        <div class="e">ya en producción</div></div>
      <div class="tar"><div class="k">${F.filter(f=>f[I.est].includes("OBRA")||f[I.est].includes("NACIDA")).length}</div>
        <div class="e">en construcción</div></div>
    </div>
    <p class="nota"><b>La regla de las fichas:</b> una herramienta sin banco propio no tiene ficha.
      Si no hay contra qué medirla, no es una herramienta nuestra — es código nuestro, que es otra cosa.</p>
    ${F.map(f => `<div class="frag-t" style="margin-top:18px">
        <span><code>${esc(f[I.id])}</code> · ${esc(f[I.nom])}</span>
        <span><span class="chip ${cls(f[I.est])}">${esc(f[I.est])}</span></span></div>
      <div style="padding:10px 4px 4px">
        <p style="margin:0 0 8px">${esc(f[I.que])}</p>
        <table style="width:100%"><tbody>
          <tr><td class="mut" style="width:130px;vertical-align:top">sustituye a</td>
              <td><b>${esc(f[I.con])}</b></td></tr>
          <tr><td class="mut" style="vertical-align:top">lo medido</td>
              <td>${esc(f[I.med])}</td></tr>
          <tr><td class="mut" style="vertical-align:top">su banco</td>
              <td class="mut">${esc(f[I.ban])}</td></tr>
          <tr><td class="mut" style="vertical-align:top">lo que falta</td>
              <td>${esc(f[I.fal])}</td></tr>
        </tbody></table></div>`).join("")}`;
});

/* ---- la piramide de vc-cm ---- */
seguro(function(){
  const P = D.pir_vccm || [];
  if (P.length < 2) { $("#pirvccm").innerHTML = '<p class="nota">Sin datos.</p>'; return; }
  const cab = P[0], F = P.slice(1);
  const j = n => cab.indexOf(n);
  const I = {niv:j("nivel"), id:j("id"), pad:j("padre"), nom:j("nombre"),
             pct:j("pct_tiempo"), fue:j("fuente"), nota:j("nota")};
  const hijos = {};
  F.forEach(f => (hijos[f[I.pad]] = hijos[f[I.pad]] || []).push(f));
  const barra = p => `<div style="background:var(--linea);height:7px;border-radius:4px;overflow:hidden;min-width:70px">
      <div style="width:${Math.min(100,p).toFixed(1)}%;height:100%;background:var(--mal)"></div></div>`;
  const fila = (f, prof) => {
    const p = +f[I.pct] || 0;
    return `<tr>
      <td style="padding-left:${8 + prof*18}px">${prof ? '<span class="mut">└ </span>' : ""}<b>${esc(f[I.nom])}</b>
        ${f[I.nota] ? `<div class="mut" style="font-size:11px;margin-top:3px;max-width:70ch">${esc(f[I.nota])}</div>` : ""}</td>
      <td class="n" style="white-space:nowrap">${p ? p.toFixed(1)+"%" : ""}</td>
      <td style="width:100px">${p ? barra(p) : ""}</td>
      <td class="n mut" style="font-size:11px;white-space:nowrap">${esc(f[I.fue])}</td></tr>`;
  };
  const pinta = (id, prof) => (hijos[id] || [])
    .sort((a,b) => (+b[I.pct]||0) - (+a[I.pct]||0))
    .map(f => fila(f, prof) + pinta(f[I.id], prof+1)).join("");
  const pred = F.find(f => f[I.id] === "predecir"), act = F.find(f => f[I.id] === "actualizar");
  const mez = F.find(f => f[I.id] === "mezclar");
  const tablas = (pred && act) ? (+pred[I.pct] + +act[I.pct]) : 0;
  $("#pirvccm").innerHTML = `
    ${tablas ? `<div class="marc">
      <div class="tar"><div class="k">${tablas.toFixed(0)}%</div>
        <div class="e">buscar y escribir en las tablas</div></div>
      <div class="tar"><div class="k">${mez ? (+mez[I.pct]).toFixed(1) : "?"}%</div>
        <div class="e">el mezclador — lo que lo hace «inteligente»</div></div>
      <div class="tar"><div class="k">484</div><div class="e">MiB de tablas</div></div>
    </div>
    <p class="nota"><b>No está limitado por cálculo: está limitado por memoria.</b> El
      ${tablas.toFixed(0)}% del tiempo es buscar y escribir en 484 MiB de tablas, donde casi
      cada acceso falla la caché. El mezclador —la parte que combina las opiniones de todos los
      modelos, que es lo que hace listo a un compresor de contextos— es el
      ${mez ? (+mez[I.pct]).toFixed(1) : "?"}%. <b>Rehacerlo por el lado del cálculo no compraría
      nada; por el lado de la memoria, casi todo.</b></p>` : ""}
    <div class="env-t"><table><thead><tr><th>peldaño</th><th class="n">% del tiempo</th>
      <th></th><th class="n">de dónde sale</th></tr></thead>
      <tbody>${pinta("", 0)}</tbody></table></div>`;
});

/* ---- la piramide ---- */
seguro(function(){
  const P = D.piramide || [];
  if (P.length < 2) { $("#piramide").innerHTML = '<p class="nota">Sin datos todavía.</p>'; return; }
  const cab = P[0], F = P.slice(1);
  const j = n => cab.indexOf(n);
  const I = {niv:j("nivel"), id:j("id"), pad:j("padre"), nom:j("nombre"), rel:j("reloj_s"),
             cpu:j("cpu_s"), modo:j("modo"), vec:j("veces"), us:j("us_por_vez"),
             com:j("compra_pct"), nota:j("nota")};
  const raiz = F.find(f => f[I.pad] === "");
  const RELOJ = +raiz[I.rel], CPU = +raiz[I.cpu];
  const hijos = {};
  F.forEach(f => (hijos[f[I.pad]] = hijos[f[I.pad]] || []).push(f));

  // La barra es SOBRE EL TOTAL DEL PROGRAMA en las dos columnas, no sobre el padre: asi dos
  // peldaños de ramas distintas se pueden comparar de un vistazo, que es para lo que sirve.
  const barra = (v, tot, cls) => {
    const p = tot > 0 ? Math.min(100, 100 * v / tot) : 0;
    return `<div style="background:var(--linea);height:7px;border-radius:4px;overflow:hidden;min-width:60px">
      <div style="width:${p.toFixed(1)}%;height:100%;background:var(--${cls})"></div></div>`;
  };
  const chip = m => m === "serie" ? '<span class="chip mal">serie</span>'
                  : m === "paralelo" ? '<span class="chip bien">paralelo</span>' : "";

  const fila = (f, prof) => {
    const rel = +f[I.rel], cpu = +f[I.cpu], vec = +f[I.vec] || 0;
    const sang = prof * 18;
    return `<tr>
      <td class="izq" style="padding-left:${8 + sang}px">${prof ? '<span class="mut">└ </span>' : ""}<b>${esc(f[I.nom])}</b>
        ${f[I.nota] ? `<div class="mut" style="font-size:11px;margin-top:3px;max-width:56ch">${esc(f[I.nota])}</div>` : ""}</td>
      <td class="n" style="white-space:nowrap">${rel ? rel.toFixed(2)+" s" : "—"}</td>
      <td style="width:90px">${rel ? barra(rel, RELOJ, "mal") : ""}</td>
      <td class="n" style="white-space:nowrap">${cpu ? cpu.toFixed(2)+" s" : "—"}</td>
      <td style="width:90px">${cpu ? barra(cpu, CPU, "bien") : ""}</td>
      <td class="n">${chip(f[I.modo])}</td>
      <td class="n">${vec ? vec.toLocaleString("es") : ""}</td>
      <td class="n mut">${f[I.us] && vec ? (+f[I.us] > 1000 ? (+f[I.us]/1000).toFixed(1)+" ms" : (+f[I.us]).toFixed(1)+" µs") : ""}</td>
      <td class="n">${f[I.com] ? "<b>+"+(+f[I.com]).toFixed(3)+"%</b>" : ""}</td></tr>`;
  };
  const pinta = (id, prof) => (hijos[id] || [])
    .sort((a,b) => +b[I.cpu] - +a[I.cpu])
    .map(f => fila(f, prof) + pinta(f[I.id], prof + 1)).join("");

  const serie = F.find(f => f[I.id] === "serie"), par = F.find(f => f[I.id] === "paralelo");
  $("#piramide").innerHTML = `
    <div class="marc">
      <div class="tar"><div class="k">${RELOJ.toFixed(0)}s</div><div class="e">de reloj — lo que se espera</div></div>
      <div class="tar"><div class="k">${CPU.toFixed(0)}s</div><div class="e">de CPU — el trabajo hecho</div></div>
      <div class="tar"><div class="k">${(CPU/RELOJ).toFixed(1)}</div><div class="e">núcleos ocupados de media</div></div>
      ${serie ? `<div class="tar"><div class="k">${(100*(+serie[I.rel])/RELOJ).toFixed(0)}%</div>
        <div class="e">del reloj va EN SERIE</div></div>` : ""}
    </div>
    <p class="nota"><b>Las dos columnas dicen cosas distintas y hay que leer las dos.</b>
      ${par ? `<code>${esc(par[I.nom])}</code> se lleva <b>${(+par[I.cpu]).toFixed(0)} s de CPU</b>
      —el ${(100*(+par[I.cpu])/CPU).toFixed(0)}% del trabajo— y sólo cuesta
      <b>~${(+par[I.rel]).toFixed(0)} s de reloj</b>, porque se reparte entre hilos.` : ""}
      ${serie ? ` <code>${esc(serie[I.nom])}</code> se lleva ${(100*(+serie[I.cpu])/CPU).toFixed(0)}%
      del trabajo y <b>${(+serie[I.rel]).toFixed(0)} s de reloj enteros</b>, porque no se solapa con nada.
      <b>Un segundo ahorrado ahí es un segundo menos de espera; en el paralelo, no.</b>` : ""}</p>
    <div class="env-t"><table><thead><tr>
      <th>peldaño</th><th class="n">reloj</th><th></th><th class="n">CPU</th><th></th>
      <th class="n">modo</th><th class="n">veces</th><th class="n">por vez</th>
      <th class="n">compra</th></tr></thead>
      <tbody>${pinta("", 0)}</tbody></table></div>
    <p class="nota" style="margin-top:12px">«compra» es cuánto <b>crece</b> el archivo si se apaga
      esa pieza: lo que aportaba. Sale de la ablación, de apagarla y medir. Un peldaño que se lleva
      mucho reloj y compra poco es el candidato a desaparecer.</p>`;
});

/* ---- los bucles ---- */
seguro(function(){
  const B = D.bucles || [];
  if (B.length < 2) { $("#bucles").innerHTML = '<p class="nota">Sin datos.</p>'; return; }
  const cab = B[0], F = B.slice(1);
  const j = n => cab.indexOf(n);
  const I = {n:j("n"), b:j("bucle"), d:j("donde"), r:j("recorre"),
             q:j("que_hace_cada_vuelta"), c:j("complejidad"), v:j("veredicto"), nt:j("nota")};
  const cls = v => v === "bien" ? "" : v === "SUELO" ? "bien"
             : v.startsWith("a MEDIR") ? "ojo" : "mal";
  const primero = F.find(f => f[I.v].startsWith("CUESTA"));
  const gordo = F.find(f => f[I.v].startsWith("AQUI"));
  $("#bucles").innerHTML = `
    <p class="nota"><b>Un bucle no es malo por ser O(F).</b> Es malo cuando hace en cada vuelta algo
      que no hace falta, o cuando su coste pasa de O(ficheros) a O(bytes) sin necesidad. Los marcados
      <span class="chip bien">SUELO</span> son el mínimo inevitable: andar el árbol pide un stat por
      entrada, y trocear pide recorrer todos los bytes. Ahí es donde el tiempo <em>debería</em> irse.</p>
    ${primero ? `<p class="nota mal"><b>El primero que cuesta de más:</b> ${esc(primero[I.b])}.</p>` : ""}
    ${gordo ? `<p class="nota mal"><b>Donde empieza la lentitud de verdad:</b> ${esc(gordo[I.b])} —
      el primer bucle cuyo coste es O(bytes) y no O(ficheros).</p>` : ""}
    <div class="env-t"><table><thead><tr><th class="n">#</th><th>bucle</th><th>recorre</th>
      <th>cada vuelta</th><th>complejidad</th><th>veredicto</th></tr></thead>
      <tbody>${F.map(f => `<tr>
        <td class="n mut">${esc(f[I.n])}</td>
        <td><b>${esc(f[I.b])}</b><div class="mut" style="font-size:11px"><code>${esc(f[I.d])}</code></div>
          ${f[I.nt] ? `<div class="mut" style="font-size:11px;margin-top:4px;max-width:64ch">${esc(f[I.nt])}</div>` : ""}</td>
        <td class="n">${esc(f[I.r])}</td>
        <td class="t" style="font-size:12px;max-width:34ch">${esc(f[I.q])}</td>
        <td class="n" style="white-space:nowrap"><code>${esc(f[I.c])}</code></td>
        <td class="n"><span class="chip ${cls(f[I.v])}">${esc(f[I.v])}</span></td></tr>`).join("")}
      </tbody></table></div>`;
});

/* ---- comparativa completa ---- */
seguro(function(){
  const C = D.comparativa || [];
  if (C.length < 2) { $("#comparativa").innerHTML = '<p class="nota">Sin datos.</p>'; return; }
  const cab = C[0], F = C.slice(1);
  const Q = D.quien_es || {};
  const ETIQ = {producto:'<span class="chip bien">producto</span>',
                "a medias":'<span class="chip ojo">a medias</span>',
                "no es producto":'<span class="chip mal">no es producto</span>',
                nosotros:'<span class="chip">nosotros</span>'};
  const iNos = cab.indexOf("nuestro_max"), iDef = cab.indexOf("nuestro_defecto"), iOrig = 1;

  // Cuantas veces ganamos, y a quien. Se cuenta aqui y no se afirma de memoria.
  // DOS CUENTAS SEPARADAS, y la razon esta en el comentario del .tsv: paq8px no es un producto.
  // Meterlo en el mismo saco que WinRAR daba «ganamos 4 de 9», que se lee como perder mas de la
  // mitad, cuando la verdad es que se gana a todos los productos y se pierde contra una cosa que
  // va a 0,008 MB/s y no se puede instalar.
  let gana = 0, total = 0, pierde = [], contraPaq = 0, totalPaq = 0;
  F.forEach(f => {
    const n = +f[iNos]; if (!n) return;
    let mejorRival = Infinity, quien = "";
    cab.forEach((c,i) => {
      if (i<=iOrig || c.startsWith("nuestro") || Q[c]==="no es producto") return;
      const v = +f[i]; if (v>0 && v<mejorRival) { mejorRival=v; quien=c; }
    });
    cab.forEach((c,i) => {
      if (Q[c]!=="no es producto") return;
      const v = +f[i]; if (v>0) { totalPaq++; if (n<v) contraPaq++; }
    });
    if (mejorRival===Infinity) return;
    total++;
    if (n < mejorRival) gana++; else pierde.push(`${f[0]} (${quien})`);
  });

  const th = cab.map((c,i)=>{
    const q = Q[c];
    return `<th class="${i?"n":""}">${esc(c)}${q?`<br><span style="font-weight:400;font-size:10px">${
      ETIQ[q]||""}</span>`:""}</th>`;
  }).join("");

  $("#comparativa").innerHTML = `
    <div class="marc">
      <div class="tar"><div class="k">${gana}/${total}</div><div class="e">ganamos al mejor PRODUCTO rival</div></div>
      <div class="tar"><div class="k">${contraPaq}/${totalPaq}</div><div class="e">contra paq8px, que no es un producto</div></div>
      <div class="tar"><div class="k">39.8%</div><div class="e">margen mayor sobre 7-Zip (PNG)</div></div>
    </div>
    <p class="nota"><b>Producto o no producto cambia la lectura.</b> zip, WinRAR, 7-Zip, xz, zstd y
      brotli están instalados en cientos de millones de máquinas y tienen soporte. zpaq es un
      archivador real pero de nicho y sin mantenimiento desde 2021. <b>paq8px no es un producto</b>:
      va a 0,008 MB/s y existe para ganar concursos de compresión.
      <b>Y nosotros tampoco lo somos todavía</b>: sin instalador, sin documentación y sin soporte.
      Decir «le ganamos a todos menos a paq8px» sin esa aclaración sería vender humo.</p>
    ${pierde.length ? `<p class="nota mal">Donde NO ganamos al mejor producto rival: ${pierde.map(esc).join(", ")}.</p>`
      : `<p class="nota"><b>No hay ninguna entrada donde un producto nos gane.</b> paq8px sí nos gana
         en ${totalPaq-contraPaq} de ${totalPaq}, y por eso se cuenta aparte: 0,008 MB/s, sin
         instalador y sin soporte. Es una referencia de laboratorio, no un rival comercial.</p>`}
    <div class="env-t"><table><thead><tr>${th}</tr></thead>
      <tbody>${F.map(f=>{
        const n = +f[iNos];
        return `<tr>${f.map((c,i)=>{
          const v = +c;
          const nos = cab[i] && cab[i].startsWith("nuestro");
          const mejor = i>iOrig && v>0 && n>0 && v<n && !nos;
          return `<td class="${i?"n":""} ${nos?"bien":""} ${mejor?"mal":""}">${
            i&&!isNaN(v)&&c!==""&&c!=="-"?fm(v):esc(c)}</td>`;
        }).join("")}</tr>`;
      }).join("")}</tbody></table></div>
    ${(D.sin_clasificar||[]).length ? `<p class="nota mal"><b>Sin clasificar:</b> ${
      D.sin_clasificar.map(esc).join(", ")}. Cuentan como producto en el marcador de arriba y puede
      que no lo sean.</p>` : ""}
    ${D.huecos ? `<p class="nota"><b>${D.huecos} celdas vacías</b>, y se quedan vacías. Son
      combinaciones de rival y entrada que nunca se midieron; rellenarlas por interpolación es
      exactamente lo que hace que una comparativa deje de valer.</p>` : ""}
    <p class="nota" style="margin-top:12px">En verde lo nuestro; en rojo, una celda donde un rival
      saca menos bytes que nuestro <code>--max</code>. Sólo hay tamaños a propósito: la máquina
      tenía trabajo de otros clientes y los relojes habrían salido falseados. Los bytes no dependen
      de la carga.</p>`;
});

/* ---- el dinero ---- */
seguro(function(){
  const E = D.economia || [];
  const txt = D.econivel || "";
  const aviso = `<p class="nota mal" style="border-left:3px solid var(--mal);padding-left:10px">
    <b>Estas cifras están pendientes de rehacerse y se dice antes de enseñarlas.</b> El coste de CPU
    de la tabla de abajo sale de las velocidades de <code>bench/nivel-zstd.tsv</code>, que se
    midieron sobre <b>una</b> muestra de 421 MB. El barrido de hoy sobre un árbol de 2,7 GB da
    <b>5,60 MB/s reales al nivel 1</b> contra los 473 que dice esa tabla. Si la velocidad real
    apenas cambia con el nivel, el coste de CPU deja de separar unos niveles de otros y
    <b>la recomendación puede darse la vuelta</b>. El ahorro en disco —las columnas de ratio— no
    depende de esto y sigue valiendo.</p>`;
  const tab = E.length>1 ? `<div class="env-t"><table><thead><tr>${
      E[0].map((c,i)=>`<th class="${i?"n":""}">${esc(c)}</th>`).join("")}</tr></thead>
      <tbody>${E.slice(1).map(f=>`<tr>${f.map((c,i)=>
        `<td class="${i?"n":""}">${i&&!isNaN(+c)&&c!==""?fm(+c):esc(c)}</td>`).join("")}</tr>`).join("")}
      </tbody></table></div>` : "";
  $("#dinero").innerHTML = `
    <p class="nota"><b>El ajuste que conviene depende del precio del almacenamiento</b>, y por eso
      la respuesta correcta no era un número sino una perilla: <code>--price</code> sondea tu dato,
      pone precio a cada ajuste y usa el más barato.</p>
    ${tab}${aviso}
    ${txt ? `<pre class="crudo" style="overflow-x:auto;font-size:11px;line-height:1.45">${esc(txt)}</pre>` : ""}`;
});

/* ---- velocidad real ---- */
seguro(function(){
  const V = D.velreal || [];
  const vacio = `<p class="nota"><b>Vacía todavía, y se dice por qué.</b> El barrido de niveles
    sobre el árbol de 2,7 GB va <b>en serie</b> —son medidas de tiempo y paralelizarlas las
    falsea— y tarda lo suyo. La vista está montada y se llena sola con
    <code>bench/velocidad-real.tsv</code>.</p>
    <p class="nota">Lo que se busca: la tabla <code>VELOCIDAD_NIVEL</code> dice que el nivel 1 va a
    <b>473 MB/s</b>. Eso es la velocidad del <b>códec</b> zstd sobre una muestra de 421 MB. El
    archivador además recorre, hace stat, enruta por firma, lee, trocea, hace blake3 y deduplica,
    y todo eso va en el <b>hilo principal</b>. Si el productor es el techo, la velocidad no sube al
    bajar el nivel — y entonces <code>--price</code> está cobrando la CPU con un número que no es.</p>`;
  if (V.length < 2) { $("#velreal").innerHTML = vacio; return; }
  const cab = V[0], F = V.slice(1);
  const j = n => cab.indexOf(n);
  const iM = j("MB_s"), iT = j("tabla_dice"), iS = j("seg"), iB = j("bytes_out"), iC = j("config");
  const peor = F.reduce((m,f) => Math.max(m, (+f[iT]||0)/Math.max(+f[iM]||1e-9,1e-9)), 0);
  $("#velreal").innerHTML = `
    <div class="marc">
      <div class="tar"><div class="k">${peor.toFixed(0)}×</div><div class="e">lo más que se equivoca la tabla</div></div>
      <div class="tar"><div class="k">${(+F[0][iM]).toFixed(1)}</div><div class="e">MB/s reales, nivel más bajo</div></div>
      <div class="tar"><div class="k">${(+F[F.length-1][iM]).toFixed(1)}</div><div class="e">MB/s reales, nivel más alto</div></div>
    </div>
    <p class="nota">Si estas dos últimas cifras se parecen, <b>el nivel no manda</b>: manda el hilo
      principal, y bajar el nivel no compra velocidad, sólo pierde compresión.</p>
    <div class="env-t"><table><thead><tr>${cab.map((c,i)=>`<th class="${i?"n":""}">${esc(c)}</th>`).join("")}</tr></thead>
      <tbody>${F.map(f=>{
        const err = (+f[iT]||0)/Math.max(+f[iM]||1e-9,1e-9);
        return `<tr>${f.map((c,i)=>`<td class="${i?"n":""} ${i===iT&&err>2?"mal":""}">${
          i&&!isNaN(+c)&&c!==""?fm(+c):esc(c)}</td>`).join("")}
          <td class="n ${err>2?"mal":""}"><b>${err>1?err.toFixed(0)+"×":"—"}</b></td></tr>`;
      }).join("")}</tbody></table></div>
    <p class="nota" style="margin-top:12px">La última columna es cuánto se equivoca la tabla en ese
      nivel. Sale de dividir lo que dice entre lo que se ha medido, sobre el mismo árbol y la misma
      máquina.</p>`;
});

/* ---- agujeros: el registro de preguntas ---- */
seguro(function(){
  const P = D.preguntas || [];
  if (P.length < 2) return;
  const cab = P[0], F = P.slice(1);
  const ix = n => cab.indexOf(n);
  const I = {id:ix("id"), eje:ix("eje"), est:ix("estado"), q:ix("pregunta"),
             r:ix("respuesta"), num:ix("numero"), uni:ix("unidad"), de:ix("de"), pr:ix("prueba")};
  const ORDEN = ["abierta","parcial","respondida","descartada"];
  const NOM = {abierta:"abierta", parcial:"a medias", respondida:"respondida", descartada:"descartada"};
  // «a medias» ya lleva la s: el plural no es añadir una siempre.
  const PLU = {abierta:"abiertas", parcial:"a medias", respondida:"respondidas", descartada:"descartadas"};
  const CLS = {abierta:"mal", parcial:"ojo", respondida:"bien", descartada:"mut"};
  const cta = e => F.filter(f => f[I.est]===e).length;

  // Los contadores primero y en grande: el numero que importa es el de ABIERTAS.
  const marc = ORDEN.map(e => `<div class="tar"><div class="k">${cta(e)}</div>
      <div class="e">${cta(e)===1 ? NOM[e] : PLU[e]}</div></div>`).join("");

  const fila = f => {
    const est = f[I.est];
    const cif = f[I.num] ? `<b>${esc(f[I.num])}</b>${f[I.uni]?' <span class="mut">'+esc(f[I.uni])+'</span>':""}` : "";
    const de  = f[I.de] ? `<div class="mut" style="font-size:11px;margin-top:3px">medido en <code>${esc(f[I.de])}</code></div>` : "";
    // La prueba que la cerraria es LO IMPORTANTE de una pregunta abierta, asi que va destacada.
    const pr  = f[I.pr] ? `<div style="margin-top:5px;font-size:12px"><span class="mut">la cerraría:</span> ${esc(f[I.pr])}</div>` : "";
    return `<tr data-est="${esc(est)}">
      <td style="white-space:nowrap;vertical-align:top"><span class="chip ${CLS[est]||"mut"}">${NOM[est]||esc(est)}</span>
        <div class="mut" style="font-size:11px;margin-top:4px">${esc(f[I.id])}</div></td>
      <td class="izq" style="vertical-align:top"><b>${esc(f[I.q])}</b>
        <div style="margin-top:4px">${esc(f[I.r])}</div>${de}${pr}</td>
      <td class="n" style="vertical-align:top;white-space:nowrap">${cif}</td></tr>`;
  };

  const ejes = Object.keys(D.ejes||{});
  const cuerpo = ejes.map(e => {
    const g = F.filter(f => f[I.eje]===e)
               .sort((a,b) => ORDEN.indexOf(a[I.est]) - ORDEN.indexOf(b[I.est]));
    if (!g.length) return "";
    const ab = g.filter(f=>f[I.est]==="abierta").length;
    return `<div class="frag-t" style="margin-top:18px"><span>${esc(e)} · ${esc(D.ejes[e])}</span>
      <span>${g.length} preguntas${ab?` · <b>${ab} abierta${ab===1?"":"s"}</b>`:""}</span></div>
      <div class="env-t"><table><tbody>${g.map(fila).join("")}</tbody></table></div>`;
  }).join("");

  // Auto-auditoria del propio banco.
  const A = D.auditoria || {dicho:[], hay:[], ninguno:[]};
  const tot = A.dicho.length + A.hay.length + A.ninguno.length;
  const aud = `<p class="nota" style="margin-top:20px"><b>Y el propio panel, ¿es reproducible?</b>
    De ${tot} tablas del banco: <b>${A.dicho.length}</b> dicen en su cabecera qué guion las
    reproduce; <b>${A.hay.length}</b> tienen guion pero no lo dicen —hay que ir a buscarlo—; y
    <b>${A.ninguno.length}</b> no tienen ninguno, así que esa cifra vive de la memoria de quien la
    midió. Los tres son problemas distintos y meterlos en un número los escondería.
    ${A.ninguno.length ? `<br><br><span class="mut" style="font-size:11px"><b>sin guion ninguno:</b> ${
      A.ninguno.map(x=>`<code>${esc(x)}</code>`).join(" ")}</span>` : ""}
    ${A.hay.length ? `<br><br><span class="mut" style="font-size:11px"><b>tienen guion, no lo dicen:</b> ${
      A.hay.map(x=>`<code>${esc(x)}</code>`).join(" ")}</span>` : ""}</p>`;

  $("#agujeros").innerHTML = `<div class="marc">${marc}</div>
    <p class="nota">De ${F.length} preguntas hay <b>${cta("abierta")+cta("parcial")}</b> sin cerrar.
      La idea de llevar este registro es del panel de NERVIO, que enseña 77 abiertas de 105 en
      primer plano. Nuestro panel sólo enseñaba respuestas, así que parecía que no quedaban
      preguntas.</p>${cuerpo}${aud}`;
});

/* ---- robustez ---- */
seguro(function(){
  const r = D.tablas["robustez"]||[], d = D.tablas["descomprimir"]||[];
  const chip = v => v==="OK" ? '<span class="chip bien">OK</span>'
    : v.includes("SIN MEDIR")||v.includes("SIN INTENTAR") ? '<span class="chip ojo">sin medir</span>'
    : `<span class="chip mal">${esc(v)}</span>`;
  $("#robusto").innerHTML = `
    <p class="nota"><b>Comprimir y descomprimir no cuestan lo mismo</b>, y eso no se sabía
      porque el banco sólo medía comprimir. Descomprimir usa <b>11× la memoria</b>: doce hilos
      construían doce modelos de 483 MiB para el mismo bloque.</p>
    ${d.length ? `<div class="env-t"><table><thead><tr>${d[0].map((c,i)=>`<th class="${i?"n":""}">${esc(c)}</th>`).join("")}</tr></thead>
      <tbody>${d.slice(1).map(f=>`<tr>${f.map((c,i)=>{
        const mal = i===3 && +c > 2000;
        return `<td class="${i?"n":""} ${mal?"mal":""}">${i&&!isNaN(+c)?fm(+c):esc(c)}</td>`;
      }).join("")}</tr>`).join("")}</tbody></table></div>` : ""}
    <p class="nota" style="margin-top:16px">Un archivo que se escribe en una máquina de 64 GB
      y no abre en una de 8 no es un formato, es un recuerdo. Por eso esto cuenta como defecto
      y no como lentitud.</p>
    ${r.length ? `<div class="env-t"><table><thead><tr><th>prueba</th><th>resultado</th><th>detalle</th></tr></thead>
      <tbody>${r.slice(1).map(f=>`<tr><td>${esc(f[0])}</td><td>${chip(f[1])}</td>
        <td class="t mut">${esc(f[2]||"")}</td></tr>`).join("")}</tbody></table></div>` : ""}`;
});

/* ---- ruido del banco ---- */
seguro(function(){
  const r = D.tablas["ruido"]||[];
  if (!r.length) return;
  $("#ruido").innerHTML = `
    <p class="nota">La misma compresión, sin cambiar nada, <b>seis veces</b>. La dispersión de
      eso ES el ruido: lo que quede por debajo no se puede afirmar con una sola medida.
      Estimarlo a ojo salió mal —publiqué ±0,62% mirando sólo los valores positivos, olvidando
      que el ruido también empuja hacia abajo—.</p>
    <div class="env-t"><table><tbody>${r.slice(1).map(f=>
      `<tr><td>${esc(f[0])}</td><td class="n"><b>${esc(f[1])}</b></td>
       <td class="t mut">${esc(f[2]||"")}</td></tr>`).join("")}</tbody></table></div>
    <p class="nota" style="margin-top:12px">Por eso las medidas de <b>ratio</b> valen más que
      las de <b>ciclos</b>: la salida del compresor es determinista, así que dos ejecuciones con
      la misma configuración dan exactamente los mismos bytes y el suelo de ruido es cero.</p>`;
});

/* ---- todas las tablas en crudo ---- */
seguro(function(){
  const ya = ["ablacion","censo","coste","poda","talla-poda","rivales","nuestro","perfil",
              "multibench","zip-progreso","zip-hipotesis","zip-composicion","zip-metodos",
              "robustez","descomprimir","ruido"];
  const resto = Object.keys(D.tablas).filter(k => !ya.includes(k)).sort();
  $("#todas").innerHTML = resto.map(k => {
    const f = D.tablas[k];
    return `<div class="frag-t" style="margin-top:14px"><span>${esc(k)}</span>
      <span>${f.length-1} filas</span></div>
      <div class="env-t"><table><thead><tr>${f[0].map((c,i)=>`<th class="${i?"n":""}">${esc(c)}</th>`).join("")}</tr></thead>
      <tbody>${f.slice(1).map(x=>`<tr>${x.map((c,i)=>
        `<td class="${i?"n":""}">${i&&!isNaN(+c)&&c!==""?fm(+c):esc(c)}</td>`).join("")}</tr>`).join("")}
      </tbody></table></div>`;
  }).join("") || '<p class="nota">Todas las tablas ya salen arriba.</p>';
  const vac = Object.entries(D.tablas_vacias || {});
  if (vac.length) {
    $("#todas").innerHTML += `<div class="frag-t" style="margin-top:20px">
        <span>tablas presentes y sin filas</span><span>${vac.length}</span></div>
      <p class="nota">Existen, tienen su sitio y no tienen datos. <b>No es lo mismo que no
        existir</b>: alguien las creó y nadie las ha llenado, así que son huecos del banco con
        nombre y apellidos. Se enseñan por eso, no por completismo.</p>
      <div class="env-t"><table><thead><tr><th>tabla</th><th class="n">lineas</th>
        <th>qué significa</th></tr></thead><tbody>
        ${vac.map(([k,n]) => `<tr><td>${esc(k)}</td><td class="n">${n}</td>
          <td>${n ? "sólo la cabecera: nunca se ha medido nada" : "ni cabecera: el fichero está en blanco"}</td></tr>`).join("")}
      </tbody></table></div>`;
  }
});

/* ---- como leer este panel ---- */
seguro(function(){
  /* Object.assign y no `||`: en una pagina de herramienta `D.auditoria` es {} —cierto: esa
     auditoria no es suya— y {} es truthy, asi que el `||` no saltaba y A.dicho salia undefined.
     Un guardia que solo cubre el caso «no existe» no cubre el caso «existe vacio». */
  const A = Object.assign({dicho:[],hay:[],ninguno:[]}, D.auditoria || {});
  const P = (D.preguntas||[]).slice(1);
  const ab = P.filter(f=>f[2]==="abierta").length;
  $("#comoleer").innerHTML = `
    <p class="nota"><b>El orden importa.</b> Se empieza por <em>Agujeros</em>, no por los rivales.
      Hay <b>${ab}</b> preguntas abiertas y son las que dicen por dónde seguir; las tablas de
      arriba dicen dónde se ha llegado, que es otra cosa y no cambia.</p>
    <p class="nota"><b>Los ciclos mandan sobre el reloj.</b> Esta máquina lleva trabajo de otros
      clientes: la misma prueba tarda 51,6 s o 97,1 s según la carga. Los bytes y los ciclos no
      dependen de eso; los segundos sí. Cuando una fila trae las dos cosas, se compara por ciclos.</p>
    <p class="nota"><b>Los bytes son la única cifra sin ruido.</b> La salida del compresor es
      determinista: dos ejecuciones iguales dan exactamente los mismos bytes. Por eso una
      diferencia de ratio del 0,1 % se puede afirmar y una de tiempo del 10 % no.</p>
    <p class="nota"><b>Un ratio sin la máquina y la entrada no significa nada.</b> Todas las
      comparativas de este panel están medidas en la MISMA máquina y sobre las MISMAS entradas.
      Las cifras de rivales se miden una vez y se cachean, porque el resultado de un compresor
      sobre una entrada dada es una constante.</p>
    <p class="nota"><b>Lo que este panel NO puede afirmar.</b> De ${A.dicho.length+A.hay.length+A.ninguno.length}
      tablas, <b>${A.ninguno.length}</b> no tienen guion que las reproduzca: se hicieron ejecutando
      cosas y pegando el resultado. Siguen siendo ciertas, pero rehacerlas cuesta tanto como
      hacerlas la primera vez, y por eso están contadas arriba en vez de calladas.</p>`;
});

/* ---- inventario ---- */
seguro(function(){
  const l = D.loc.filter(x=>x[1]>30);
  const mx = Math.max(...l.map(x=>x[1]));
  $("#inv").innerHTML = `
    <div class="dos">
      <div>
        <div class="frag-t"><span>Nuestro código</span><span>${fm(D.loc.reduce((a,b)=>a+b[1],0))} líneas</span></div>
        <div class="env-t"><table><tbody>${l.map(f=>`<tr><td class="n">${f[0].replace("crates/","")}</td>
          <td class="n">${fm(f[1])}</td><td><div class="barra"><i style="width:${f[1]/mx*100}%"></i></div></td></tr>`).join("")}
        </tbody></table></div>
        <div class="frag-t" style="margin-top:14px"><span>Binarios</span></div>
        <div class="env-t"><table><tbody>${D.bin.map(f=>`<tr><td class="n">${f[0]}</td>
          <td class="n">${(f[1]/1048576).toFixed(2)} MB</td></tr>`).join("")}</tbody></table></div>
      </div>
      <div>
        <div class="frag-t"><span>Dependencias externas</span><span>${((D.deps||{}).externos||[]).length} crates</span></div>
        <div class="env-t" style="max-height:340px;overflow:auto"><table><tbody>
          ${((D.deps||{}).externos||[]).map(d=>`<tr><td class="n">${esc(d)}</td></tr>`).join("")}
        </tbody></table></div>
        <p class="nota" style="margin-top:12px">Empezamos en 60 y estamos en ${((D.deps||{}).externos||[]).length}.
          El modelo —<b>vc-cm</b>, donde vive toda la compresión— no tiene <b>ninguna</b>:
          es aritmética entera y nada más. Por eso el mismo byte sale en x86 y en ARM.</p>
      </div>
    </div>`;
});
}

/* El clic en las pestañas se engancha UNA vez, al contenedor, que no se reemplaza nunca.
   Engancharlo dentro de pinta() acumularía un oyente por repintado. */
$("#tabs").addEventListener("click", e=>{
  const b = e.target.closest("button"); if(!b) return;
  sel = +b.dataset.i;
  [...$("#tabs").children].forEach((x,i)=>x.setAttribute("aria-selected", i===sel));
  seguro(pintaMapa);
});

pinta();
/* Fuera de pinta() a proposito: el menu de herramientas se construye UNA vez. Dentro, cada
   repintado del modo en vivo le colgaria otro desplegable al titulo. */
pintaHerramientas();
/* ---- cambiar de herramienta ---------------------------------------------------------------
   Sin recargar y sin ir a otro fichero: el panel es UNO. Cada herramienta era un html aparte con
   su copia entera del programa, y cambiar de una a otra era cargar 200 KB de javascript
   identico para ver otros numeros. Ahora se cambia la rama de datos que se mira. */
function pintaSubs(){
  const S = (RAW.comun && RAW.comun.subs) || [];
  const el = $("#subs");
  if (!S.length || el === _VACIO) return;
  el.innerHTML = S.map(x => `<button data-h="${esc(x.slug)}" class="${x.slug===ACTIVA?"aqui":""}"
    title="${esc(x.que_es)}">${esc(x.nombre)}</button>`).join("");
}
function cambia(slug){
  if (!RAW.vistas[slug] || slug === ACTIVA) return;
  ACTIVA = slug;
  arma();
  pinta();
  pintaSubs();
  history.replaceState(null, "", "#" + (slug === _VISTA_POR_DEFECTO ? "" : slug));
  window.scrollTo({ top: 0, behavior: "instant" });
}
seguro(function(){
  pintaSubs();
  const el = $("#subs");
  if (el === _VACIO) return;
  el.addEventListener("click", e => {
    const b = e.target.closest("button[data-h]");
    if (b) cambia(b.dataset.h);
  });
});
avisaFallos();

/* ---- en vivo ----------------------------------------------------------------------------
   Abierto como fichero suelto (file://) no hay nada que pedir y se queda estático, que es lo
   correcto. Servido por http, se re-pide datos.json y se repinta EN SITIO. Se compara el sello
   antes de repintar: si el dato no ha cambiado no se toca el DOM, y así el scroll, la pestaña
   y una selección de texto sobreviven a un panel que lleva horas abierto. */
if (location.protocol !== "file:") {
  const ind = document.createElement("span");
  ind.className = "vivo-ind"; $(".meta").prepend(ind);
  let sello = (RAW.comun||{}).sello, err = 0, visto = Date.now();
  const pinta_ind = () => {
    const s = Math.round((Date.now()-visto)/1000);
    ind.innerHTML = err
      ? `<span class="pt malo"></span>sin conexión con el generador`
      : `<span class="pt"></span>en vivo · ${s < 90 ? s+" s" : Math.round(s/60)+" min"}`;
  };
  setInterval(pinta_ind, 1000);
  setInterval(async () => {
    try {
      const r = await fetch("datos.json?" + Date.now(), {cache: "no-store"});
      if (!r.ok) throw 0;
      const n = await r.json();
      err = 0; visto = Date.now();
      /* Sello contra sello. Antes se comparaba D.sello (con segundos) contra n.gen (con
         minutos): campos distintos y formatos distintos, asi que NUNCA coincidian y la
         primera pasada repintaba siempre, tirandote el scroll sin que hubiera un dato nuevo. */
      /* Dos ritmos distintos, a proposito:
         · el BANCO se repinta entero solo cuando cambia el sello, o sea cuando hay una medida
           nueva de verdad. Asi el scroll y la pestaña sobreviven a un panel abierto horas.
           (pinta() no toca el titulo, asi que el selector de herramientas tampoco se rehace:
           reconstruirlo colgaria un desplegable nuevo cada vez.)
         · la CARGA DE LA MAQUINA cambia cada pocos segundos y no es una medida del banco, asi
           que se refresca EN SITIO, sin tocar nada mas. Meterla en el sello haria repintar la
           pagina entera cada 8 s por un numero que no es del experimento. */
      if (n.comun.sello !== sello) {
        /* Se cambia RAW ENTERO y se vuelve a armar la vista, CONSERVANDO la herramienta que
           estabas mirando. Esto hacia `D = n` con el json global encima de una pagina de
           herramienta: te dejaba el dato de vcompresor entero bajo el titulo de vdeflate, con
           sus doce pestañas sobre cuatro secciones. La separacion duraba 8 segundos. */
        sello = n.comun.sello;
        RAW = n;
        if (!RAW.vistas[ACTIVA]) ACTIVA = _VISTA_POR_DEFECTO;
        arma(); pinta(); pintaSubs(); $(".meta").prepend(ind);
      } else { D.carga = n.comun.carga; D.vuelo = n.comun.vuelo; }
      refrescaCarga(n.comun);
    } catch { err = 1; }
    pinta_ind();
  }, 8000);
  pinta_ind();
}
</script>
"""

# POST-PROCESO: se asigna cada <section> a su pestaña por el titulo de su <h2>. Se hace aqui
# y no tocando las 21 secciones a mano para que la plantilla siga siendo legible de un vistazo.
def _titulo(cuerpo):
    import re as _re
    t = _re.search(r"<h2>([^<]+)</h2>", cuerpo)
    return t.group(1).strip() if t else ""

def _reparte(html):
    """Cada seccion se marca con SU PESTAÑA y SU DUENO, y se queda en la misma pagina.

    UN SOLO PANEL (orden del dueño, 31-07): esto se partia en once ficheros html y cada uno se
    llevaba una copia entera del javascript y del css — 2,6 MB para once veces el mismo programa,
    y once artefactos que pueden quedarse viejos por separado. Lo que hay que separar es EL DATO,
    no el fichero. La pagina es una y ensena la herramienta que elijas."""
    import re as _re
    fuera = []
    def _pon(m):
        cuerpo = m.group(0)
        titulo = _titulo(cuerpo)
        i = REPARTO.get(titulo)
        if i is None:
            if titulo: fuera.append(titulo)
            i = len(PESTANAS) - 1        # a CRUDO: visible, no perdida
        d = DUENO.get(titulo, "principal")
        return cuerpo.replace("<section>", f'<section data-t="{i}" data-h="{d}">', 1)
    html = _re.sub(r"<section>.*?</section>", _pon, html, flags=_re.S)
    if fuera:
        print("   secciones sin pestaña asignada (van a CRUDO):", ", ".join(fuera))
    return html


def _visible(dueno, slug):
    """¿Sale esta seccion cuando se mira `slug`? La MISMA regla vive en el javascript, y la
    verificacion comprueba que las dos digan lo mismo."""
    if dueno == TODAS_LAS_PAGINAS: return True
    if dueno == CADA_HERRAMIENTA:  return slug != "principal"
    return dueno == slug

# ============================================================================================
# DE QUIEN ES CADA SECCION
# ============================================================================================
#
# El panel principal se queda con lo TRANSVERSAL —lo que cruza herramientas o habla del metodo—
# y cada herramienta con lo suyo. Una seccion que no este aqui se queda en el principal y se
# dice al generar: repartir a ojo es como una cifra acaba decidiendo por quien no la midio.
CADA_HERRAMIENTA = "*"
TODAS_LAS_PAGINAS = "**"
DUENO = {
    # ── en TODAS las paginas sin excepcion, cada una con SUS numeros ──
    "Qué mejoró y qué empeoró, commit a commit":       TODAS_LAS_PAGINAS,
    # ── en TODAS las paginas de herramienta, cada una con SUS numeros ──
    "La herramienta, de un vistazo":                   CADA_HERRAMIENTA,
    # ── el principal: la portada y el instrumento ──
    "Qué se decidió, y con qué medida":                "principal",
    "Nuestras herramientas, una por una":              "principal",
    "Las junturas":                                    "principal",
    "El techo conocido":                               "principal",
    "Cómo leer este panel":                            "principal",
    "Agujeros":                                        "principal",
    "Las preguntas del camino":                        "principal",
    "El resultado es correcto, el camino no":          "principal",
    "Cinco formas de ir más rápido":                   "principal",
    "Precomputar: cuándo sí y cuándo empeora":         "principal",
    "Dónde se han hecho ya":                           "principal",
    "Cómo se resuelve":                                "principal",
    "Las piezas: lo de fuera y lo nuestro":            "principal",
    "Nuestras herramientas":                           "principal",
    "La aguja":                                        "principal",
    # ── el archivador: la tuberia entera y lo que produce ──
    "La pirámide":                                     "archivador",
    "Los bucles, en orden":                            "archivador",
    "Dónde se va el tiempo":                           "archivador",
    "Velocidad real contra la que decimos tener":      "archivador",
    "El dinero":                                       "archivador",
    "¿Qué carril se paga, y a qué precio?":            "archivador",
    "Contra todos los archivadores conocidos":         "archivador",
    "Contra los rivales":                              "archivador",
    # Del archivador, como las otras dos: son los bytes que produce LA TUBERIA ENTERA. Ponerla en
    # el principal la haria transversal, y no lo es — es el resultado de una herramienta concreta
    # contra los suyos. Que la pestaña sea la de la linea de tiempo no cambia de quien es el dato.
    "Contra los rivales, en el tiempo":                "archivador",
    "El carril ZIP":                                   "archivador",
    "Mapa de bits":                                    "archivador",
    "Robustez y puntos ciegos":                        "archivador",
    # ── vdeflate ──
    "vdeflate — nuestro DEFLATE":                      "vdeflate",
    # ── vc-cm ──
    "La pirámide de vc-cm — nuestra alternativa a zstd": "vc-cm",
    "Libro mayor de modelos":                          "vc-cm",
    # ── el banco: el instrumento y todo lo medido ──
    "El registro de métodos":                          "banco",
    "El libro mayor, agregado y auditado":             "banco",
    "Precisión del propio banco":                      "banco",
    "Todas las medidas":                               "banco",
    "Ejecuciones":                                     "banco",
    "Inventario":                                      "banco",
}

# ============================================================================================
# UNA PAGINA POR HERRAMIENTA — TODAS, SIN EXCEPCION
# ============================================================================================
#
# La lista SALE DEL CENSO (bench/atribucion.py) y no se escribe aqui. Escrita aqui tenia cinco
# entradas cuando el censo ya tenia diez: seis herramientas nuestras no tenian pagina, vlz entre
# ellas, y no habia forma de enterarse mirando el panel. Ahora anadir una herramienta es anadir
# una linea al censo; si ademas hubiera que acordarse de tocar esto, algun dia no se tocaria —
# que es exactamente lo que paso.
#
# slug · fichero · nombre · una linea de que es
SUBS = [("principal", "panel.html", "vcompresor",
         "La portada: qué hemos construido, cómo va cada pieza y qué preguntas quedan.")] + [
        (_k, f"h-{_k}.html", _nom, _res)
        for _k, _nom, _q, _s, _c, _p, _res in ATR.HERRAMIENTAS]

# ============================================================================================
# EL DATO SE PARTE POR HERRAMIENTA, NO SOLO LA PANTALLA
# ============================================================================================
#
# EL FALLO DE FONDO, y estuvo a la vista desde el primer dia sin que nadie lo leyera: `_para()`
# repartia las SECCIONES y el bloque de datos se copiaba ENTERO en las cinco paginas.
# h-archivador.html llevaba dentro el libro mayor completo, el camino completo y los rivales
# completos — los mismos bytes que panel.html con dos cadenas cambiadas (`yo` y `pestanas`).
# Y EL PROPIO PANEL LO CANTABA: las cinco paginas salian con el mismo sello, y la cabecera dice
# que dos paneles con el mismo sello son el mismo dato. El instrumento estaba dando la senal
# correcta y el generador no la escuchaba.
#
# Norma del dueño (31-07-2026): «cada herramienta debe de tener sus mediciones, sus caminos y
# todo aparte, porque se miden por separado». Asi que cada pagina se lleva SOLO lo suyo. No es
# cosmetica: mientras el dato viaje entero, el modo en vivo lo vuelve a mezclar en 8 segundos.

# De quien es cada clave se DEDUCE de quien la pinta. Ver _duenos_de_claves().
COMPARTIDAS = {
    # el armazon: lo pinta el marco, no una seccion de nadie.
    "commit", "asunto", "sucio", "host", "pestanas", "sello", "subs", "yo", "paneles",
    "carga", "vuelo", "gen",
    # HASTA DONDE LLEGA EL LIBRO MAYOR. Va en el armazon y no en una seccion porque es
    # del PANEL ENTERO, no de una herramienta: contesta «¿lo que miro es de hoy?», y esa
    # pregunta se hace igual en la pagina de vdeflate que en la del archivador.
    "libro_hasta", "libro_filas",
    # LAS PREGUNTAS DEL CAMINO VIAJAN INTACTAS entre herramientas y entre proyectos: son el
    # instrumento, no el dato de nadie. Lo que cambia por herramienta es la RESPUESTA, y esa
    # vive en `auditcam`, que si se filtra fila a fila.
    "camino",
    # la ficha de la herramienta de esta pagina y su recorrido, calculados abajo.
    "ficha",
    # el log de git: es metadato del repo, no medida de nadie, y lo necesitan las once paginas
    # para poner nombre y fecha a cada punto de su linea de tiempo.
    "commits",
}
# Fila a fila, no por clave entera: son las tablas donde conviven varias herramientas.
FILTRADAS = ("mayor", "ejec", "tablas", "tablas_vacias", "auditcam", "desperdicio", "loc",
             "linea")
# Las cinco que el deducido no puede acertar, con su motivo. La cabecera las usa, pero SOLO en el
# principal: en una pagina de herramienta la cabecera pinta los numeros de ESA herramienta
# (`D.ficha`) y no toca ninguna de estas.
FORZADAS = {"multi": {"archivador"}, "mapa": {"archivador"},
            "deps": {"principal", "banco"}, "bin": {"banco"}, "nuestro": {"archivador"},
            # ESTAS TRES NO LAS PINTA NADIE. Se cargan, viajan en el json y no hay un solo
            # renderizador que las use: `versiones` acompana a la comparativa, `camino_comprimir`
            # es el camino del archivador y `decay` es del modelo de vc-cm. Se quedan con su
            # dueno —no se tiran, que son medidas— y se ven en CRUDO, pero que conste que estan
            # sin pantalla propia. El generador lo dice en cada pasada.
            "versiones": {"archivador"}, "camino_comprimir": {"archivador"},
            "decay": {"vc-cm"}}
SIN_PANTALLA = ("versiones", "camino_comprimir", "decay")


def _duenos_de_claves(html):
    """clave de datos -> conjunto de duenos, DEDUCIDO de quien la pinta.

    No se escribe a mano a proposito. `piramide` es del archivador porque quien la usa es el
    renderizador que escribe en `#piramide`, que vive en la seccion «La pirámide», que DUENO da
    al archivador. Un mapa a mano seria una SEGUNDA lista que mantener — y la segunda lista que
    nadie mantuvo es exactamente como empezo todo esto."""
    ids = {}
    for m in re.finditer(r"<section>.*?</section>", html, re.S):
        d = DUENO.get(_titulo(m.group(0)), "principal")
        for i in re.findall(r'id="([\w-]+)"', m.group(0)):
            ids[i] = d
    js = html[html.index("<script>"):]
    out, cabecera = {}, set()
    for m in re.finditer(r"seguro\(function\(\)\{", js):
        ini, prof, fin = m.end() - 1, 0, m.end() - 1
        for k in range(ini, len(js)):
            if js[k] == "{": prof += 1
            elif js[k] == "}":
                prof -= 1
                if prof == 0: fin = k; break
        bloque = js[ini:fin]
        mios = {ids[i] for i in re.findall(r'\$\("#([\w-]+)"\)', bloque) if i in ids}
        claves = (set(re.findall(r"\bD\.(\w+)", bloque))
                  | set(re.findall(r'\bD\["(\w+)"\]', bloque)))
        if not mios: cabecera |= claves
        for c in claves:
            out.setdefault(c, set()).update(mios)
    for c in cabecera:
        out.setdefault(c, set())
    return out


_DUENOS = _duenos_de_claves(HTML)
_DUENOS.update(FORZADAS)

_PROPIAS_IX = {f[0]: f for f in D["propias"][1:]} if D["propias"] else {}
_PCAB = D["propias"][0] if D["propias"] else []
_ACAB = D["auditcam"][0] if D["auditcam"] else []


def _ficha_de(k):
    """La tarjeta de una herramienta: que es, que se ha medido de ella y que se le ha preguntado.

    Los contadores salen de LO MISMO que pinta la tabla de al lado —el cruce de las preguntas del
    camino con las filas de esta pieza— para que el numero de arriba y la tabla de abajo no
    puedan contradecirse. Lo que las matrices propias anaden encima se dice aparte, no se suma en
    silencio."""
    fila = {h[0]: h for h in ATR.HERRAMIENTAS}[k]
    med = [r for r in _HIST if ATR.herr_de_fila(r)[0] == k]
    decl = sum(1 for r in med if ATR.herr_de_fila(r)[1])
    # su camino: el mismo cruce que pinta la tabla
    _iv = _ACAB.index("veredicto") if "veredicto" in _ACAB else -1
    _ip = _ACAB.index("pregunta_id") if "pregunta_id" in _ACAB else -1
    suyas = {f[_ip]: f[_iv] for f in D["auditcam"][1:]
             if _iv >= 0 and len(f) > _iv and ATR.MAPA_PIEZA.get(f[0]) == k}
    _idp = D["camino"][0].index("id") if D["camino"] else 0
    total = max(len(D["camino"]) - 1, 0)
    hechas = hall = sinm = 0
    for q in D["camino"][1:]:
        v = suyas.get(q[_idp], "SIN MIRAR")
        if v.startswith("SIN"): sinm += 1
        elif v.startswith("NO"): pass
        else:
            hechas += 1
            if v.startswith("HALLAZGO") or v.startswith("SOSPECHA"): hall += 1
    # lo que dice el censo, que ademas cuenta las matrices propias (vdeflate y comprimir)
    cen = _PROPIAS_IX.get(k)
    def _c(col):
        try: return int(cen[_PCAB.index(col)])
        except Exception: return 0
    return {
        "clave": k, "nombre": fila[1], "que_es": fila[2], "sustituye_a": fila[3],
        "codigo": fila[4], "resumen": fila[6],
        "estado": cen[_PCAB.index("estado")] if cen and "estado" in _PCAB else "—",
        "medidas": len(med), "declaradas": decl,
        "camino": {"hechas": hechas, "hallazgos": hall, "sin": sinm, "total": total,
                   "mas": max(_c("preguntas_hechas") - hechas, 0)},
        "tablas": sorted(t for t in D["tablas"] if ATR.herr_de_tabla(t) == k),
        "loc": [l for l in D["loc"] if ATR.herr_de_ruta(l[0]) == k],
    }


def _vacio_como(v):
    """El hueco del mismo tipo. Las claves que no son de esta pagina NO se quitan: se dejan
    VACIAS.

    Suena a matiz y no lo es. El javascript va entero en las once paginas —esa es la regla que
    permite un solo generador— asi que el renderizador de los rivales existe tambien en la pagina
    de vlz. Si le quitamos la clave del todo, `D.rivales` es `undefined` y revienta; si le
    dejamos el dato, hemos vuelto a mezclar. Vacia es lo unico que es a la vez seguro y CIERTO:
    esta pagina no tiene rivales medidos. Y el renderizador ya sabe decirlo — «Sin datos» es una
    respuesta honesta, y ademas es la respuesta correcta."""
    if isinstance(v, list): return []
    if isinstance(v, dict): return {}
    if isinstance(v, bool): return v
    if isinstance(v, (int, float)): return 0
    if isinstance(v, str): return ""
    return None


def _para_datos(slug):
    """Los datos de UNA pagina: lo compartido, lo que es suyo, y el resto vacio."""
    d = {}
    for k, v in D.items():
        if k in COMPARTIDAS or k in FILTRADAS: continue
        d[k] = v if slug in _DUENOS.get(k, set()) else _vacio_como(v)
    # LO COMPARTIDO NO SE COPIA EN CADA VISTA: vive UNA vez en `comun` y el navegador lo mezcla
    # al armar `D`. Copiarlo aqui metia el log de git y las preguntas del camino once veces —
    # 400 KB de exactamente el mismo texto dentro del mismo fichero. Es la misma enfermedad que
    # los once html, en pequeño.

    if slug == "principal":
        # El principal es la portada del PROYECTO: lo transversal, el censo entero y las dos
        # vistas que cruzan herramientas a proposito —la matriz del camino y el censo del
        # desperdicio—. El libro mayor y las tablas en crudo NO: esos son del banco.
        d["propias"] = D["propias"]
        d["junturas"] = D["junturas"]
        d["undisparo"] = D["undisparo"]
        d["techos"] = D["techos"]
        d["loc"] = D["loc"]
        d["auditcam"] = D["auditcam"]
        d["desperdicio"] = D["desperdicio"]
        d["mayor"] = _vacio_como(D["mayor"])
        # LA LINEA GLOBAL VA EN EL PRINCIPAL: es la del producto entero, la que contesta «cuánto
        # ha mejorado o empeorado esto con cada commit» sin partir por pieza.
        d["linea"] = D["linea"]
        d["ejec"] = []
        d["tablas"] = {}
        d["tablas_vacias"] = {}
        return d

    # ── una herramienta: SOLO sus filas, y SOLO lo que su pagina pinta ──
    #
    # Lo segundo importa tanto como lo primero. El libro mayor agregado pesa ~700 KB y lo pinta
    # UNA seccion, la del banco: mandarselo tambien a las otras diez vistas es copiarlo diez
    # veces para no enseñarlo. De la ficha de cada herramienta solo cuelgan los CONTADORES, asi
    # que va el resumen sin los grupos. Un panel de 1,5 MB del que 1,2 MB no se pintan nunca es
    # la misma enfermedad de las copias repartidas, solo que dentro del fichero.
    d["ficha"] = _ficha_de(slug)
    _mias = [r for r in _HIST if ATR.herr_de_fila(r)[0] == slug]
    d["ejec"] = []
    d["loc"] = d["ficha"]["loc"]
    d["auditcam"] = ([D["auditcam"][0]] +
                     [f for f in D["auditcam"][1:] if ATR.MAPA_PIEZA.get(f[0]) == slug]
                     ) if D["auditcam"] else []
    d["desperdicio"] = []
    # Las tablas en crudo solo las pinta quien tiene seccion para ellas — el carril ZIP y la
    # robustez del archivador, y el CRUDO del banco. Al resto le basta con la LISTA de nombres
    # que ya lleva su ficha.
    if slug in _DUENOS.get("tablas", set()):
        d["tablas"] = {t: v for t, v in D["tablas"].items() if ATR.herr_de_tabla(t) == slug}
        d["tablas_vacias"] = {t: v for t, v in D["tablas_vacias"].items()
                              if ATR.herr_de_tabla(t) == slug}
    else:
        d["tablas"], d["tablas_vacias"] = {}, {}
    if slug == "banco":
        # EL BANCO ES EL INSTRUMENTO: su objeto de estudio ES el libro mayor entero, con su
        # reparto por herramienta dentro. No es mezcla, es su trabajo — y es la unica pagina
        # donde mirar el total tiene sentido.
        d["mayor"] = D["mayor"]
        d["ejec"] = D["ejec"]
        d["tablas"] = D["tablas"]
        d["tablas_vacias"] = D["tablas_vacias"]
    else:
        # El resumen SIN los grupos: la ficha usa los contadores y el suelo, nadie pinta la lista
        # de experimentos fuera del banco. `filas` se queda porque es lo que la verificacion
        # contrasta contra la ficha para cazar dato ajeno dentro de una vista.
        d["mayor"] = dict(_mayor_de(_mias, _grupos_de(_mias)), grupos=[])
    # Su recorrido, con SU suelo de ruido: el del banco entero no vale para juzgar el delta de
    # una pieza que se mide de otra manera.
    d["linea"] = (D["linea"] if slug == "banco"
                  else _linea_de(_mias, _suelo_de(d["mayor"])))
    return d

# ============================================================================================
# SE ESCRIBE **UN** PANEL
# ============================================================================================
#
# EL SELLO ES EL CONTENIDO, no la hora. sha256 de todo lo que se va a pintar: cambia si y solo si
# cambia un dato. Sirve para las dos cosas a la vez — que el html sea determinista y que la pagina
# servida sepa si de verdad hay algo nuevo que repintar. Lo que cambia solo por mirar el reloj o
# la maquina NO entra en el html: la carga y los procesos en vuelo son estado del servidor en este
# instante, no del banco. Si entraran, dos generaciones seguidas darian ficheros distintos sin que
# haya cambiado ni una medida, que es justo lo que hace inutil un `git diff` del panel.
#
# LA FORMA DEL BLOQUE DE DATOS ES LA SEPARACION. No hay un `D` plano con todo mezclado: hay
#     {comun: {...}, vistas: {principal: {...}, archivador: {...}, vlz: {...}, ...}}
# y el navegador arma `D = comun + vistas[la que miras]`. Las mediciones de vdeflate no estan
# «filtradas al pintar»: es que no viven en la misma rama del json que las del archivador. Para
# que una cifra cruce de herramienta habria que reescribir esto a proposito.
sal = os.path.join(B, "panel.html")
_repartido = _reparte(HTML)
D["subs"] = [{"slug": sl, "fichero": fi, "nombre": no, "que_es": qe} for sl, fi, no, qe in SUBS]

# GUARDIA R1 APLICADA A LOS DATOS, no solo a las tablas: toda clave de D tiene que tener dueno.
# El dia que alguien anada un dato nuevo y no lo pinte nadie, o lo pinte un renderizador que este
# fichero no sabe leer, saldria en TODAS las vistas o en NINGUNA sin que nadie se entere. Aqui se
# para: es el mismo criterio que ya impedia que una .tsv nueva quedara invisible.
_sin_dueno = sorted(k for k in D
                    if k not in COMPARTIDAS and k not in FILTRADAS and k not in VIVOS
                    and not _DUENOS.get(k))
if _sin_dueno:
    print(f"VERIFICACION FALLA: claves de datos sin dueno —no se sabe en que vista van—: "
          f"{_sin_dueno}", file=sys.stderr)
    sys.exit(1)
print(f"   datos sin pantalla propia (se ven solo en CRUDO): {', '.join(SIN_PANTALLA)}")

# Las pestañas de cada vista: las que de verdad tienen alguna seccion suya. Una pestaña vacia es
# peor que no tenerla, porque promete algo que no esta. Viaja el INDICE GLOBAL junto al nombre —
# el numero que ve el usuario se cuenta por posicion al pintar. Mezclar esos dos numeros dejo el
# panel en blanco una vez: van separados y con nombre distinto.
_SECS = [(int(m.group(1)), m.group(2))
         for m in re.finditer(r'<section data-t="(\d+)" data-h="([^"]*)">', _repartido)]


def _pestanas_de(slug):
    return [[t, PESTANAS[t][1].split(" · ", 1)[-1]]
            for t in sorted({t for t, d in _SECS if _visible(d, slug)})]


_VISTAS = {}
for _slug, _fich, _nom, _ in SUBS:
    _v = _para_datos(_slug)
    _v["pestanas"] = _pestanas_de(_slug)
    _VISTAS[_slug] = _v

_COMUN = {k: D[k] for k in COMPARTIDAS if k in D and k not in VIVOS and k != "pestanas"}
_COMUN["subs"] = D["subs"]
_TODO = {"comun": _COMUN, "vistas": _VISTAS}
_sello = hashlib.sha256(
    json.dumps(_TODO, ensure_ascii=False, sort_keys=True).encode()).hexdigest()[:16]
_TODO["comun"]["sello"] = _sello
D["sello"] = _sello

open(sal, "w", encoding="utf-8").write(
    _repartido.replace("__DATOS__", json.dumps(_TODO, ensure_ascii=False)))
# Los mismos datos, aparte. Sirviendo el directorio por http la pagina los re-pide y se repinta
# sola; abierta como fichero suelto no los pide y usa los incrustados. Un solo generador cubre los
# dos modos y no hay una version que se quede vieja. En el .json SI va la hora: es lo que deja al
# indicador decir "en vivo hace 12 s" y ponerse rojo si el generador deja de responder.
open(os.path.join(B, "datos.json"), "w", encoding="utf-8").write(json.dumps(
    dict(_TODO, comun=dict(_COMUN, carga=D.get("carga"), vuelo=D.get("vuelo"),
                           gen=time.strftime("%Y-%m-%d %H:%M:%S"))), ensure_ascii=False))

for _slug, _fich, _nom, _ in SUBS:
    _v = _VISTAS[_slug]
    _f = _v.get("ficha") or {}
    print(f"   {_nom:<22} {len(_v['pestanas'])} pestañas · "
          f"{sum(1 for t, d in _SECS if _visible(d, _slug)):>2} secciones"
          + ("" if not _f else f" · {_f.get('medidas')} medidas suyas"))
print(f"{sal}  ({os.path.getsize(sal)//1024} KB)  ·  {len(SUBS)} vistas en UN fichero  ·  "
      f"sello {_sello}")

# LOS FICHEROS DEL PANEL TROCEADO SE BORRAN. Si se quedan, el navegador que tenga abierto un
# h-*.html viejo sigue enseñandolo tan tranquilo —con el dato del dia que se genero— y no hay
# forma de saber que se esta mirando un fantasma. Un artefacto viejo que abre es peor que uno que
# falta: el que falta se nota.
_restos = sorted(glob.glob(os.path.join(B, "h-*.html")) +
                 glob.glob(os.path.join(B, "datos-*.json")))
for _r in _restos:
    os.remove(_r)
if _restos:
    print(f"   quitados {len(_restos)} ficheros del panel troceado (ahora es uno solo)")


# Un solo panel, una sola verificacion — pero que cubre las once vistas por dentro.
_rc = verifica(sal)

# ── Y QUE SE VEA DE VERDAD, abriendolo con un navegador ──────────────────────────────────────
#
# La verificacion de arriba mira el FICHERO: que las pestañas esten, que los datos parseen, que
# no haya referencias externas. Todo eso lo cumplia un panel que en el navegador salia EN BLANCO,
# y hoy paso DOS VECES: una porque `pintaMapa` reventaba y era la unica llamada fuera de
# `seguro()`, y otra porque las secciones se renumeraban a 1..5 y el nav las enciende por 0..4.
#
# Asi que se abre de verdad, se pulsan todas las pestañas y se comprueba que ninguna seccion
# quede vacia ni invisible. Si no hay navegador se DICE y no se calla: una comprobacion que se
# salta en silencio es peor que no tenerla, porque deja la sensacion de que se hizo.
import subprocess as _sp
_vivo = os.path.join(B, "panel-vivo.js")
if os.path.exists(_vivo) and not os.environ.get("VC_PANEL_SIN_NAVEGADOR"):
    try:
        _r = _sp.run(["node", _vivo, B], capture_output=True, text=True, timeout=180)
        print(_r.stdout.rstrip())
        if _r.returncode == 2:
            print("   (sin navegador: NO se ha comprobado que el panel se vea. "
                  "Ver bench/panel-vivo.js)", file=sys.stderr)
        elif _r.returncode != 0:
            print(_r.stderr.rstrip(), file=sys.stderr)
            _rc |= 1
    except FileNotFoundError:
        print("   (no hay `node`: NO se ha comprobado que el panel se vea)", file=sys.stderr)
    except _sp.TimeoutExpired:
        print("   (la comprobacion viva se colgo: NO se ha comprobado que el panel se vea)",
              file=sys.stderr)
        _rc |= 1
sys.exit(_rc)
