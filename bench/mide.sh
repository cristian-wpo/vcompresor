#!/usr/bin/env bash
# LA BASCULA. Ninguna medida vuelve a hacerse a pelo.
#
# El problema que resuelve no es de graficos: hoy se hicieron treinta medidas y viven en
# /tmp de una maquina que se reinicia, en cinco formatos distintos y en la cabeza de quien
# las hizo. Cuando hace falta comparar contra "lo de anteayer", hay que volver a medirlo.
# Eso es lo caro. No el calculo: la RE-derivacion.
#
# Aqui todo lo que se ejecuta pasa por el mismo sitio y deja UNA linea JSON en
# bench/historia.jsonl, con el commit, la maquina, la configuracion, los bytes, los
# segundos, la CPU, la memoria pico y los ciclos. Append-only y versionado en git.
#
#   bench/mide.sh -e "F2.48 decaimiento w>>16" -m sql.10m -c "VC_DECAY=16" -- \
#       ./target/release/vcompresor comprimir /tmp/s/sql.10m -o /tmp/x.vc --max
#
# Reglas que impone (y que hoy se saltaron a mano mas de una vez):
#   - los CICLOS mandan sobre el reloj. El reloj de esta maquina varia el doble segun la
#     carga; los ciclos cuentan nuestro trabajo. Se guardan los dos y se compara por ciclos.
#   - se anota la carga de la maquina en el momento. Una medida tomada a carga 8 y otra a
#     carga 0 no son comparables, y el panel lo marca en vez de dejar que enganen.
#   - con -s FICHERO se guarda lo que el programa escriba por stderr (el desglose por
#     fases de VC_FASES, por ejemplo). Sin -s se pierde, que es lo que pasaba hasta hoy.
#   - se anota si el arbol estaba sucio, si lo estaba EL CODIGO (crates/, Cargo.*) y el SELLO DEL
#     BINARIO que corrio. Los tres, porque contestan cosas distintas: «sucio» es honesto pero salta
#     por un .sh de bench que el compresor no abre; «sucio_codigo» acota; y el sello caza el caso
#     que ni el commit ni el arbol limpio ven — checkout sin recompilar, arbol impecable, binario
#     de otro commit. Dos filas con el mismo sello midieron EL MISMO PROGRAMA.
#   - se anota DE QUE HERRAMIENTA es la medida, y esto no es burocracia: --price estimaba la
#     CPU con una tabla que mide EL CODEC SOLO para decidir sobre EL ARCHIVADOR ENTERO. Los dos
#     numeros eran correctos —34,8x de diferencia entre niveles para el codec, 1,79x para el
#     archivador— y ninguno servia para lo del otro. La cifra cruzo de herramienta porque nada
#     anotaba de cual era. Con -H se dice; sin -H se deduce del comando, porque una norma que
#     hay que acordarse de cumplir no es una norma.
set -uo pipefail
cd "$(dirname "$0")/.."

# EL CANDADO. Dos medidas a la vez no son dos medidas: son dos medidas mal.
#
# Paso el 01-08 y no fue un descuido de un momento. Tres guiones de fondo lanzados con una hora
# de diferencia (13:20, 13:36, 14:19) seguian VIVOS, encolados detras de compresiones largas, y
# llegaron a su paso --max a la vez. Dos escribian en el MISMO /tmp/b.vc, asi que uno peso el
# archivo del otro; y los tres se repartieron la CPU, asi que ninguno de los relojes valia. El
# fallo NO SE VE al lanzarlos —cada guion, por separado, era correcto—: aparece por el retraso
# entre lanzar y ejecutar. Por eso la disciplina de «acuerdate de correrlas en serie» no basta y
# hace falta que la maquina lo IMPONGA.
#
# Se re-ejecuta bajo flock. Si hay otra medida dentro, ESPERA en vez de abortar: encolar es lo
# correcto, porque quien lanza una tanda quiere sus numeros, no un error. --verbose dice cuanto
# espero, y esa linea es la prueba de que hubo cola.
if [ -z "${VC_MIDE_EN_SERIE:-}" ]; then
  export VC_MIDE_EN_SERIE=1
  exec flock --verbose bench/.medida.lock "$0" "$@"
fi

REG=bench/historia.jsonl

