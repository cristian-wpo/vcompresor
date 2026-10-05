#!/usr/bin/env bash
# LA ABLACION DEL ARBOL COMPLETO, OTRA VEZ, CON LA MAQUINA QUIETA.
#
# POR QUE SE REPITE. Las medidas de F3.1 se tomaron con el servidor a CARGA 41, 44, 54 y 56 —lo
# dice bench/historia.jsonl, porque la bascula la anota— y luego se compararon contra medidas
# tomadas a carga 3,6 y 6,5. De ahi salieron dos cifras que el panel publica hoy:
#
#     agrupar cuesta 13,3% del reloj   <- pareja a carga 54 contra carga 56
#     JXL     cuesta 42,3% del reloj   <- run a carga 56 contra run a carga 6,5
#
# La unica pareja limpia que hay en todo el registro —las dos de --no-jpeg, a carga 6,5 y 3,6,
# seguidas— dice que agrupar cuesta 1,2%. Y la tanda F3.2 entera, tomada a carga 4-7, dice 1,76%.
# Ocho veces menos que la cifra publicada.
#
# El resultado de aquellas medidas era correcto: los programas corrieron y los numeros salieron.
# El camino estaba mal: comparar dos numeros exige que las dos maquinas fueran la misma maquina,
# y a carga 56 en 12 nucleos no lo es. Ni siquiera los CICLOS se salvan —a carga alta el mismo
# trabajo gasta mas ciclos, porque los fallos de cache se pagan en ciclos parados.
#
# COMO SE HACE AQUI:
#   - se aborta si la carga de salida pasa de 8. Un guion que mide igual con la maquina llena
#     no es un instrumento, es un generador de numeros.
#   - EN SERIE, alternando configuraciones, para que una deriva de carga reparta el daño en
#     vez de caer entera sobre una de las dos.
#   - todo pasa por bench/mide.sh, que deja la fila en bench/historia.jsonl con su carga y su
#     commit. Nada vive en /tmp: esta mañana /tmp se comio la muestra sub600 y cuatro horas
#     de medidas se salvaron por los pelos.
#   - se comprueba la IDA Y VUELTA de cada archivo. Una configuracion que produce un archivo
#     roto en la mitad de tiempo no es un ahorro, y midiendo solo segundos parece que si.
#   - el desglose por fases (VC_FASES) se guarda, que hasta hoy la bascula lo tiraba.
set -uo pipefail
cd "$(dirname "$0")/.."

IN=/tmp/compresor-f1/corpus
VC=$PWD/target/release/vcompresor
VUELTAS=${VUELTAS:-2}
NIVEL=${NIVEL:-9}          # el mismo nivel de F3.1, para que sea comparable con lo publicado

CARGA=$(awk '{print $1}' /proc/loadavg)
if (( $(echo "$CARGA > 8" | bc -l) )); then
  echo "ABORTA: carga $CARGA. Esta medida existe precisamente porque las anteriores se"
  echo "        tomaron con la maquina llena. Espera a que baje."
  exit 3
fi
[ -d "$IN" ] || { echo "ABORTA: no esta $IN"; exit 3; }

W=$(mktemp -d /tmp/abl.XXXXXX)          # unico: dos instancias no pueden pisarse
REG=bench/registros/fases
mkdir -p "$REG"
echo "arbol: $(find $IN -type f | wc -l) ficheros · carga de salida: $CARGA · nivel $NIVEL · $VUELTAS vueltas"
echo

