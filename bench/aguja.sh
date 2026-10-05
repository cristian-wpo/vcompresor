#!/usr/bin/env bash
# LA AGUJA. La misma bateria de medidas, siempre igual, una instantanea por commit.
#
#   bench/aguja.sh            # -> una linea en bench/aguja.jsonl
#
# EL PROBLEMA QUE RESUELVE. El banco medía cosas sueltas: cada experimento con su muestra, su
# formato y su momento. Sirve para decidir un cambio concreto y no sirve para lo otro —ver si
# ese cambio movio la aguja EN TODO LO DEMAS—. Hoy mismo: el carril ZIP mejoro el ratio un 13%
# y nadie miro si empeoro los .gz, ni cuanto subio la memoria de descompresion. Se descubrio
# de casualidad, dos vueltas despues.
#
# Aqui las metricas son FIJAS y se comparan contra la instantanea anterior. Un cambio que
# mejora una y empeora tres se ve en la misma pantalla.
#
# POR QUE TARDA SEIS MINUTOS Y NO VEINTE. Las medidas de RATIO son deterministas —misma
# entrada y misma config dan los mismos bytes— asi que pueden correr en PARALELO sin falsearse.
# Solo las de ciclos y memoria necesitan serie. Una bateria que tarda veinte minutos no se
# ejecuta nunca, y una que no se ejecuta no mide nada.
set -u
cd "$(dirname "$0")/.."
S=/tmp/compresor-f1/samples
VC=./target/release/vcompresor
W=/tmp/aguja; rm -rf $W; mkdir -p $W
COMMIT=$(git rev-parse --short HEAD 2>/dev/null || echo "?")
SUCIO=$(git status --porcelain 2>/dev/null | head -1 | wc -l)
ETIQ=${1:-}

echo "aguja · commit $COMMIT$([ "$SUCIO" = 1 ] && echo ' (arbol sucio)')"

# ---- 1. RATIO en las cinco muestras. En paralelo: la salida es determinista. ----------------
for m in sql logs csvjson codigo enwik8; do
  ( nice -n 5 $VC comprimir $S/$m.10m -o $W/$m.vc --max >/dev/null 2>&1 ) &
done
wait
TOT=0
for m in sql logs csvjson codigo enwik8; do
  eval "R_$m=$(stat -c%s $W/$m.vc)"; TOT=$((TOT + $(stat -c%s $W/$m.vc)))
done
echo "   ratio 5 muestras: $TOT"

# ---- 2. OFIMATICA: 20 documentos con imagenes dentro, que es donde vive el carril ZIP -------
mkdir -p $W/of
n=0
while IFS= read -r -d "" f; do
  [ "$(head -c2 "$f" | xxd -p)" = "504b" ] || continue
  n=$((n+1)); [ $n -gt 20 ] && break
  cp "$f" "$W/of/$(printf '%03d' $n).zipx"
done < <(find /tmp/compresor-f1/corpus/pdfoffice -type f -print0 2>/dev/null)
$VC comprimir $W/of -o $W/of.vc --max >/dev/null 2>&1
OF=$(stat -c%s $W/of.vc 2>/dev/null || echo 0)
echo "   ofimatica (20 docx): $OF"