ETI="" MUE="" CFG="" NOTA="" ERR="" HERR="" SALX=""
while [ $# -gt 0 ]; do
  case "$1" in
    -e|--etiqueta) ETI=$2; shift 2 ;;
    -m|--muestra)  MUE=$2; shift 2 ;;
    -c|--config)   CFG=$2; shift 2 ;;
    -n|--nota)     NOTA=$2; shift 2 ;;
    -s|--stderr)   ERR=$2; shift 2 ;;   # guardar lo que el programa diga por stderr
    -H|--herramienta) HERR=$2; shift 2 ;; # de QUE herramienta es esta medida
    # LA SALIDA, CUANDO EL COMANDO NO LLEVA `-o`. Abajo se deduce del `-o` del propio
    # comando, que vale para lo nuestro y NO vale para nadie mas: zpaq escribe
    # `zpaq a ARCHIVO ficheros`, sin flag. El 03-08 se midio zpaq con esta bascula por
    # primera vez y la fila salio con `bytes_out: 0` — o sea la bascula apuntando cero en
    # silencio, que es justo lo que un banco no puede hacer. Con `-S` se le dice cual es.
    -S|--salida)   SALX=$2; shift 2 ;;
    --) shift; break ;;
    *) echo "opcion desconocida: $1" >&2; exit 2 ;;
  esac
