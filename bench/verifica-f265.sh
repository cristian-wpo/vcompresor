#!/usr/bin/env bash
# VERIFICACION DE F2.65 — la carga util de la transformada se escribe una vez.
#
# Lo que hay que probar, y por que cada cosa:
#  1. QUE BAJE el coste por instantanea, en los tres corpus. Es la razon de existir del cambio.
#  2. QUE NO CREZCA el archivo. F2.64 arreglaba lo mismo y crecia un 5,87%; si este tambien
#     crece, tampoco vale.
#  3. IDA Y VUELTA con VARIAS instantaneas. La etiqueta 7 solo se dispara cuando hay
#     repeticion, asi que una prueba de una sola copia no la ejercitaria: pasaria en verde sin
#     haber tocado el codigo nuevo.
#  4. QUE LOS ARCHIVOS VIEJOS SE SIGAN ABRIENDO, con el binario ANTERIOR construido de verdad y
#     afirmando la premisa antes de comparar (bench/verifica-compatibilidad.sh).
#  5. FUZZ.
set -u
export PATH="$HOME/.cargo/bin:$PATH"
R="$(cd "$(dirname "$0")/.." && pwd)"
cd $R
NUEVO=$R/target/release/vcompresor
W=/tmp/v265; rm -rf $W; mkdir -p $W

echo "=== binario anterior, en un arbol aparte ==="
# 🚨 EL CERROJO VA ANTES DEL `rm -rf`, Y NO ES UN DETALLE DE ORDEN.
#
# Este guion borraba su directorio de trabajo SIN coger el cerrojo del reloj. Si otra medida
# estaba usando ese arbol —o simplemente corriendo—, este `rm -rf` se lo llevaba por debajo: la
# otra medida no falla, TERMINA Y PUBLICA UN NUMERO, con la mitad de su entrada desaparecida.
# Un fallo que se ve es barato; este produce una cifra creible y equivocada.
#
# Encontrado el 02-08 preparando las pruebas de escala, sin ejecutarlo.
. "$(dirname "$0")/comun.sh" 2>/dev/null || true
if declare -f toma_reloj >/dev/null 2>&1; then
  toma_reloj || { echo "  hay una medida en marcha: no se borra nada y se sale."; exit 1; }
fi
rm -rf /tmp/oldvc && cp -r $R /tmp/oldvc && rm -rf /tmp/oldvc/target
cp /tmp/f265.f.bak /tmp/oldvc/crates/vc-core/src/formato.rs
grep -q "e.byte(7); e.var(j);" $R/crates/vc-core/src/formato.rs \
  || { echo "!! el arbol NUEVO no tiene la etiqueta 7. ABORTA"; exit 1; }
grep -q "e.byte(7); e.var(j);" /tmp/oldvc/crates/vc-core/src/formato.rs \
  && { echo "!! el arbol VIEJO ya tiene la etiqueta 7: la prueba no valdria. ABORTA"; exit 1; }
echo "   premisa verificada: el viejo no la tiene, el nuevo si"
( cd /tmp/oldvc && cargo build --release > /tmp/oldbuild.log 2>&1 ) \
  || { echo "!! el binario viejo no compila"; tail -5 /tmp/oldbuild.log; exit 1; }
VIEJO=/tmp/oldvc/target/release/vcompresor
echo

pos_de() { python3 -c "
import struct
f=open('$1','rb'); f.seek(-64,2); print(struct.unpack('<Q', f.read(8))[0])"; }

prep() { rm -rf $W/in; mkdir -p $W/in; local n=0
  while IFS= read -r -d "" f; do cp "$f" "$W/in/$(printf '%05d' $n)" 2>/dev/null && n=$((n+1)); [ $n -ge $2 ] && break
  done < <(find "$1" -type f -print0 2>/dev/null); }

printf '%-15s %13s %8s %13s %13s %13s\n' corpus archivo "vs base" indice "+instant.1" "+instant.2"
corpus() {
  local ETQ="$1" DIR="$2" LIM="$3" BASE="$4" BINST="$5"
  prep "$DIR" "$LIM"
  rm -f $W/n.vc
  bench/mide.sh -e "F2.65 carga util una vez: $ETQ" -m "$ETQ" -c "nivel 19" -- \
    $NUEVO compress $W/in -o $W/n.vc --level 19 --name d0 >/dev/null 2>&1
  local T=$(stat -c%s $W/n.vc) P=$(pos_de $W/n.vc)
  # DOS instantaneas mas: la primera crea las referencias, la segunda comprueba que se
  # mantienen cuando el indice ya venia con referencias dentro.
  $NUEVO add $W/n.vc $W/in --name d1 >/dev/null 2>&1; local T2=$(stat -c%s $W/n.vc)
  $NUEVO add $W/n.vc $W/in --name d2 >/dev/null 2>&1; local T3=$(stat -c%s $W/n.vc)
  printf '%-15s %13s %7s%% %13s %13s %13s\n' "$ETQ" "$T" \
    "$(echo "scale=2;($T-$BASE)*100/$BASE"|bc)" "$((T-P))" "$((T2-T))" "$((T3-T2))"
  echo "                (antes era +$BINST por instantanea)"

  rm -rf $W/o1 && $NUEVO extract $W/n.vc -o $W/o1 >/dev/null 2>&1
  local D=0
  for snap in d0 d1 d2; do
    for f in $(ls $W/in); do cmp -s "$W/in/$f" "$W/o1/$snap/$f" || D=$((D+1)); done
  done
  local NEX=$(find $W/o1 -type f | wc -l)
  # un archivo VIEJO leido por el NUEVO
  rm -f $W/v.vc; $VIEJO compress $W/in -o $W/v.vc --level 19 --name d0 >/dev/null 2>&1
  rm -rf $W/o2 && $NUEVO extract $W/v.vc -o $W/o2 >/dev/null 2>&1
  local D2=0; for f in $(ls $W/in); do cmp -s "$W/in/$f" "$W/o2/d0/$f" || D2=$((D2+1)); done
  echo "                3 instantaneas: $NEX ficheros extraidos, $D diferencias  ·  archivo VIEJO: $D2 diferencias"
}

corpus "pdfoffice"     /tmp/compresor-f1/corpus/pdfoffice  500 77091115 8664994
corpus "png-fireworks" /tmp/compresor-f1/corpus/png        400 25455219 14441264
corpus "png-normal"    /tmp/compresor-f1/corpus/png-normal 800 151501612 2004

echo
echo "=== fuzz ==="
[ -x target/release/vc-fuzz ] && target/release/vc-fuzz 60 200 2>&1 | tail -3 || echo "   (sin binario de fuzz)"
rm -rf $W/in $W/o1 $W/o2
echo FIN-V265-CENTINELA
