#!/usr/bin/env bash
# ¿CUAL ES EL SUELO FISICO DE NUESTRO INFLATE, Y A CUANTO ESTAMOS DE EL?
#
# La pregunta del dueño: reducir el CPU al minimo matematica y fisicamente posible, SIN
# SUPOSICIONES. Entonces el suelo no se estima: se mide en ESTA maquina.
#
# HAY DOS SUELOS Y MANDA EL MAYOR:
#
#   FISICO      el plano hay que ESCRIBIRLO. 6.072.611.093 bytes de salida tienen que pasar
#               por el bus de memoria pase lo que pase, y las copias ademas los releen. Ese
#               suelo lo pone el ancho de banda de esta maquina, que se mide aqui abajo.
#
#   ALGORITMICO cada simbolo hay que decodificarlo. Son 257.576.834 simbolos —contados, no
#               estimados, por bin/vc-cobertura— y ninguno se puede resolver en menos de una
#               busqueda. Ese suelo lo pone la latencia de una tabla que quepa en cache.
#
# Lo que NO es suelo: el metodo bit a bit que usamos hoy. Es una DECISION —esta escrita en la
# cabecera del modulo: «el mas lento y el mas facil de demostrar correcto»— y por tanto es
# exactamente el margen que queda por recuperar.
#
# Medida de tiempo: EN SERIE, aborta si la maquina no esta quieta.
set -uo pipefail
cd "$(dirname "$0")/.."
C=$(awk '{print $1}' /proc/loadavg)
if (( $(echo "$C > 8" | bc -l) )); then echo "ABORTA: carga $C, esto es tiempo"; exit 3; fi
echo "carga: $C"
echo

# ── EL SUELO FISICO: cuanto tarda esta maquina en escribir 6,07 GB ──────────────────────────
cat > /tmp/ancho.c <<'EOF'
// Mide el ancho de banda de escritura y de copia de ESTA maquina. Sin suposiciones sobre el
// modelo de CPU: se cronometra lo que hace.
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
static double ahora(void){struct timespec t;clock_gettime(CLOCK_MONOTONIC,&t);return t.tv_sec+t.tv_nsec/1e9;}
// EL SUMIDERO. Sin esto el compilador ve que nadie lee `b` y borra el bucle de memcpy
// entero: la primera version de esta prueba daba 107.248.512 GB/s, que es exactamente la
// velocidad de no hacer nada. Es volatile y la funcion es noinline para que no pueda
// demostrar que las escrituras sobran.
static volatile unsigned long sumidero;
__attribute__((noinline)) static void toca(const char *p, size_t n){
  sumidero += (unsigned char)p[0] + (unsigned char)p[n/2] + (unsigned char)p[n-1];
}
// Un ancho de banda por encima de esto no existe en un solo hilo en ninguna maquina real: si
// sale, es que se ha medido el vacio. La prueba tiene que saber reconocerlo.
#define IMPOSIBLE 200.0
int main(void){
  size_t n = 512ull<<20;               // 512 MiB, muy por encima de cualquier cache
  char *a = malloc(n), *b = malloc(n);
  if(!a||!b){puts("sin memoria");return 1;}
  memset(a,1,n); memset(b,2,n);        // tocarlas para que las paginas existan de verdad
  double t0=ahora(); for(int i=0;i<4;i++){ memset(a,i+1,n); toca(a,n); } double t1=ahora();
  double t2=ahora(); for(int i=0;i<4;i++){ memcpy(b,a,n); toca(b,n); } double t3=ahora();
  double gb = 4.0*n/1e9;
  double esc = gb/(t1-t0), cop = gb/(t3-t2);
  printf("  escribir (memset): %6.1f GB/s\n", esc);
  printf("  copiar   (memcpy): %6.1f GB/s  (lee y escribe: %.1f GB/s de trafico)\n", cop, 2*cop);
  if(esc > IMPOSIBLE || cop > IMPOSIBLE){
    printf("  *** ABORTA: %0.f GB/s en un hilo no existe. El compilador se ha comido el bucle\n",
           esc > cop ? esc : cop);
    printf("  *** y esto habria acabado en una tabla como si fuera una medida.\n");
    return 4;
  }
  return 0;
}
EOF
gcc -O2 -o /tmp/ancho /tmp/ancho.c 2>/dev/null || { echo "sin gcc"; exit 3; }
echo "ANCHO DE BANDA DE ESTA MAQUINA (un solo hilo):"
/tmp/ancho | tee /tmp/ancho.txt
if grep -q "ABORTA" /tmp/ancho.txt; then
  echo "El ancho de banda no se ha podido medir: NO se escribe bench/suelo.tsv." >&2
  exit 4