done
[ $# -gt 0 ] || { echo "uso: bench/mide.sh -e etiqueta -m muestra [-c config] -- comando..." >&2; exit 2; }

# la salida: la que se haya declarado con -S, y si no, el -o del propio comando. El orden
# importa: lo declarado a mano gana, porque se declara precisamente cuando lo automatico
# no puede acertar.
SAL="${SALX:-}"; prev=""
[ -n "$SAL" ] || for a in "$@"; do [ "$prev" = "-o" ] && SAL=$a; prev=$a; done
# la entrada: el primer camino que existe SALTANDO el ejecutable y lo que cuelgue de target/.
# (la primera version cogia ./target/release/vcompresor como "entrada" y media el binario)
ENT=""; n=0
for a in "$@"; do
  n=$((n+1)); [ $n -eq 1 ] && continue
  case "$a" in target/*|./target/*|/*/target/*) continue ;; esac
  [ -e "$a" ] && [ -z "$ENT" ] && ENT=$a
done

COMMIT=$(git rev-parse --short HEAD 2>/dev/null || echo "?")
# «Sucio» quiere decir: el CODIGO que se esta midiendo no esta identificado por su commit.
# NO cuentan los ficheros que la propia maquina de medir escribe -el libro mayor, el panel y su
# acompanante-, porque entonces la primera medida de cualquier tanda ensucia todas las
# siguientes y el instrumento se marca a si mismo como contaminacion. Paso de verdad: el
# control de cero salio con sus tres vueltas «sucias» con el codigo intacto.
SUCIO=$(git status --porcelain -- . \
          ':!bench/historia.jsonl' ':!bench/datos.json' ':!bench/panel.html' ':!bench/registros' ':!bench/registros-m3' \
        2>/dev/null | head -1 | wc -l)
# ...Y «SUCIO DE CODIGO», que es lo que de verdad invalida una fila.
#
# `sucio` es un booleano sobre TODO el arbol, asi que dejar caer un guion nuevo en bench/ marca la
# medida como no reproducible EXACTAMENTE IGUAL que tocar el codec. Paso el 02-08: siete filas de
# una tanda salieron «sucias» por seis ficheros .sh que el compresor no abre nunca. Una alarma que
# salta por lo que no importa enseña a ignorarla, y entonces no salta por lo que si.
SUCIO_COD=$(git status --porcelain -- crates Cargo.toml Cargo.lock 2>/dev/null | head -1 | wc -l)

# EL SELLO DEL CODIGO QUE SE MIDIO — y no del commit que habia en HEAD, que no es lo mismo.
#
# 🚨 EL AGUJERO QUE TAPA, visto el 02-08 con la linea de tiempo del panel plana un dia entero de
# trabajo. El metodo de esta casa es MEDIR PRIMERO Y COMMITEAR DESPUES, con la cifra en el mensaje
# — que es lo correcto—. Pero `commit` se sella en el momento de medir, asi que la fila queda bajo
# el commit ANTERIOR y el commit que CONTIENE el cambio sale con cero medidas. Diez commits de una
# tarde, todos a cero, y las cuatro filas buenas archivadas bajo el commit de la vispera.
#
# `sucio_codigo` ya avisaba de que el codigo no era el del commit, pero avisar no es identificar.
# Esto lo IDENTIFICA: el digesto de los blobs de git de crates/ + Cargo.* tal y como estan EN EL
# DISCO. Dos filas con el mismo `arbol_sha` midieron el mismo codigo, lo hubiera commiteado
# alguien o no — y `bench/aplicado.py` calcula ese mismo digesto para cada commit y las casa.
#
# Se usa `git hash-object` y no un sha256 del fichero a proposito: es EXACTAMENTE el identificador
# que git le da a ese contenido, asi que el lado del commit puede sacarlo de `git ls-tree` sin
# leer un solo fichero. Los dos lados ordenan por ruta y digieren el mismo flujo de bytes.
#
# ⚠️ Incluye los NO RASTREADOS (`-o`): un fichero nuevo bajo crates/ es codigo que se esta
# midiendo. Si no esta en ningun commit, el digesto no casara con ninguno — que es la respuesta
# correcta, no un fallo.
ARBOL_SHA=$( { git ls-files -co --exclude-standard -- crates Cargo.toml Cargo.lock 2>/dev/null \
                 | LC_ALL=C sort \
                 | while IFS= read -r f; do
                     [ -f "$f" ] && printf '%s %s\n' "$f" "$(git hash-object "$f" 2>/dev/null)"
                   done; } \
               | (sha256sum 2>/dev/null || shasum -a 256 2>/dev/null) | cut -c1-12 )

# EL SELLO DEL BINARIO QUE DE VERDAD CORRIO. Ni el commit ni el arbol limpio lo garantizan.
#
# El commit dice que codigo hay ESCRITO; no dice cual esta COMPILADO. Un `git checkout` sin
# `cargo build` deja el arbol limpio, el commit correcto y un binario de OTRO commit — y la fila
# sale impecable y miente. Al reves tambien: arbol sucio en bench/ con el binario intacto marca
# contaminacion donde no la hay.
#
# El sello del ejecutable contesta la pregunta de verdad —«¿es comparable esta fila con aquella?»—
# sin depender de git: dos filas con el mismo sello midieron EL MISMO PROGRAMA.
EJE=""
for a in "$@"; do case "$a" in */target/*|target/*|./target/*) [ -x "$a" ] && EJE=$a && break ;; esac; done
[ -z "$EJE" ] && EJE=$(command -v "$1" 2>/dev/null)
BINSHA=""
if [ -n "$EJE" ] && [ -f "$EJE" ]; then
  BINSHA=$( (sha256sum "$EJE" 2>/dev/null || shasum -a 256 "$EJE" 2>/dev/null) | cut -c1-12 )
fi
CARGA=$(awk '{print $1}' /proc/loadavg 2>/dev/null || sysctl -n vm.loadavg 2>/dev/null | awk '{print $2}')
T=$(mktemp); P=$(mktemp)

# perf, y a ser posible SIN sudo. Los eventos ":u" cuentan solo espacio de usuario, que es
# justo lo que queremos —nuestro trabajo, no el del nucleo— y ademas los deja medir un usuario
# normal con perf_event_paranoid=1. Correr esto bajo sudo tenia un efecto secundario feo: el
# fichero de salida quedaba de root y la siguiente medida no podia sobrescribirlo.
PERF=""
if command -v perf >/dev/null 2>&1; then
  if perf stat -x, -e cycles:u -o /dev/null true 2>/dev/null; then
    PERF="perf stat -x, -e cycles:u,instructions:u,cache-misses:u -o $P"
  elif sudo -n true 2>/dev/null; then
    PERF="sudo -n perf stat -x, -e cycles,instructions,cache-misses -o $P"
  fi
fi

E=$(mktemp)
# LA SALIDA DEL COMANDO SE GUARDA (P12). Antes iba a /dev/null y se perdieron tres tablas en un
# dia, la ultima tras 53 minutos de maquina: quedaba la fila del libro mayor -ciclos, RAM,
# reloj- y desaparecia LO QUE LA MEDIDA HABIA AVERIGUADO.
#
# Se guarda y NO se re-emite: volcarla a la terminal ahogaria el resumen de la propia bascula
# y haria ilegible una tanda de veinte medidas. El nombre lleva la etiqueta y el commit, que es
# lo que hace falta para volver a encontrarla dentro de un mes.
mkdir -p bench/registros/salidas
SLUG=$(printf '%s' "${ETI:-medida}" | tr -cs 'A-Za-z0-9._-' '-' | cut -c1-60)
OUTF="bench/registros/salidas/${SLUG}-${COMMIT}.txt"
/usr/bin/time -v -o "$T" $PERF "$@" >"$OUTF" 2>"$E"
RC=$?
# Una medida que fallo y se registra como si nada es peor que no medir: la fila queda en el
# registro con ceros y luego alguien la compara. Se avisa en la salida y rc queda en el JSON.
[ $RC -ne 0 ] && { echo "!! rc=$RC — la medida NO es valida:"; tail -3 "$E" | sed 's/^/   /'; }
# Se dice SIEMPRE, tambien si fallo: una medida que falla es justo cuando mas falta hace saber
# que llego a decir antes de caerse.
[ -s "$OUTF" ] && echo "   salida: $OUTF ($(wc -l < "$OUTF" | tr -d ' ') lineas)"

# Y SI NO DIJO NADA, NO SE DEJA EL FICHERO. La redireccion de arriba CREA el fichero siempre,
# tambien cuando el programa no escribe una linea por stdout — que es el caso normal de `vc-cm`,
# que escribe el resultado al fichero de salida y el desglose a stderr. Lo que quedaba entonces
# era un .txt de 0 bytes con la etiqueta y el commit en el nombre: un fichero que ATESTA una tanda
# y no contiene ni una linea de ella. El 10-08 la revision de F6.Q3 se encontro seis, y tenia
# razon en cantarlos — un nombre no es una prueba. Se borra el vacio: la prueba de esa corrida es
# su fila en bench/historia.jsonl, y el desglose se pide con -s.
[ -s "$OUTF" ] || rm -f "$OUTF"

# Y el caso que se cuela con rc=0: se pidio -o y el fichero no esta. Paso de verdad —un rm -rf
# a destiempo le borro la salida a una medida en vuelo— y la fila quedo en el registro con
# 0 bytes, con sus ciclos y su RAM al lado, con toda la pinta de ser un dato. Una fila que
# miente es peor que una fila que falta.
if [ -n "${SAL:-}" ] && [ ! -f "${SAL:-}" ] && [ $RC -eq 0 ]; then
  echo "!! el comando dijo que fue bien pero $SAL no existe — la medida NO es valida"
  RC=90
fi

val() { awk -F: "/$1/{gsub(/^ +/,\"\",\$2); print \$2; exit}" "$T"; }
pv()  { awk -F, "/$1/{print \$1; exit}" "$P" 2>/dev/null; }

SEG=$(awk -F': ' '/Elapsed \(wall/{print $NF; exit}' "$T" | awk -F: '{if(NF==3)print $1*3600+$2*60+$3; else if(NF==2)print $1*60+$2; else print $1}')
CPU=$(val "Percent of CPU"); CPU=${CPU%\%}
RSS=$(val "Maximum resident set size")
BIN=$( [ -n "$ENT" ] && du -sb "$ENT" 2>/dev/null | cut -f1 || echo 0 )
BOUT=$( [ -n "$SAL" ] && [ -f "$SAL" ] && stat -c%s "$SAL" 2>/dev/null || echo 0 )
CIC=$(pv cycles); INS=$(pv instructions); FAL=$(pv cache-misses)

# LAS VARIABLES QUE HABIA DE VERDAD, no las que el que llamo declaro en -c.
#
# Las dos cosas no son la misma y hoy se vio la diferencia. VC_VISTAS tenia el defecto en 3 con
# la medida ya en contra; el defecto equivocado sobrevivio porque TODOS los guiones pasaban
# VC_VISTAS=0 a mano, asi que el banco solo veia el caso corregido. -c lo escribe una persona y
# refleja lo que esa persona CREE que puso; esto lee el entorno del proceso. Cuando los dos no
# coincidan, el que sabe es este.
#
# Y sirve tambien para lo contrario: una fila con env_vc VACIO es una tirada del producto tal
# como se lo encuentra el usuario. Antes esa fila no se distinguia de una con la config puesta.
# 🚨 Y SE TAPA LO QUE PAREZCA UN SECRETO. Este fichero (bench/historia.jsonl) ESTA VERSIONADO
# EN GIT: una variable VC_PASSWORD puesta para medir el cifrado acabaria commiteada. Se tapa el
# VALOR y se conserva el NOMBRE, porque saber que la corrida iba con contrasena SI importa para
# leerla.
ENVVC=$(env | grep "^VC_" | grep -v "^VC_MIDE_EN_SERIE=" | sort \
  | sed -E "s/^(VC_[A-Z0-9_]*(PASS|PASSWORD|SECRET|TOKEN|KEY|CLAVE|CRED)[A-Z0-9_]*)=.*/\1=<TAPADO>/" \
  | tr "\n" " ")

python3 - "$REG" <<PY
import json, sys, time, os, platform
f = {
  "t": time.strftime("%Y-%m-%dT%H:%M:%S"), "commit": "$COMMIT", "sucio": $SUCIO == 1,
  "sucio_codigo": $SUCIO_COD == 1, "bin_sha": "$BINSHA", "arbol_sha": "$ARBOL_SHA",
  "host": platform.node(), "arch": platform.machine(),
  "etiqueta": """$ETI""", "muestra": """$MUE""", "config": """$CFG""", "nota": """$NOTA""",
  "cmd": """$*""", "rc": $RC, "carga": float("${CARGA:-0}" or 0),
  "herramienta": """$HERR""", "env_vc": """$ENVVC""",
  "bytes_in": $BIN, "bytes_out": $BOUT,
  "seg": float("${SEG:-0}" or 0), "cpu_pct": float("${CPU:-0}" or 0),
  "rss_kb": int("${RSS:-0}" or 0),
  "ciclos": int("${CIC:-0}" or 0), "instr": int("${INS:-0}" or 0), "fallos": int("${FAL:-0}" or 0),
}
# DE QUE HERRAMIENTA ES ESTO. Si no se dijo con -H, se deduce del binario que se ejecuto. Una
# medida sin herramienta es una medida que puede acabar decidiendo por otra cosa.
if not f["herramienta"]:
    c = f["cmd"]
    if "vc-inflate" in c or "vc-redeflate" in c or "vc-fase3" in c \
       or "vc-prediccion" in c or "vc-cobertura" in c or "vc-hueco" in c and "huecos" not in c:
        f["herramienta"] = "vdeflate"
    elif "vc-huecos" in c or "vc-zstd-perfil" in c:
        f["herramienta"] = "codec-zstd"
    elif "vc-cm" in c:
        f["herramienta"] = "vc-cm"
    elif "vcompresor" in c:
        f["herramienta"] = "archivador"
    else:
        f["herramienta"] = "sin clasificar"
if f["bytes_in"] and f["bytes_out"]:
    f["ratio"] = round(f["bytes_in"] / f["bytes_out"], 4)
    f["bpb"] = round(f["bytes_out"] * 8 / f["bytes_in"], 5)
if f["ciclos"] and f["instr"]:
    f["ipc"] = round(f["instr"] / f["ciclos"], 3)
open(sys.argv[1], "a").write(json.dumps(f, ensure_ascii=False) + "\n")
print(f'{f["etiqueta"] or f["cmd"][:40]}  ->  {f["bytes_out"]} B'
      + (f'  ratio {f["ratio"]}x' if "ratio" in f else "")
      + f'  {f["seg"]:.1f}s  {f["rss_kb"]//1024} MB'
      + (f'  {f["ciclos"]/1e9:.2f} Gciclos  IPC {f["ipc"]}' if "ipc" in f else "")
      + (f'  [carga {f["carga"]:.1f}]' if f["carga"] > 1 else ""))
PY
# Lo que el programa cuenta de si mismo —el desglose de VC_FASES, los avisos— se estaba
# TIRANDO al acabar. Una medida de siete minutos producia dos cosas y el instrumento se
# quedaba con una: para volver a ver el desglose habia que repetir los siete minutos.
# Con -s se conserva; sin -s, el comportamiento es el de siempre.
[ -n "${ERR:-}" ] && cp "$E" "$ERR"
rm -f "$T" "$P" "$E"
exit $RC
