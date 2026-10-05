# Muestras de preflate-rs 0.7.6

45 ficheros (10,2 MB) traídos de `https://github.com/microsoft/preflate-rs`, etiqueta **v0.7.6**,
que es exactamente la versión vendorizada en [`../preflate/`](../preflate/PROCEDENCIA.md).
Copyright (c) Microsoft Corporation, **Apache-2.0** — la misma licencia y el mismo `LICENSE` que
el código, en `crates/preflate/LICENSE`.

## Por qué están aquí

Al vendorizar `crates/preflate/` se trajo el código y **no los datos de prueba**, y su suite los
lee por ruta relativa (`crates/preflate/src/utils.rs`: `CARGO_MANIFEST_DIR/../samples/`). El
resultado era que **26 de las 49 pruebas de preflate reventaban en 0,00 s** con
`Os { code: 2, kind: NotFound }` — no fallaban: ni llegaban a correr. Una suite roja permanente
deja de mirarse, y con ella se pierde la única red que tenemos bajo el motor que nos da el 39,8%
sobre 7-Zip en PNG. Con las muestras en su sitio: **49 pasan, 0 fallan**.

## Qué se trajo y qué no

- **Sí**: los 44 `.deflate` y `sample1.bin.gz`. Los `.deflate` no se citan solo por nombre — 
  `test_on_all_deflate_files` **recorre el directorio entero**, así que traer un subconjunto
  habría dejado la suite verde probando menos de lo que dice probar.
- **No**: `compressed2.jpg`, `compressed2.jpgoutput` y `compressed2.lep` (13,9 MB). Son del
  carril JPEG/lepton y no los toca ninguna de las pruebas que corren aquí.

Se reproduce con `bench/trae-muestras.sh`, que comprueba el tamaño de cada fichero contra el que
declara la API de GitHub antes de escribirlo.
