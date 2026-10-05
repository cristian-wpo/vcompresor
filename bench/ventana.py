# -*- coding: utf-8 -*-
"""¿ALCANCE O REARRANQUES? Las dos ganancias que el barrido de frames no supo separar.

DE DONDE VIENE. El barrido de frames midio que subir el frame a 64 MiB compra un 2,147% de bytes.
La lectura facil era «el frame era el limite, luego el hueco del LDM (-5,89%) ya esta desbloqueado».
Es FALSA, y lo dice la propia libreria: ZSTD_getCParams devuelve windowLog=22 para el nivel 9 SEA
CUAL SEA el tamaño de la entrada. Le estabamos dando frames de 64 MiB a un compresor que mira 4 MiB
hacia atras. O sea que ese 2,147% no puede ser alcance: es no rearrancar las tablas de entropia
tantas veces.

QUE SEPARA ESTA PRUEBA. Sobre EL MISMO trozo de 64 MiB, cuatro configuraciones:
  A  como hoy            el nivel a secas — es lo que hace `zstd::bulk::compress(&pf.data, lvl)`
  B  ventana al frame    windowLog=26, o sea 64 MiB: el alcance que el frame ya permitiria
  C  LDM                 el buscador de largo alcance, con la ventana de hoy
  D  ventana + LDM       las dos

Si B ~ A, el alcance no es el limite y toda la ganancia del frame era rearranques.
Si C > B, el buscador barato de largo alcance encuentra lo que la ventana grande sola no.
Si D > C y D > B, se suman y hay que exponer las dos perillas.

LA MUESTRA NO ES «LOS PRIMEROS N». Eso ya publico un 97,14% falso contra un 84,70% honesto: los
ficheros salen ordenados por nombre, o sea AGRUPADOS POR ORIGEN, y los primeros no se parecen al
resto. Aqui se toma un trozo de cada K a lo largo de todo el corpus.

Y CADA CONFIGURACION HACE IDA Y VUELTA. Un archivo mas pequeño que no vuelve no es una mejora.
"""
import ctypes, ctypes.util, glob, os, sys, time, json

# ── la libreria, y se le pregunta a ella ──────────────────────────────────────────────────────
ruta = ctypes.util.find_library("zstd") or next(iter(
    glob.glob("/usr/lib/x86_64-linux-gnu/libzstd.so*") + glob.glob("/usr/lib/libzstd.so*")), None)
if not ruta:
    sys.exit("no encuentro libzstd — no invento la tabla, se dice y se para")
lz = ctypes.CDLL(ruta)
lz.ZSTD_versionString.restype = ctypes.c_char_p
lz.ZSTD_createCCtx.restype = ctypes.c_void_p
lz.ZSTD_createDCtx.restype = ctypes.c_void_p
lz.ZSTD_freeCCtx.argtypes = lz.ZSTD_freeDCtx.argtypes = [ctypes.c_void_p]
lz.ZSTD_CCtx_setParameter.argtypes = [ctypes.c_void_p, ctypes.c_int, ctypes.c_int]
lz.ZSTD_CCtx_setParameter.restype = ctypes.c_size_t
lz.ZSTD_compress2.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_size_t,
                              ctypes.c_char_p, ctypes.c_size_t]
lz.ZSTD_compress2.restype = ctypes.c_size_t
lz.ZSTD_decompressDCtx.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_size_t,
                                   ctypes.c_char_p, ctypes.c_size_t]
lz.ZSTD_decompressDCtx.restype = ctypes.c_size_t
lz.ZSTD_DCtx_setParameter.argtypes = [ctypes.c_void_p, ctypes.c_int, ctypes.c_int]
lz.ZSTD_DCtx_setParameter.restype = ctypes.c_size_t
lz.ZSTD_isError.restype = ctypes.c_uint
lz.ZSTD_getErrorName.restype = ctypes.c_char_p
lz.ZSTD_compressBound.argtypes = [ctypes.c_size_t]
lz.ZSTD_compressBound.restype = ctypes.c_size_t
# Devuelve la estructura POR VALOR. Tiene que declararse como Structure: poner un array de
# c_uint como restype compila y revienta con volcado de nucleo, sin decir por que.
class CParams(ctypes.Structure):
    _fields_ = [(n, ctypes.c_uint) for n in
                ("windowLog", "chainLog", "hashLog", "searchLog",
                 "minMatch", "targetLength", "strategy")]
lz.ZSTD_getCParams.restype = CParams
lz.ZSTD_getCParams.argtypes = [ctypes.c_int, ctypes.c_ulonglong, ctypes.c_size_t]

def chk(r, que):
    if lz.ZSTD_isError(r):
        sys.exit(f"zstd dice que no en «{que}»: {lz.ZSTD_getErrorName(r).decode()}")
    return r

C_NIVEL, C_WLOG, C_LDM, D_WLOG = 100, 101, 160, 100   # ZSTD_c_* / ZSTD_d_windowLogMax

NIVEL = int(os.environ.get("NIVEL", "9"))
RAIZ  = os.environ.get("CORPUS", "/tmp/compresor-f1/corpus")
TROZO = int(os.environ.get("TROZO_MIB", "64")) << 20
CADA  = int(os.environ.get("CADA", "5"))       # un trozo de cada CADA, a lo largo de TODO el corpus

# La ventana que pediria un frame de este tamaño, y la que zstd usa hoy por su cuenta.
WLOG = max(10, (TROZO - 1).bit_length())
WHOY = None                     # se rellena abajo, preguntandoselo a la libreria