# ---- 3. IDA Y VUELTA del mismo archivo. Lo primero que hay que saber, no lo ultimo. ---------
rm -rf $W/of-out
$VC descomprimir $W/of.vc -o $W/of-out >/dev/null 2>&1
RT=1
for f in $W/of/*; do
  g=$(find $W/of-out -type f -name "$(basename "$f")" | head -1)
  { [ -n "$g" ] && cmp -s "$f" "$g"; } || RT=0
done
echo "   ida y vuelta: $([ $RT = 1 ] && echo OK || echo MAL)"

# ---- 4. DETERMINISMO: dos veces la misma entrada -> ¿los mismos bytes? ----------------------
mkdir -p $W/det && head -c 900000 $S/codigo.10m > $W/det/a.txt
$VC comprimir $W/det -o $W/d1.vc --max >/dev/null 2>&1
$VC comprimir $W/det -o $W/d2.vc --max >/dev/null 2>&1
DET=$(cmp -s $W/d1.vc $W/d2.vc && echo 1 || echo 0)
echo "   determinista: $DET"

# ---- 5. COMPRIMIR vs DESCOMPRIMIR: tiempo y memoria. EN SERIE, que aqui si importa. --------
T=$(mktemp)
/usr/bin/time -v -o $T $VC comprimir $W/det -o $W/c.vc --max >/dev/null 2>&1
CS=$(awk -F': ' '/Elapsed \(wall/{print $NF}' $T | awk -F: '{if(NF==2)print $1*60+$2; else print $1}')
CM=$(awk '/Maximum resident/{print $NF}' $T)
/usr/bin/time -v -o $T $VC descomprimir $W/c.vc -o $W/cout >/dev/null 2>&1
DS=$(awk -F': ' '/Elapsed \(wall/{print $NF}' $T | awk -F: '{if(NF==2)print $1*60+$2; else print $1}')
DM=$(awk '/Maximum resident/{print $NF}' $T)
rm -f $T
echo "   comprimir ${CS}s ${CM}KB · descomprimir ${DS}s ${DM}KB"

# ---- 6. FUZZ: la garantia de que nada de lo anterior vale si esto falla ---------------------
FZ=$(./target/release/vc-fuzz 60 200 1 2>&1 | tail -2 | tr -d '\n')
FOK=$(echo "$FZ" | grep -c "FUZZ OK")
echo "   fuzz: $([ "$FOK" = 1 ] && echo OK || echo MAL)"

# ---- 7. VELOCIDAD SOBRE MUCHOS FICHEROS, con el desglose por fases -------------------------
# La medida de velocidad de arriba se hace sobre UN fichero de 900 KB, y ahi todo lo que se paga
# POR FICHERO es invisible: el stat, el enrutado, la firma, los metadatos, la entrada del indice.
# Es el mismo error que tenia la tabla del dinero. Un arbol de muchos ficheros pequeños lo ve.
mkdir -p $W/muchos
find /tmp/compresor-f1/corpus -type f 2>/dev/null | sort | awk 'NR%20==0' > $W/lista.txt
while IFS= read -r f; do
  d=$W/muchos/$(dirname "${f#/tmp/compresor-f1/corpus/}"); mkdir -p "$d"; cp -a "$f" "$d/" 2>/dev/null
done < $W/lista.txt
NF=$(find $W/muchos -type f 2>/dev/null | wc -l)
NB=$(du -sb $W/muchos 2>/dev/null | cut -f1)
if [ "${NF:-0}" -gt 100 ]; then
  T0=$(date +%s%N)
  VC_FASES=1 $VC compress $W/muchos -o $W/m.vc --level 19 >/dev/null 2>$W/fases.txt
  MS=$(echo "scale=3; ($(date +%s%N) - $T0)/1000000000" | bc)
  MBS=$(echo "scale=2; $NB/1048576/$MS" | bc)
  echo "   muchos ficheros: $NF ficheros, $NB B -> $MBS MB/s"
else
  NF=0; NB=0; MBS=0; : > $W/fases.txt
  echo "   muchos ficheros: SIN CORPUS (no esta /tmp/compresor-f1/corpus)"
fi

# ---- 8. EL ARBOL DE DEPENDENCIAS. Bajo de 60 a 33 en F2.7 y hoy son 59. ---------------------
DEPS=$(cargo tree --edges normal 2>/dev/null | grep -oE "[a-z0-9_-]+ v[0-9][0-9.]*" | sort -u | wc -l)
echo "   dependencias: $DEPS crates"

python3 - <<PY
import json, time, os, re
z = 79761241  # zpaq -m5 sobre el corpus pdfoffice completo, de la cache de rivales
f = {
  "t": time.strftime("%Y-%m-%dT%H:%M:%S"), "commit": "$COMMIT", "sucio": $SUCIO == 1,
  "etiqueta": """$ETIQ""",
  "sql": $R_sql, "logs": $R_logs, "csvjson": $R_csvjson,
  "codigo": $R_codigo, "enwik8": $R_enwik8, "total5": $TOT,
  "ofimatica": $OF,
  "ida_vuelta": $RT == 1, "determinista": $DET == 1, "fuzz": $FOK == 1,
  "comp_seg": float("$CS" or 0), "comp_mb": int("$CM" or 0)//1024,
  "desc_seg": float("$DS" or 0), "desc_mb": int("$DM" or 0)//1024,
  "muchos_n": int("$NF" or 0), "muchos_b": int("$NB" or 0), "muchos_mbs": float("$MBS" or 0),
  "deps": int("$DEPS" or 0),
}

# LAS FASES, en el orden del camino. Es lo que convierte «hemos perdido un 12%» en «hemos
# perdido un 12% y esta en enrutar».
fases = {}
try:
    for l in open("$W/fases.txt", encoding="utf-8", errors="replace"):
        m = re.match(r"^(\S[\w+]*)\s+([\d.]+)\s+([\d.]+)%\s+(\d+)\s+([\d.]+)$", l.strip())
        if m: fases[m.group(1)] = round(float(m.group(2)), 3)
except OSError:
    pass
f["fases"] = fases
p = "bench/aguja.jsonl"
prev = None
if os.path.exists(p):
    ls = [l for l in open(p) if l.strip()]
    if ls: prev = json.loads(ls[-1])
open(p, "a").write(json.dumps(f, ensure_ascii=False) + "\n")

print()
print(f"{'metrica':<16} {'ahora':>12} {'antes':>12} {'aguja':>10}")
print("-" * 54)
for k in ("total5","sql","logs","csvjson","codigo","enwik8","ofimatica",
          "comp_seg","comp_mb","desc_seg","desc_mb","muchos_mbs","deps"):
    a = f[k]; b = prev.get(k) if prev else None
    d = "" if not b else f"{(a-b)*100/b:+.3f}%"
    aviso = ""
    if b and k.startswith(("total","sql","logs","csv","codigo","enwik","ofim")):
        aviso = "  <- MEJOR" if a < b else ("  <- PEOR" if a > b else "")
    print(f"{k:<16} {a:>12} {str(b or '-'):>12} {d:>10}{aviso}")
print()
for k in ("ida_vuelta","determinista","fuzz"):
    print(f"{k:<16} {'OK' if f[k] else '!! MAL'}")
if not (f["ida_vuelta"] and f["determinista"] and f["fuzz"]):
    print("\\n!! HAY UNA GARANTIA ROTA. Ninguna cifra de ratio de arriba cuenta.")

# ---------------- LA PRIMERA PERDIDA DE VELOCIDAD, Y POR QUE ----------------
# Se señala LA PRIMERA fase del camino que empeora, no la que mas empeora. Que sea la primera
# importa: una fase tardia puede crecer porque la anterior le pasa mas trabajo, y arreglar la
# tardia seria arreglar el sintoma.
ORDEN = ["recorrer","agrupar","stat","enrutar","jxl","transformar",
         "leer+trocear","hash","dedup+copiar","esperar","meta+xattr"]
UMBRAL = 5.0        # por debajo de esto es ruido del reloj, no una perdida
if prev and f.get("fases") and prev.get("fases"):
    a, b = f["fases"], prev["fases"]
    vel_a, vel_b = f.get("muchos_mbs") or 0, prev.get("muchos_mbs") or 0
    print()
    if vel_b and vel_a:
        d = (vel_a - vel_b) * 100 / vel_b
        print(f"VELOCIDAD sobre muchos ficheros: {vel_a:.2f} MB/s contra {vel_b:.2f} ({d:+.1f}%)")
    culpables = []
    for k in ORDEN:
        if k in a and k in b and b[k] > 0.05:
            d = (a[k] - b[k]) * 100 / b[k]
            if d > UMBRAL: culpables.append((k, b[k], a[k], d))
    if culpables:
        k, antes, ahora, d = culpables[0]
        print(f"PRIMERA FASE QUE EMPEORA: «{k}»  {antes:.2f}s -> {ahora:.2f}s  ({d:+.1f}%)")
        if len(culpables) > 1:
            print("  despues tambien: " + ", ".join(
                f"{k} ({d:+.0f}%)" for k, _, _, d in culpables[1:]))
        print("  Es LA PRIMERA del camino, no la mayor: una fase tardia puede estar creciendo")
        print("  porque la anterior le pasa mas trabajo.")
    else:
        print("Ninguna fase empeora por encima del %.0f%% de ruido." % UMBRAL)
    if f.get("deps") and prev.get("deps") and f["deps"] > prev["deps"]:
        print(f"!! EL ARBOL DE DEPENDENCIAS HA CRECIDO: {prev['deps']} -> {f['deps']} crates.")
elif f.get("fases"):
    print()
    print("Primera instantanea con fases; desde la proxima se podra decir cual empeora.")
    for k in ORDEN:
        if k in f["fases"]: print(f"   {k:<14}{f['fases'][k]:>9.2f}s")
PY
rm -rf $W
echo FIN-AGUJA-CENTINELA