# LA TRAMPA QUE ESTA VERSION ESQUIVA. La primera version usaba "" para «todo encendido», que es
# lo natural: sin banderas, la configuracion por defecto. Y la primera medida delato el fallo —
# salio 1.055.963.286 bytes, que es EXACTAMENTE lo que daba --no-group antes—. Motivo: esta
# mañana se cambio el defecto y agrupar ya viene APAGADO. Asi que "" no era «todo encendido»
# sino «todo menos agrupar», la ablacion de agrupar habria comparado la misma configuracion
# consigo misma, y el guion habria publicado tranquilamente que agrupar cuesta 0,00%.
# Un guion correcto, un resultado creible, y una conclusion inventada. Por eso «completo» pide
# --group EXPLICITO: lo que se mide no puede depender de cual sea el defecto de hoy.
for v in $(seq 1 "$VUELTAS"); do
  for C in "--group" "" "--group --no-jpeg"; do
    case "$C" in
      "--group")            ETI="completo"   ;;
      "")                   ETI="--no-group" ;;   # el defecto de hoy YA es sin agrupar
      "--group --no-jpeg")  ETI="--no-jpeg"  ;;
    esac
    rm -f "$W/o.vc"
    VC_FASES=1 bench/mide.sh \
      -e "F4.1 ablacion arbol maquina quieta: $ETI (vuelta $v)" \
      -m arbol2.7g -c "$ETI" \
      -n "repeticion de F3.1 con carga baja: aquella se tomo a carga 41-56" \
      -s "$REG/F41-$(echo "$ETI" | tr -d ' -')-v$v.txt" \
      -- "$VC" compress "$IN" -o "$W/o.vc" --level "$NIVEL" $C || { echo "!! fallo, se para"; break 2; }
    # Una configuracion rapida que produce un archivo roto no es un ahorro. Se comprueba.
    if ! "$VC" verify "$W/o.vc" >/dev/null 2>&1; then
      echo "!! IDA Y VUELTA ROTA en «$ETI» — la medida de velocidad no vale nada"; break 2
    fi
    echo "   carga ahora: $(awk '{print $1}' /proc/loadavg)"
  done
done
rm -rf "$W"

echo
echo "=== lo que dice el registro (que es lo que sobrevive) ==="
python3 - <<'PY'
import json, collections
G = collections.defaultdict(list)
for l in open("bench/historia.jsonl"):
    f = json.loads(l)
    if f["etiqueta"].startswith("F4.1 ablacion arbol"):
        G[f["config"]].append(f)
if not G:
    print("  sin filas F4 todavia"); raise SystemExit
base = G.get("completo", [])
def med(rs, k): return sum(r[k] for r in rs) / len(rs)
print(f"  {'config':<12} {'ciclos (G)':>12} {'reloj (s)':>10} {'bytes':>12} {'carga':>7}  n")
for c, rs in G.items():
    print(f"  {c:<12} {med(rs,'ciclos')/1e9:12.1f} {med(rs,'seg'):10.1f} "
          f"{int(med(rs,'bytes_out')):12d} {med(rs,'carga'):7.1f}  {len(rs)}")
if base:
    b_c, b_b = med(base, "ciclos"), med(base, "bytes_out")
    # Si dos configuraciones sacan EXACTAMENTE los mismos bytes, no son dos configuraciones:
    # la bandera no llego a hacer nada y la «ablacion» se esta comparando consigo misma. Paso
    # de verdad hace un rato, por un cambio de defecto. Sin este aviso, el resultado —0,00%—
    # es perfectamente creible y perfectamente falso.
    for c, rs in G.items():
        if c != "completo" and int(med(rs, "bytes_out")) == int(b_b):
            print(f"  !! «{c}» saca los MISMOS bytes que «completo»: la bandera no hizo nada.")
            print( "     La ablacion no mide la pieza, se mide a si misma. NO publicar.")
    print()
    for c, rs in G.items():
        if c == "completo": continue
        cc, bb = med(rs, "ciclos"), med(rs, "bytes_out")
        pieza = c.replace("--no-", "")
        print(f"  {pieza:<10} cuesta {100*(b_c-cc)/b_c:5.2f}% de los ciclos "
              f"y compra {100*(bb-b_b)/b_b:5.3f}% de los bytes")
    print()
    if len(base) > 1:
        cs = [r["ciclos"] for r in base]
        print(f"  ruido del propio banco: el completo repetido {len(cs)} veces varia "
              f"{100*(max(cs)-min(cs))/min(cs):.2f}% en ciclos")
PY
echo FIN-ABLACION