# LAS CUATRO SE FIJAN EXPLICITAMENTE, y esto es una correccion de la propia prueba. En el primer
# intento «C LDM» no fijaba ventana, y salio IDENTICA a «D ventana + LDM» hasta el ultimo byte.
# No era casualidad: encender el LDM SUBE la ventana por su cuenta —zstd la lleva a 128 MB salvo
# que se ponga a mano—, asi que C y D acababan siendo la misma configuracion con dos nombres.
# Una prueba que no puede distinguir las dos cosas que compara no es una prueba.
def conf():
    return [
        ("A como hoy",         {}),                            # el nivel a secas
        ("B ventana al frame", {C_WLOG: WLOG}),                 # solo ALCANCE
        ("C LDM sin alcance",  {C_WLOG: WHOY, C_LDM: 1}),       # solo el BUSCADOR largo
        ("D ventana + LDM",    {C_WLOG: WLOG, C_LDM: 1}),       # las dos
    ]

# ── la muestra: un trozo de cada CADA, no los primeros ─────────────────────────────────────────
def trozos():
    buf, i = bytearray(), 0
    for d, subs, fs in os.walk(RAIZ):
        subs.sort()
        for f in sorted(fs):
            try:
                with open(os.path.join(d, f), "rb") as fh: buf += fh.read()
            except OSError:
                continue
            while len(buf) >= TROZO:
                t, buf = bytes(buf[:TROZO]), buf[TROZO:]
                if i % CADA == 0: yield i, t
                i += 1

print(f"  libzstd {lz.ZSTD_versionString().decode()} · nivel {NIVEL} · trozos de {TROZO>>20} MiB "
      f"· uno de cada {CADA}")
_cp = lz.ZSTD_getCParams(NIVEL, TROZO, 0)
WHOY = _cp.windowLog
CONF = conf()
print(f"  ventana que zstd usa solo: {2**WHOY>>20} MiB (windowLog {WHOY}) · "
      f"la que le pedimos en B y D: {2**WLOG>>20} MiB (windowLog {WLOG})")
if WLOG <= WHOY:
    sys.exit(f"  el trozo ({TROZO>>20} MiB) no supera la ventana del nivel ({2**WHOY>>20} MiB): "
             f"B y D no pueden dar mas alcance y la prueba no distingue nada. Sube TROZO_MIB.")
print()

tot = {n: [0, 0.0] for n, _ in CONF}          # bytes, segundos
n_trozos = crudo = 0
dcx = lz.ZSTD_createDCtx()
chk(lz.ZSTD_DCtx_setParameter(dcx, D_WLOG, 31), "ventana del descompresor")

for idx, t in trozos():
    n_trozos += 1; crudo += len(t)
    cap = lz.ZSTD_compressBound(len(t))
    dst = ctypes.create_string_buffer(cap)
    vuelta = ctypes.create_string_buffer(len(t))
    linea = []
    for nombre, params in CONF:                # EN SERIE: paralelizar el reloj lo falsea
        cx = lz.ZSTD_createCCtx()
        chk(lz.ZSTD_CCtx_setParameter(cx, C_NIVEL, NIVEL), "nivel")
        for k, v in params.items():
            chk(lz.ZSTD_CCtx_setParameter(cx, k, v), f"parametro {k}={v}")
        t0 = time.perf_counter()
        n = chk(lz.ZSTD_compress2(cx, dst, cap, t, len(t)), f"comprimir {nombre}")
        dt = time.perf_counter() - t0
        lz.ZSTD_freeCCtx(cx)
        # IDA Y VUELTA, siempre: un archivo mas pequeño que no vuelve no es una mejora.
        m = chk(lz.ZSTD_decompressDCtx(dcx, vuelta, len(t), dst, n), f"descomprimir {nombre}")
        if m != len(t) or vuelta.raw[:m] != t:
            sys.exit(f"IDA Y VUELTA ROTA en {nombre}, trozo {idx}")
        tot[nombre][0] += n; tot[nombre][1] += dt
        linea.append(f"{nombre.split()[0]} {100*n/len(t):5.2f}%")
    print(f"  trozo {idx:>3}   " + "  ".join(linea))

lz.ZSTD_freeDCtx(dcx)
print(f"\n  {n_trozos} trozos · {crudo/2**20:.0f} MiB de dato real\n")
base = tot[CONF[0][0]][0]
bt   = tot[CONF[0][0]][1]
print(f"  {'configuracion':<20}{'bytes':>14}{'vs. hoy':>10}{'seg':>9}{'CPU vs. hoy':>13}")
print("  " + "-" * 66)
fil = []
for nombre, _ in CONF:
    b, s = tot[nombre]
    d  = "—" if b == base else f"{100*(b-base)/base:+.3f}%"
    dc = "—" if s == bt   else f"{s/bt:.2f}x"
    print(f"  {nombre:<20}{b:>14,}{d:>10}{s:>9.1f}{dc:>13}")
    fil.append({"conf": nombre, "bytes": b, "seg": round(s, 2),
                "vs_hoy_pct": None if b == base else round(100*(b-base)/base, 4),
                "cpu_x": None if s == bt else round(s/bt, 3)})
print()
os.makedirs("bench/registros/ventana", exist_ok=True)
sal = f"bench/registros/ventana/n{NIVEL}-t{TROZO>>20}-c{CADA}.json"
json.dump({"nivel": NIVEL, "trozo_mib": TROZO >> 20, "cada": CADA, "trozos": n_trozos,
           "crudo": crudo, "wlog_pedido": WLOG, "libzstd": lz.ZSTD_versionString().decode(),
           "filas": fil}, open(sal, "w"), ensure_ascii=False, indent=1)
print(f"  {sal}")
