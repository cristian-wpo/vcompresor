#!/usr/bin/env bash
# VERIFICACION DE F2.64 — el esqueleto por trozos.
#
# Tres cosas, y ninguna se da por buena sin verla:
#   1. IDA Y VUELTA en los tres corpus. Un archivador que no devuelve el byte exacto no es nada.
#   2. LOS ARCHIVOS VIEJOS SE SIGUEN ABRIENDO. Y para eso hay que construir el binario ANTERIOR
#      de verdad, no suponer que un archivo es viejo: hoy mismo un `cargo build | head` mato a
#      cargo con SIGPIPE, dejo el binario anterior en su sitio y la prueba de compatibilidad
#      dio verde comparando el binario viejo consigo mismo. La prueba AFIRMA SU PROPIA PREMISA.
#   3. QUE MEJORE, y en los DOS corpus de PNG mas el de oficina. Si solo mejora uno, el arreglo
#      esta mal planteado.
set -u
export PATH="$HOME/.cargo/bin:$PATH"
R="$(cd "$(dirname "$0")/.." && pwd)"
cd $R
NUEVO=$R/target/release/vcompresor
W=/tmp/v264; rm -rf $W; mkdir -p $W

# ---------- el binario ANTERIOR, construido aparte ----------------------------------------------
echo "=== construyendo el binario anterior en un arbol aparte ==="
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
cp /tmp/f264.l.bak  /tmp/oldvc/crates/vc-core/src/lib.rs
cp /tmp/f264.f.bak  /tmp/oldvc/crates/vc-core/src/formato.rs
cp /tmp/f264.fu.bak /tmp/oldvc/crates/vc-cli/src/fusefs.rs
# PREMISA: el viejo tiene el esqueleto crudo y el nuevo no. Si esto no se cumple, la prueba de
# compatibilidad no prueba nada y hay que parar.
grep -q "Deflate { esqueleto: Vec<u8>, huecos" /tmp/oldvc/crates/vc-core/src/lib.rs \
  || { echo "!! el arbol viejo NO tiene la forma vieja: la prueba no valdria. ABORTA"; exit 1; }
grep -q "Deflate { esq: Vec<u64>" $R/crates/vc-core/src/lib.rs \
  || { echo "!! el arbol nuevo NO tiene la forma nueva. ABORTA"; exit 1; }
echo "   premisa verificada: viejo=esqueleto crudo, nuevo=esqueleto por trozos"
( cd /tmp/oldvc && cargo build --release > /tmp/oldbuild.log 2>&1 ) \
  || { echo "!! el binario viejo no compila"; tail -5 /tmp/oldbuild.log; exit 1; }
VIEJO=/tmp/oldvc/target/release/vcompresor
echo "   binario anterior listo"
echo

pos_de() { python3 -c "
import struct
f=open('$1','rb'); f.seek(-64,2); print(struct.unpack('<Q', f.read(8))[0])"; }

prep() {  # aplana un corpus a $W/in
  rm -rf $W/in; mkdir -p $W/in; local n=0
  while IFS= read -r -d "" f; do
    cp "$f" "$W/in/$(printf '%05d' $n)" 2>/dev/null && n=$((n+1))
    [ $n -ge $2 ] && break
  done < <(find "$1" -type f -print0 2>/dev/null)
}

printf '%-16s %13s %13s %12s %12s %13s\n' corpus archivo frames indice "+instant." "vs base"
echo "corpus	archivo	frames	indice	mas_instant	rt_nuevo	rt_viejo_con_nuevo" > $W/r.tsv

corpus() {
  local ETQ="$1" DIR="$2" LIM="$3" BASE="$4"
  prep "$DIR" "$LIM"

  # --- lo NUEVO ---
  rm -f $W/n.vc
  bench/mide.sh -e "F2.64 esqueleto por trozos: $ETQ" -m "$ETQ" -c "nivel 19" -- \
    $NUEVO compress $W/in -o $W/n.vc --level 19 --name d0 >/dev/null 2>&1
  local T=$(stat -c%s $W/n.vc) P=$(pos_de $W/n.vc)
  $NUEVO add $W/n.vc $W/in --name d1 >/dev/null 2>&1
  local T2=$(stat -c%s $W/n.vc)
  printf '%-16s %13s %13s %12s %12s %13s\n' "$ETQ" "$T" "$P" "$((T-P))" "$((T2-T))" \
    "$(echo "scale=2;($T-$BASE)*100/$BASE"|bc)%"

  # --- ida y vuelta de lo nuevo (dos instantaneas) ---
  rm -rf $W/o1 && $NUEVO extract $W/n.vc -o $W/o1 >/dev/null 2>&1
  local D1=0; for f in $(ls $W/in); do cmp -s "$W/in/$f" "$W/o1/d1/$f" || D1=$((D1+1)); done

  # --- un archivo VIEJO, escrito por el binario anterior, leido por el NUEVO ---
  rm -f $W/v.vc
  $VIEJO compress $W/in -o $W/v.vc --level 19 --name d0 >/dev/null 2>&1
  rm -rf $W/o2 && $NUEVO extract $W/v.vc -o $W/o2 >/dev/null 2>&1
  local D2=0; for f in $(ls $W/in); do cmp -s "$W/in/$f" "$W/o2/d0/$f" || D2=$((D2+1)); done

  printf '%-16s   ida y vuelta nuevo: %s dif   ·   archivo VIEJO leido por el nuevo: %s dif\n' \
    "" "$D1" "$D2"
  echo "$ETQ	$T	$P	$((T-P))	$((T2-T))	$D1	$D2" >> $W/r.tsv
}

corpus "png-normal"    /tmp/compresor-f1/corpus/png-normal 800 151501612
corpus "png-fireworks" /tmp/compresor-f1/corpus/png        400 25455219
corpus "pdfoffice"     /tmp/compresor-f1/corpus/pdfoffice  500 77091115

echo
echo "=== fuzz (la red que pilla lo que las pruebas dirigidas no) ==="
if [ -x target/release/vc-fuzz ]; then
  target/release/vc-fuzz 60 200 2>&1 | tail -4
else
  echo "   (no hay binario de fuzz construido)"
fi
rm -rf $W/in $W/o1 $W/o2
echo FIN-V264-CENTINELA