fi
ESCR=$(awk '/memset/{print $3}' /tmp/ancho.txt)
COPI=$(awk '/memcpy/{print $3}' /tmp/ancho.txt)
echo

# ── EL CENSO EXACTO, del propio programa ────────────────────────────────────────────────────
# DOS EJECUCIONES, y no es despiste. Los contadores de VC_INFLATE_CUENTA cuestan tiempo -16,3 s
# contra 12,2 s en la misma maquina-, asi que no pueden estar puestos cuando se miden ciclos.
# Una vuelta CON contadores da el censo; otra SIN ellos, bajo perf, da el coste. Mezclarlas
# seria medir el instrumento.
echo "EL CENSO, contado por el propio programa:"
VC_INFLATE_CUENTA=1 ./target/release/vc-inflate > /tmp/censo.txt 2>&1
lee() { awk -v pat="$1" '$0 ~ pat {for(i=1;i<=NF;i++) if($i ~ /^[0-9]+$/ && $i+0>1000) {print $i; exit}}' /tmp/censo.txt; }
SIM=$(lee "simbolos decodificados")
SAL=$(lee "bytes de plano producidos")
COP=$(lee "copiados .coincidencias")
NCOP=$(lee "de ellos copias")
if [ -z "$SIM" ] || [ -z "$SAL" ] || [ -z "$COP" ]; then
  echo "ABORTA: no se ha podido leer el censo del programa. NO se escribe la tabla." >&2; exit 5
fi
echo "  simbolos $SIM · salida $SAL B · copiados $COP B · copias $NCOP"
echo

# ── LO QUE TARDAMOS HOY, y en que se va ─────────────────────────────────────────────────────
echo "NUESTRO INFLATE, con perf (sin contadores):"
T0=$(python3 -c 'import time;print(time.monotonic())')
if perf stat -x, -e cycles:u,instructions:u -o /tmp/pf.txt ./target/release/vc-inflate >/tmp/inf.log 2>&1; then
  T1=$(python3 -c 'import time;print(time.monotonic())')
  grep -E "MiB comprimidos" /tmp/inf.log | sed 's/^/  /'
  CIC=$(awk -F, '/cycles/{print $1;exit}' /tmp/pf.txt)
  INS=$(awk -F, '/instructions/{print $1;exit}' /tmp/pf.txt)
  echo "  ciclos: $CIC   instrucciones: $INS"
  # Y AQUI SE ESCRIBE LA TABLA. Todo lo que el panel diga de vdeflate sale de este fichero:
  # ninguna cifra se vuelve a copiar a mano a un .py.
  python3 - "$CIC" "$INS" "$T0" "$T1" "$ESCR" "$COPI" "$SIM" "$SAL" "$COP" "$NCOP" <<'PY'
