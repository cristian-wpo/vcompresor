# zprobe-ncomp

Sonda de solo lectura citada en `docs/f4-velocidad-por-que-son-mas-rapidos.md` (sección
"CORRECCIÓN DE LIBRO MAYOR"). Cuenta los componentes que `libzpaq::makeConfig()` genera de verdad para
`-m5` sobre un fichero de entrada real, en vez de leer un número fijo del código.

No reescribe ni modifica `libzpaq.cpp`/`libzpaq.h` (fuente en `~/fuentes-rivales/zpaq715`,
citada también en `docs/f4-velocidad-por-que-son-mas-rapidos.md:11`): `main.cpp` sólo copia
literalmente el bloque de `compressBlock` (nivel 5, detección de periodicidad) y enlaza contra
la `makeConfig()` real.

Build (fuera del workspace de cargo, no vendoreado):

```
g++ -std=c++17 -O2 -Dunix -c ~/fuentes-rivales/zpaq715/libzpaq.cpp -o libzpaq.o
g++ -std=c++17 -O2 -c main.cpp -I ~/fuentes-rivales/zpaq715 -o main.o
g++ main.o libzpaq.o -o probe
./probe /tmp/compresor-f1/samples/codigo.10m
./probe /tmp/compresor-f1/samples/sql.10m
```

`codigo.10m.txt` y `sql.10m.txt` son la salida capturada (stderr+stdout) de esas dos corridas,
commit `1d7f55d`. Las líneas de componentes 0..17 (18 en total) son el predictor antes de la
etapa de mezcla en `codigo.10m`; 0..25 (26 en total) en `sql.10m` — de ahí el 18/26 citado en
`docs/f4-velocidad-por-que-son-mas-rapidos.md`.
