# -*- shell-script -*-
# LA FONTANERIA COMUN DE LAS PRUEBAS. Se carga con:  . "$(dirname "$0")/comun.sh"
#
# POR QUE EXISTE. Cada prueba nueva reinventaba lo mismo y volvia a pisar la MISMA trampa. En un
# solo dia se repitieron cinco que ya se conocian:
#   1. `declare -A` — el bash 3.2 de macOS no lo tiene, y el resumen de una prueba salio en blanco
#   2. `stat -c%s` en Linux contra `stat -f%z` en el Mac — un guion portado devolvia vacio
#   3. `pgrep -f <cadena>` se encuentra A SI MISMO: su propia linea de comandos contiene la cadena.
#      Contesta «si» para siempre. Y una vez mato la conexion SSH desde la que se llamaba.
#   4. una muestra demasiado pequeña para distinguir las dos cosas que compara: el corpus sql cabe
#      en UN frame, asi que «solid» y «no-solid» daban bytes IDENTICOS y la prueba no probaba nada
#   5. dos cadenas de reloj corriendo a la vez por relanzar sin comprobar
#   6. EDITAR UN GUION MIENTRAS CORRE. Bash NO lee el fichero entero al arrancar: lo va leyendo
#      por posicion, asi que reescribirlo a mitad hace que siga leyendo desde el offset viejo
#      sobre el texto nuevo. Paso el 02-08 con bench/mezclador.sh: solto sus dos tablas de
#      resultados —correctas, las produjo el guion viejo— y despues murio con «syntax error near
#      unexpected token )» en una linea que en disco esta perfecta. La regla es la misma que ya
#      valia para el repo durante una medida: mientras algo corre, no se toca su fuente.
#
# Ninguna era nueva. Se repiten porque cada guion se las vuelve a encontrar por su cuenta.

# ── portabilidad Mac/Linux ────────────────────────────────────────────────────────────────────
talla() { stat -c%s "$1" 2>/dev/null || stat -f%z "$1" 2>/dev/null || echo 0; }
carga() { awk '{print $1}' /proc/loadavg 2>/dev/null || uptime | sed 's/.*averages*: *//' | awk '{print $1}' | tr -d ','; }
nucleos() { nproc 2>/dev/null || sysctl -n hw.ncpu 2>/dev/null || echo 4; }

# ── esperar: SIEMPRE por centinela en fichero, NUNCA por proceso ──────────────────────────────
# Un `pgrep -f X` desde un guion cuya linea de comandos contiene X se encuentra a si mismo.
# Y un proceso que arranca y muere entre configuraciones se «pierde» si se mira en el instante
# equivocado: eso encadeno dos pruebas a la vez en el Mac.
# Y SI AUN ASI HAY QUE PREGUNTAR POR UN PROCESO: EL CORCHETE, PERO ESCRITO A MANO.
#
# La regla de arriba —espera por fichero, nunca por proceso— es la buena, y aun asi el 02-08 volvi
# a escribir `until ! pgrep -f 'bash bench/mezclador.sh'` desde una linea de comandos que CONTIENE
# esa cadena. El bucle se encontro a si mismo y no habria salido nunca. Tercera vez que muerde,
# segunda estando ya documentada.
#
#   MAL   pgrep -f 'bench/mezclador.sh'      <- la cadena esta en tu propia linea de comandos
#   BIEN  pgrep -f 'bench/mezclad[o]r.sh'    <- tu comando lleva los CORCHETES, y el patron
#                                               'mezclad[o]r' no casa con el texto 'mezclad[o]r'
#
# ⚠️ Y NO SE PUEDE ENVOLVER EN UNA FUNCION, que fue lo primero que intente. Escribi un
# `corre()` que le metia el corchete al patron en tiempo de ejecucion... y sigue encontrandose a
# si mismo: el corchete tiene que estar en EL TEXTO QUE TU ESCRIBES, porque lo que pgrep compara
# es la linea de comandos literal. Si la funcion lo añade despues, tu linea sigue teniendo la
# cadena desnuda. Comprobado: la funcion contestaba OCUPADO con la maquina vacia.
#
# O sea que esto no tiene atajo: o centinela en fichero, o corchete tecleado. Por eso sigue
# primero lo de arriba.
espera_centinela() {   # $1 = fichero  $2 = cadena  [$3 = segundos entre miradas]
  local f="$1" c="$2" s="${3:-20}"
  while ! grep -q "$c" "$f" 2>/dev/null; do sleep "$s"; done
}

