#!/usr/bin/env bash
# ¿A partir de que tamaño el archivo deja de ser MAS GRANDE que la entrada?
#
# Salio de una prueba final que dio "ratio 2675%": dos ficheros minusculos producian un archivo
# trece veces mayor. Es esperable -cabecera, indice y pie cuestan lo que cuestan- pero nadie lo
# habia medido ni decidido si avisar. Un archivador que infla y no lo dice es exactamente lo que
# llevo dos dias cazando en otros sitios.
#
# Se mide con TRES clases de dato porque el punto de cruce no puede ser el mismo:
#   texto        se comprime mucho: cruza pronto
#   aleatorio    NO se comprime: es el caso peor y el que marca el suelo real
#   muchos ficheros pequeños  el indice crece con el NUMERO, no con los bytes
set -u
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.."
VC=$PWD/target/release/vcompresor
W=/tmp/cruce; rm -rf $W; mkdir -p $W

echo "=== 1. UN fichero, tamaños crecientes ==="
printf '%-12s %12s %12s %10s %12s %10s\n' clase entrada archivo ratio "sobrecoste" cruzado
for clase in texto aleatorio; do
  for n in 0 100 1000 10000 100000 1000000 10000000; do
    rm -rf $W/in; mkdir -p $W/in
    if [ "$clase" = "texto" ]; then
      [ $n -gt 0 ] && head -c $n /tmp/compresor-f1/samples/codigo.10m > $W/in/f.bin || : > $W/in/f.bin
    else
      [ $n -gt 0 ] && head -c $n /dev/urandom > $W/in/f.bin || : > $W/in/f.bin
    fi
    rm -f $W/a.vc
    $VC compress $W/in -o $W/a.vc --level 19 >/dev/null 2>&1
    A=$(stat -c%s $W/a.vc 2>/dev/null || echo 0)
    R=$(echo "scale=1; if($n>0) $A*100/$n else 0" | bc)
    printf '%-12s %12s %12s %9s%% %12s %10s\n' "$clase" "$n" "$A" "$R" "$((A-n))" \
      "$([ "$A" -le "$n" ] && echo SI || echo no)"
  done
  echo
done

echo "=== 2. MUCHOS ficheros pequeños: el indice crece con el NUMERO ==="
printf '%-12s %12s %12s %10s %12s\n' ficheros entrada archivo ratio "B/fichero"
for k in 1 10 100 1000 10000; do
  rm -rf $W/in; mkdir -p $W/in
  for i in $(seq 1 $k); do echo "linea de prueba numero $i" > $W/in/f$i.txt; done
  IN=$(du -sb $W/in | cut -f1)
  rm -f $W/a.vc; $VC compress $W/in -o $W/a.vc --level 19 >/dev/null 2>&1
  A=$(stat -c%s $W/a.vc)
  printf '%-12s %12s %12s %9s%% %12s\n' "$k" "$IN" "$A" \
    "$(echo "scale=1;$A*100/$IN"|bc)" "$(echo "scale=1;$A/$k"|bc)"
done

echo
echo "=== 3. el SUELO: que cuesta un archivo vacio ==="
rm -rf $W/in; mkdir -p $W/in
$VC compress $W/in -o $W/v.vc --level 19 >/dev/null 2>&1
echo "  arbol vacio -> $(stat -c%s $W/v.vc 2>/dev/null || echo 'error') B"
rm -rf $W
echo FIN-CRUCE-CENTINELA