import sys, csv, subprocess, datetime
cic, ins = int(sys.argv[1]), int(sys.argv[2])
seg = float(sys.argv[4]) - float(sys.argv[3])
escr, copi = float(sys.argv[5]), float(sys.argv[6])
# NINGUNA constante aqui: las cuatro las acaba de contar el programa en la vuelta anterior.
SIM, SAL, COP, NCOP = (int(sys.argv[i]) for i in (7, 8, 9, 10))
print()
print(f"  {'':22} {'por SIMBOLO':>14} {'por BYTE de salida':>20}")
print(f"  {'ciclos':22} {cic/SIM:14.1f} {cic/SAL:20.2f}")
print(f"  {'instrucciones':22} {ins/SIM:14.1f} {ins/SAL:20.2f}")
print(f"  IPC: {ins/cic:.2f}")
# EL SUELO FISICO: el trafico de memoria que no se puede evitar, decodifique quien decodifique.
# Son DOS cosas y las dos hay que pagarlas:
#   · ESCRIBIR los bytes de salida. Hay que producirlos igual.
#   · RELEER los bytes que vienen de coincidencias, que salen del propio buffer ya escrito.
# Contar solo la escritura da un suelo mas bajo del real y una distancia inflada.
t_escribir = SAL / (escr * 1e9)
t_releer   = COP / (copi * 1e9)
suelo = t_escribir + t_releer
com = subprocess.run(["git","rev-parse","--short","HEAD"], capture_output=True, text=True).stdout.strip()
filas = [
 ("ancho_escribir_gb_s",  f"{escr:.2f}",        "memset de 512 MiB x4, medido aqui mismo"),
 ("ancho_copiar_gb_s",    f"{copi:.2f}",        "memcpy de 512 MiB x4"),
 ("simbolos",             str(SIM),             "contados por el propio vc-inflate, no estimados"),
 ("salida_bytes",         str(SAL),             "plano producido por los 1.973 flujos"),
 ("bytes_copiados",       str(COP),             "los que vienen de coincidencias: se releen del buffer"),
 ("copias",               str(NCOP),            "tokens de copia"),
 ("suelo_escribir_seg",   f"{t_escribir:.2f}",  "SAL / ancho de escritura"),
 ("suelo_releer_seg",     f"{t_releer:.2f}",    "bytes_copiados / ancho de copia"),
 ("ciclos",               str(cic),             "perf cycles:u"),
 ("instrucciones",        str(ins),             "perf instructions:u"),
 ("ipc",                  f"{ins/cic:.3f}",     "instrucciones por ciclo"),
 ("ciclos_por_simbolo",   f"{cic/SIM:.1f}",     "el numero que hay que bajar"),
 ("instr_por_simbolo",    f"{ins/SIM:.1f}",     ""),
 ("nuestro_seg",          f"{seg:.2f}",         "reloj de vc-inflate sobre el banco entero"),
 ("suelo_fisico_seg",     f"{suelo:.2f}",       "escribir la salida MAS releer las coincidencias"),
 ("distancia_al_suelo_x", f"{seg/suelo:.1f}",   "cuantas veces por encima del minimo fisico"),
 ("commit",               com,                  ""),
 ("fecha",                datetime.datetime.now().strftime("%Y-%m-%d %H:%M"), ""),
]
with open("bench/suelo.tsv", "w", newline="") as f:
    f.write("# EL SUELO FISICO DE VDEFLATE, medido en esta maquina. Lo escribe bench/suelo.sh.\n")
    f.write("# NO SE EDITA A MANO: es el arreglo del hallazgo P10 —estas cifras vivian copiadas\n")
    f.write("# dentro de bench/vdeflate.py y caducaron dos veces el mismo dia—. Quien mide\n")
    f.write("# escribe; quien informa lee.\n")
    w = csv.writer(f, delimiter="\t"); w.writerow(["clave","valor","de_donde"]); w.writerows(filas)
print()
print(f"  reloj {seg:.2f} s · suelo fisico {suelo:.2f} s · estamos {seg/suelo:.1f}x por encima")
print("  escrito bench/suelo.tsv")
PY
else
  echo "  no hay perf sin privilegios; se mide solo el reloj"
  ./target/release/vc-inflate 2>&1 | grep "MiB comprimidos" | sed 's/^/  /'
fi
echo
echo "EL SUELO, y de donde sale cada parte:"
echo "  hay que ESCRIBIR $SAL B de salida y RELEER $COP B de coincidencias."
echo "  Eso es el trafico de memoria que no se puede evitar decodifique quien decodifique."
echo "  Todo lo que tardemos por encima es margen nuestro, y esta en bench/suelo.tsv."
echo FIN-SUELO