# ── un solo RELOJ por maquina ─────────────────────────────────────────────────────────────────
# Los BYTES son deterministas y se pueden medir en paralelo; los relojes no. Este cerrojo NO es
# «una prueba a la vez»: es «una que cronometre a la vez».
CERROJO=${CERROJO:-/tmp/vc-reloj.lock}
toma_reloj() {
  if [ -e "$CERROJO" ] && kill -0 "$(cat "$CERROJO" 2>/dev/null)" 2>/dev/null; then
    echo "  ya hay una medida de TIEMPO en marcha (pid $(cat "$CERROJO")). Esta espera o se salta."
    return 1
  fi
  echo $$ > "$CERROJO"; trap 'rm -f "$CERROJO"' EXIT; return 0
}

# ── el arbol limpio, con las exclusiones correctas ────────────────────────────────────────────
# bench/registros es SALIDA DE MEDIDAS, no codigo: si contara como sucio, cada medida haria
# abortar a la siguiente. Paso de verdad y mato dos.
arbol_limpio() {
  git rev-parse --git-dir >/dev/null 2>&1 || return 0     # el Mac no es repo: no aplica
  local n
  n=$(git status --porcelain -- . ':!bench/historia.jsonl' ':!bench/datos.json' \
        ':!bench/panel.html' ':!bench/registros' ':!bench/registros-m3' 2>/dev/null | wc -l)
  [ "$n" -eq 0 ] && return 0
  echo "  ABORTA: $n ficheros de codigo sin commitear, y esto mide tiempo"; return 1
}

# ── la muestra: REPARTIDA, nunca «los primeros N» ─────────────────────────────────────────────
# «Los primeros N» ya publico un 97,14% falso contra un 84,70% honesto: los ficheros salen
# ordenados por nombre, o sea AGRUPADOS POR ORIGEN, y los primeros no se parecen al resto.
# EL SELLO DE UNA MUESTRA, para que dos numeros de dos dias se puedan comparar de verdad.
#
# ⚠️ Nace de una discrepancia sin explicar: el 02-08, dos ejecuciones del MISMO barrido sobre la
# MISMA muestra de sql dieron 38.389 y 38.388 bytes. Un byte en 38 mil (0,0026%) no cambia ninguna
# conclusion, pero la suite entera se apoya en que los bytes son DETERMINISTAS —es lo que justifica
# correr las de bytes en paralelo—, asi que una diferencia sin causa no se puede dejar pasar. Se
# comprobo aparte que `muestra_repartida` SI es determinista (tres llamadas, tres sha256
# identicos), o sea que la causa estaba en otro sitio y no se encontro.
#
# Esto no lo arregla: lo hace ATRIBUIBLE. Con el sello en el log, la proxima vez se sabe de un
# vistazo si cambio la muestra o cambio el codec — que hoy no se pudo distinguir.
sello() { (sha256sum "$1" 2>/dev/null || shasum -a 256 "$1" 2>/dev/null) | cut -c1-12; }

muestra_repartida() {   # $1 = raiz  $2 = bytes  $3 = salida
  local raiz="$1" tope="$2" sal="$3" n
  n=$(find "$raiz" -type f 2>/dev/null | wc -l)
  [ "$n" -gt 0 ] || { echo "  no hay ficheros en $raiz"; return 1; }
  # Uno de cada K, calculado para acercarse al tope sin pasarse mucho.
  find "$raiz" -type f 2>/dev/null | sort | awk -v k=3 'NR%k==1' \
    | xargs cat 2>/dev/null | head -c "$tope" > "$sal"
  [ "$(talla "$sal")" -gt 4096 ] || { echo "  la muestra salio vacia"; return 1; }
  MUESTRA_SELLO=$(sello "$sal")
  return 0
}

# ── ¿PUEDE esta prueba distinguir lo que compara? ─────────────────────────────────────────────
# La comprobacion que falto el dia que se comparo «solid» contra «no-solid» sobre 5 MB: los dos
# caben en un frame, dieron bytes IDENTICOS, y el resultado se leyo como «no hay diferencia».
# Una prueba que no puede distinguir sus dos casos no es una prueba que sale limpia: es ninguna.
distingue_o_aborta() {   # $1 = valorA  $2 = valorB  $3 = que se comparaba
  if [ "$1" = "$2" ]; then
    echo "  ⚠️ ABORTA: $3 dan EXACTAMENTE el mismo resultado ($1)."
    echo "     Eso casi nunca significa «no hay diferencia»: significa que la entrada es"
    echo "     demasiado pequeña o que la bandera no llego. Subir la entrada y repetir."
    return 1
  fi
  return 0
}

cab() {   # cabecera comun de toda prueba
  echo "$1"
  echo "  maquina $(uname -s) · $(nucleos) nucleos · carga $(carga) · $(date -u +%H:%M:%SZ)"
  git rev-parse --short HEAD >/dev/null 2>&1 && echo "  commit $(git rev-parse --short HEAD)"
  echo
}
