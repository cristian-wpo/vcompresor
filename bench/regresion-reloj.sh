#!/usr/bin/env bash
# V0 — ¿DE DONDE SALIERON LOS 595 SEGUNDOS?
#
# El 28-07 el corpus de codigo costaba 145 s con --max y hoy cuesta 740,8. zpaq, medido hoy en la
# misma maquina, da 139,6 s contra los 132,8 de entonces: +5%. O sea que LA MAQUINA NO LO EXPLICA.
# Antes de optimizar nada hay que saber que se compro con esos 595 segundos.
#
# COMO. Se recorren N commits del tramo, se compila cada uno y se mide la MISMA entrada
# (`codigo.10m`, un fichero suelto) con `--max`. Un fichero suelto AISLA EL CODEC: si la curva de
# ciclos sube aqui, el gasto esta en vc-cm; si sale plana, esta en el archivador (recorrido,
# carriles, dedup) y el siguiente paso es repetirlo sobre un arbol.
#
# SE COMPARAN CICLOS, NO RELOJ. La maquina lleva la flota encima y el reloj varia el doble; los
# ciclos cuentan nuestro trabajo. Por eso cada punto pasa por `bench/mide.sh` DEL PROPIO COMMIT:
# asi la fila lleva su commit de verdad, y no el de HEAD del arbol principal.
#
# NO TOCA EL ARBOL PRINCIPAL. Otra sesion esta trabajando en el repo, asi que esto vive en un
# worktree aparte y comparte el directorio de compilacion para no rehacer las dependencias N veces.
#
#   uso:  bash bench/regresion-reloj.sh [n_puntos] [muestra]
set -uo pipefail
cd "$(dirname "$0")/.."
RAIZ=$(pwd)
# Una sesion SSH no interactiva no lee el perfil, asi que `cargo` no esta en el PATH y el guion
# se saltaba los diez puntos diciendo "no compila". Un guion que informa de un fallo del entorno
# como si fuera un fallo del codigo es peor que uno que se para.
export PATH="$HOME/.cargo/bin:$PATH"
command -v cargo >/dev/null || { echo "no hay cargo en el PATH"; exit 1; }
PUNTOS=${1:-9}
MUESTRA=${2:-codigo.10m}
ENTRADA=/tmp/compresor-f1/samples/$MUESTRA
DESDE=fa1f379   # 28-07, el commit del README que publica los 145 s
HASTA=$(git rev-parse --short HEAD)
WT=/tmp/v0-bisect
export CARGO_TARGET_DIR=/tmp/v0-target
FILAS=/tmp/v0-filas.jsonl

[ -f "$ENTRADA" ] || { echo "no existe $ENTRADA"; exit 1; }
: > "$FILAS"

# Los puntos se reparten por POSICION en la historia, no por fecha: un dia con 80 commits y otro
# con 3 no son dos puntos comparables si lo que se busca es donde entro el gasto.
mapfile -t TODOS < <(git log --format=%h --reverse $DESDE..$HASTA)
N=${#TODOS[@]}
PASO=$(( N / (PUNTOS-1) )); [ "$PASO" -lt 1 ] && PASO=1
SEL=("$DESDE")
for ((i=PASO; i<N-1; i+=PASO)); do SEL+=("${TODOS[$i]}"); done
SEL+=("$HASTA")
echo "V0 · ${#SEL[@]} puntos de $N commits · muestra $MUESTRA · ciclos manda"
printf '   %s\n' "${SEL[@]}"

git worktree add -f --detach "$WT" "$DESDE" >/dev/null 2>&1 || { echo "no se pudo crear el worktree"; exit 1; }

for c in "${SEL[@]}"; do
  echo "── $c ──"
  # `-f`: la medida anterior dejo `bench/historia.jsonl` modificado en el worktree y el checkout
  # siguiente se negaba. El worktree es desechable —lo que habia que salvar ya esta en $FILAS—,
  # asi que aqui forzar es correcto. En el arbol principal NO se hace esto nunca.
  git -C "$WT" checkout -q -f --detach "$c" 2>/dev/null || { echo "   checkout falla, salto"; continue; }
  # EL MISMO INSTRUMENTO EN LOS DIEZ PUNTOS. La bascula nacio el 29-07 (`d551d93`), asi que en los
  # commits de antes no existe y la mitad de la curva se mediria de otra forma —o no se mediria—.
  # Se copia la de HOY encima: mide.sh lee el commit de SU directorio, o sea el del punto, asi que
  # la fila sigue llevando el commit correcto. Cambiar de regla a mitad de una serie es la forma
  # mas facil de fabricar un escalon que no existe.
  cp -f "$RAIZ/bench/mide.sh" "$WT/bench/mide.sh" 2>/dev/null
  [ -f "$RAIZ/bench/comun.sh" ] && cp -f "$RAIZ/bench/comun.sh" "$WT/bench/comun.sh" 2>/dev/null
  touch "$WT/bench/historia.jsonl"
  # `--offline`: el tramo no cambia dependencias, y bajar la red a mitad de tanda meteria un
  # tiempo que no es nuestro.
  if ! ( cd "$WT" && cargo build --release --offline -q 2>/dev/null ); then
    ( cd "$WT" && cargo build --release -q 2>&1 | tail -3 ) || { echo "   no compila, salto"; continue; }
  fi
  [ -x "$WT/target/release/vcompresor" ] || [ -x "$CARGO_TARGET_DIR/release/vcompresor" ] || {
    echo "   sin binario, salto"; continue; }
  BIN=$CARGO_TARGET_DIR/release/vcompresor
  [ -x "$BIN" ] || BIN=$WT/target/release/vcompresor
  ANTES=$(wc -l < "$WT/bench/historia.jsonl" 2>/dev/null || echo 0)
  ( cd "$WT" && bash bench/mide.sh -e "V0 regresion de reloj" -m "$MUESTRA" -c "max" -- \
      "$BIN" comprimir "$ENTRADA" -o /tmp/v0.vc --max ) 2>&1 | tail -1
  tail -n +$((ANTES+1)) "$WT/bench/historia.jsonl" 2>/dev/null >> "$FILAS"
done

git worktree remove --force "$WT" >/dev/null 2>&1
echo "── resumen (ciclos por byte, que es lo que no depende de la carga) ──"
python3 - "$FILAS" <<'PY'
import json,sys
prev=None
for l in open(sys.argv[1]):
    l=l.strip()
    if not l: continue
    r=json.loads(l)
    cb=r.get("ciclos",0)/max(1,r.get("bytes_in",1))
    d="" if prev is None else "  %+.1f%%"%((cb-prev)/prev*100)
    print("  %-9s %s  %12s B  %7.1f s  %9.0f ciclos/byte%s"%(
        r.get("commit"), r.get("t","")[:10], r.get("bytes_out"), r.get("seg",0), cb, d))
    prev=cb
PY
echo
# ⚠️ AQUI HABIA UNA BOMBA: se imprimia `cat $FILAS >> bench/historia.jsonl`. El libro es
# append-only, no tiene clave unica y nadie lo auditaba, asi que correr esa linea dos veces metia
# las filas repetidas sin un solo aviso. Y las filas repetidas no rompen: PROMEDIAN, y las cifras
# de ese experimento salen un poco mal para siempre. bench/integra.py hace lo mismo comparando
# por la clave del libro: correrlo dos veces deja el libro igual que correrlo una.
echo "Las filas estan en $FILAS — se integran (sin poder duplicarlas) con:"
echo "  python3 bench/integra.py $FILAS"
